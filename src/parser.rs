use crate::debug::DebugLevel;
use crate::grammar::{Factor, Grammar, Mark, Rule, TMark, TerminalDefn};
use crate::utils;
use crate::EarleyStr;
use crate::{debug_earley_fail, debug_earley_pos};
use indextree::{Arena, NodeId};
use log::{debug, info, trace};
use multimap::MultiMap;
use std::rc::Rc;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    hash::{Hash, Hasher},
};

const DOTSEP: &str = "•";

/// Parse session state - contains per-parse mutable state including statistics and progress tracking
#[derive(Debug)]
pub(crate) struct ParseSession {
    /// Total length of input being parsed
    pub input_length: usize,
    /// Track operations at each position for infinite loop detection
    pub position_repeat_count: HashMap<usize, u32>,
    /// Total operations performed during this parse
    pub total_operations: u32,
    /// Furthest position reached in input (for debugging)
    pub farthest_pos: usize,
    /// Maximum queue size reached during parsing
    pub max_queue_size: usize,
    /// Maximum operations allowed at any single position before detecting infinite loop
    pub infinite_loop_threshold: u32,
    /// Per-phase wall-time in nanoseconds for the parse loop arms, indexed by
    /// [complete, predict, scan, insertion]. Only populated when phase reporting is on.
    pub phase_ns: [u128; 4],
    /// Per-phase invocation counts, same index order as `phase_ns`.
    pub phase_calls: [u64; 4],
    /// Wall-time in nanoseconds for tree extraction (`unpack_parse_tree`).
    pub unpack_ns: u128,
    /// Allocation-counter snapshot taken at the start of the parse, as
    /// `[allocs, deallocs, reallocs, bytes]`. Only captured when phase reporting
    /// is on; the `--stats` breakdown subtracts it from a fresh snapshot to print
    /// the parse's allocation delta (no-op unless built with `--features alloc-count`).
    pub alloc_start: [u64; 4],
}

impl Default for ParseSession {
    fn default() -> Self {
        Self {
            input_length: 0,
            position_repeat_count: HashMap::new(),
            total_operations: 0,
            farthest_pos: 0,
            max_queue_size: 0,
            infinite_loop_threshold: 1000,
            phase_ns: [0; 4],
            phase_calls: [0; 4],
            unpack_ns: 0,
            alloc_start: [0; 4],
        }
    }
}

impl ParseSession {
    /// Increment operation count at given position and return new count
    pub fn increment_position(&mut self, pos: usize) -> u32 {
        let count = self.position_repeat_count.entry(pos).or_insert(0);
        *count += 1;
        *count
    }

    /// Reset position tracking when advancing to new positions
    pub fn reset_position_tracking(&mut self) {
        self.position_repeat_count.clear();
    }

    /// Record an operation and update statistics
    pub fn record_operation(&mut self, queue_size: usize) {
        self.total_operations += 1;
        self.max_queue_size = self.max_queue_size.max(queue_size);
    }
}

impl fmt::Display for ParseSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ParseSession {{ ops: {}, pos: {}/{}, max_queue: {}, hotspots: [{}] }}",
            self.total_operations,
            self.farthest_pos,
            self.input_length,
            self.max_queue_size,
            self.position_repeat_count
                .iter()
                .filter(|(_, &count)| count > 10) // Show positions with many operations
                .map(|(pos, count)| format!("{}:{}", pos, count))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

/// One edge of a [`MatchStack`], newest at the head.
#[derive(Debug, Eq, PartialEq)]
struct MatchNode {
    rec: MatchRec,
    next: Option<Rc<MatchNode>>,
}

/// Immutable, structurally-shared stack of matched edges (newest at the head).
///
/// `advance_dot` runs once per cursor advance (every scanned terminal / completed
/// child), of which there are O(n^2) on right-recursive grammars. The old
/// `Vec<MatchRec>` representation cloned the whole vector and then re-grew it
/// (one allocation + one reallocation) on each advance. Prepending a node here is
/// a single fixed-size allocation with no reallocation, and distinct continuations
/// of the same dotted item share the common prefix via `Rc`. The matched length
/// is bounded by the rule's RHS arity (small), so the O(len) accessors below are
/// cheap. `len` is cached so the cursor / `is_completed` checks stay O(1). See
/// TODO.txt "Stop cloning grammar fragments in the hot loop" (c).
#[derive(Debug, Clone, Eq, PartialEq, Default)]
struct MatchStack {
    head: Option<Rc<MatchNode>>,
    len: usize,
}

/// Iterator over a [`MatchStack`] in newest-first (head -> tail) order.
struct MatchStackIter<'a> {
    node: Option<&'a MatchNode>,
}

impl<'a> Iterator for MatchStackIter<'a> {
    type Item = &'a MatchRec;
    fn next(&mut self) -> Option<&'a MatchRec> {
        let n = self.node?;
        self.node = n.next.as_deref();
        Some(&n.rec)
    }
}

impl MatchStack {
    /// Return a new stack with `rec` prepended (newest). O(1): one node allocation
    /// plus a refcount bump on the shared tail.
    fn push(&self, rec: MatchRec) -> Self {
        Self {
            head: Some(Rc::new(MatchNode {
                rec,
                next: self.head.clone(),
            })),
            len: self.len + 1,
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    /// Iterate newest-first (head -> tail).
    fn iter_rev(&self) -> MatchStackIter<'_> {
        MatchStackIter {
            node: self.head.as_deref(),
        }
    }

    /// Borrow the matched edges oldest-first — the historical `matched_so_far`
    /// order the tree extractor walks. Collects borrows (no `MatchRec` clones) and
    /// reverses; `len` is the rule arity, so this is cheap.
    fn in_order(&self) -> Vec<&MatchRec> {
        let mut v: Vec<&MatchRec> = self.iter_rev().collect();
        v.reverse();
        v
    }

    /// The first (oldest) matched edge, i.e. the bottom of the stack. O(len).
    fn first(&self) -> Option<&MatchRec> {
        self.iter_rev().last()
    }
}

// Hash the logical sequence (length + newest-first edges) so equal stacks hash
// equal. The absolute value is irrelevant — family signatures are only compared
// within a single parse — but it must be consistent with the derived `Eq`.
impl std::hash::Hash for MatchStack {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.len.hash(state);
        for rec in self.iter_rev() {
            rec.hash(state);
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
/// A sort of iterator for a Rule.
/// Instead of just calling next(), For completed terms, it tracks positions and specifically-matched chars
/// `matched_so_far.len`() is the cursor position
pub(crate) struct DotNotation {
    /// The rule being matched. Shared via `Rc` so `advance_dot` (called once per cursor
    /// advance, i.e. per scanned terminal / completed child) is a refcount bump rather
    /// than a deep clone of the whole `Vec<Factor>` — see TODO.txt "Stop cloning grammar
    /// fragments in the hot loop".
    iteratee: Rc<Rule>,
    /// Edges matched so far, as an immutable shared stack (see [`MatchStack`]) so
    /// each advance is one node allocation, not a `Vec` clone + regrow.
    matched_so_far: MatchStack,
}

impl DotNotation {
    /// Build a fresh dot notator that shares the grammar-owned `Rc<Rule>`. Cloning the
    /// `Rc` is a refcount bump, so predicting an alternative no longer deep-clones its
    /// `Vec<Factor>` — see TODO.txt "Stop cloning grammar fragments in the hot loop".
    pub fn new(rule: Rc<Rule>) -> Self {
        Self {
            iteratee: rule,
            matched_so_far: MatchStack::default(),
        }
    }

    /// record a new match. Intnded for literal character data
    /// this returns an entirely new `DotNotation`
    fn advance_dot(&self, rec: MatchRec) -> Self {
        // Bump the rule `Rc` and prepend one shared `MatchStack` node — no `Vec`
        // clone or regrow; the matched prefix is shared with sibling continuations.
        Self {
            iteratee: Rc::clone(&self.iteratee),
            matched_so_far: self.matched_so_far.push(rec),
        }
    }

    fn is_completed(&self) -> bool {
        self.iteratee.len() == self.matched_so_far.len()
    }

    fn _is_at_start(&self) -> bool {
        self.matched_so_far.len() == 0
    }

    /// retrieve the match info (oldest-first) for trace processing
    fn matches_in_order(&self) -> Vec<&MatchRec> {
        self.matched_so_far.in_order()
    }

    /// the first (oldest) matched edge, if any
    fn first_match(&self) -> Option<&MatchRec> {
        self.matched_so_far.first()
    }

    /// next term to parse. A.k.a. "What's next after the dot?"
    /// returns cloned Term
    fn next_unparsed(&self) -> Factor {
        let cursor = self.matched_so_far.len();
        self.iteratee.factors[cursor].clone()
    }

    /// Borrow the next term to parse without cloning it. Returns the shared
    /// `Rc<Rule>` (a refcount bump) plus the cursor, so the caller can hold
    /// `&rule.factors[cursor]` independently of the `TraceArena` borrow. This lets
    /// the hot dispatch loop reach the `&mut self` scan/predict paths without
    /// cloning a `Factor` per pop — and, for terminals, without cloning a heap
    /// `TerminalDefn { Vec<CharMatcher> }`. See TODO.txt "Stop cloning grammar
    /// fragments in the hot loop" (b).
    fn next_unparsed_shared(&self) -> (Rc<Rule>, usize) {
        (Rc::clone(&self.iteratee), self.matched_so_far.len())
    }
}

impl fmt::Display for DotNotation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let cursor = self.matched_so_far.len();

        // handled rules
        let done: String = self
            .matched_so_far
            .in_order()
            .into_iter()
            .map(|i| match i {
                MatchRec::Term(ch, pos, tmark) => format!("{tmark}'{ch}'@{pos}"),
                MatchRec::NonTerm(name, pos, mark, alias) => {
                    if let Some(alias) = alias {
                        format!("{mark}{name}>{alias}@{pos}")
                    } else {
                        format!("{mark}{name}@{pos}")
                    }
                }
                MatchRec::Insertion(pos, text, tmark) => format!("{tmark}+\"{text}\"@{pos}"),
            })
            .collect::<Vec<_>>()
            .join(", ");

        // remaining rules
        let remain = self
            .iteratee
            .factors
            .iter()
            .skip(cursor)
            .map(std::string::ToString::to_string)
            .collect::<Vec<String>>()
            .join(", ");
        write!(f, "{done} {DOTSEP} {remain}")
    }
}

/// The internal record of a fragment of a matching parse
/// See also the Content enum for the stable, outward facing record of a similar nature
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum MatchRec {
    Term(char, usize, TMark),
    NonTerm(EarleyStr, usize, Mark, Option<EarleyStr>),
    /// Insertion: text inserted without consuming input (position, text, mark)
    Insertion(usize, EarleyStr, TMark),
}

impl MatchRec {
    fn pos(&self) -> usize {
        match self {
            Self::Term(_, pos, _) => *pos,
            Self::NonTerm(_, pos, _, _) => *pos,
            Self::Insertion(pos, _, _) => *pos,
        }
    }
}

/// Identity of one derivation family of a completed item: its alternative plus the
/// exact sequence of child edges (`matched_so_far`). Folding in `alt_index` makes two
/// *different alternatives* completing the same span distinct families (Mechanism A),
/// while differing child split points make same-alt derivations distinct (Mechanism B).
fn family_signature(alt_index: usize, dot: &DotNotation) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    alt_index.hash(&mut hasher);
    dot.matched_so_far.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Task {
    id: TraceId,      // unique id, as handled by TraceArena
    name: EarleyStr,  // BranchingRule name
    alt_index: usize, // which alt of this BranchingRule (0-based)
    mark: Mark,       // effective mark for this task
    alias: Option<EarleyStr>,
    origin: usize,    // starting position in the input
    pos: usize,       // current position in the input
    dot: DotNotation, // progress
    hash: u64,        // identity hash based on name, alt_index, origin, pos, dot
}

/// Display task content for debugging
impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}{}[{}] {}:{} {}",
            self.mark, self.name, self.alt_index, self.origin, self.pos, self.dot
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TraceId(usize);

#[derive(Debug)]
/// Position-bucketed queue for proper Earley left-to-right processing.
/// Ensures all tasks at position N are completed before advancing to N+1.
/// Within each position, maintains front/back priority for predictions vs completions.
pub(crate) struct PositionBucketedQueue {
    /// Map from position to tasks at that position
    buckets: std::collections::BTreeMap<usize, VecDeque<TraceId>>,
    /// Current position being processed
    current_position: usize,
}

impl PositionBucketedQueue {
    pub fn new() -> Self {
        Self {
            buckets: std::collections::BTreeMap::new(),
            current_position: 0,
        }
    }

    /// Add task to front of its position bucket (high priority - predictions)
    pub fn push_front(&mut self, task_id: TraceId, position: usize) {
        self.buckets
            .entry(position)
            .or_default()
            .push_front(task_id);
    }

    /// Add task to back of its position bucket (normal priority - completions, scanning)
    pub fn push_back(&mut self, task_id: TraceId, position: usize) {
        self.buckets.entry(position).or_default().push_back(task_id);
    }

    /// Get next task, advancing position when current bucket is empty
    pub fn pop_front(&mut self) -> Option<TraceId> {
        loop {
            if let Some(bucket) = self.buckets.get_mut(&self.current_position) {
                if let Some(task_id) = bucket.pop_front() {
                    return Some(task_id);
                }
                // Current bucket is empty, remove it and advance position
                self.buckets.remove(&self.current_position);
            }

            // Find next non-empty position
            if let Some((&next_pos, _)) = self.buckets.range(self.current_position..).next() {
                self.current_position = next_pos;
            } else {
                // No more tasks
                return None;
            }
        }
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    /// Get total number of tasks across all buckets
    pub fn len(&self) -> usize {
        self.buckets.values().map(|bucket| bucket.len()).sum()
    }

    /// Iterate over all tasks in queue order (by position, then by priority within position)
    pub fn iter(&self) -> impl Iterator<Item = &TraceId> {
        self.buckets.values().flat_map(|bucket| bucket.iter())
    }
}

impl std::fmt::Display for PositionBucketedQueue {
    /// Format as "S(0):3 S(1):7 S(2):1" showing position buckets with task counts
    /// Current position marked with * like "S(1):7*"
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.buckets.is_empty() {
            write!(f, "empty")
        } else {
            let bucket_strs: Vec<String> = self
                .buckets
                .iter()
                .map(|(pos, bucket)| {
                    format!(
                        "S({}):{}{}",
                        pos,
                        bucket.len(),
                        if *pos == self.current_position {
                            "*"
                        } else {
                            ""
                        }
                    )
                })
                .collect();
            write!(f, "{}", bucket_strs.join(" "))
        }
    }
}

#[derive(Debug)]
/// the permanent home of all Traces/Tasks
pub(crate) struct TraceArena {
    /// main storage for Tasks. The vector index becomes the TraceId
    /// (which should always match what's stored in task.id)
    arena: Vec<Task>,

    /// active queue of tasks, bucketed by position for proper Earley ordering
    queue: PositionBucketedQueue,

    /// Track every place where a nonterminal can be triggered, indexed by the
    /// Earley set it was predicted in. Key is `(nonterminal name, origin position)`;
    /// value is a TraceId of a parent task whose dot sits just before that
    /// nonterminal at that position. This is the classic Earley-set indexing: a
    /// completion of `name` spanning `origin..pos` only needs to resume parents that
    /// predicted `name` at `origin`, i.e. parents keyed by `(name, origin)`. Keying
    /// by name alone (no position) would force every completion to scan every parent
    /// that ever referenced `name` anywhere in the input — O(n^2)+ on pervasive
    /// nonterminals.
    /// For example in
    /// doc = S.
    /// S = S, "+", T | T
    /// when predicting S at position 0 we record
    /// ("S", 0) -> (TraceId for doc=(• S))
    /// ("S", 0) -> (TraceId for S=(• S "+" T))
    /// so completing an "S" that started at 0 resumes exactly those two parents.
    continuations: MultiMap<(EarleyStr, usize), TraceId>,

    /// Map Earley item identity to its stored task for fast deduplication.
    task_by_hash: HashMap<u64, TraceId>,

    /// For each fully-completed item span (name, origin, pos), the set of distinct
    /// derivation-family signatures `hash(alt_index, matched_so_far)` that produced it.
    /// Two or more entries at a span reachable from the accepting root means there are
    /// two genuinely distinct derivations of that span — i.e. structural ambiguity.
    families: HashMap<(EarleyStr, usize, usize), HashSet<u64>>,

    /// Count of tasks that were deduplicated
    pub deduplicated_count: u32,
}

impl TraceArena {
    fn new() -> Self {
        Self {
            arena: Vec::new(),
            queue: PositionBucketedQueue::new(),
            continuations: MultiMap::new(),
            task_by_hash: HashMap::new(),
            families: HashMap::new(),
            deduplicated_count: 0,
        }
    }

    fn get(&self, id: TraceId) -> &Task {
        let TraceId(n) = id;
        &self.arena[n]
    }

    /// store the immutable task. Takes ownership
    fn save_task(&mut self, task: Task) {
        assert_eq!(task.id.0, self.arena.len());
        self.arena.push(task);
    }

    /// Record one derivation family for a fully-completed item span. Called at every
    /// site that advances a dot to completion — including the deduplicated path — so a
    /// second derivation reaching the same Earley item still registers its (different)
    /// family. Idempotent: re-recording an identical derivation is a no-op.
    fn record_family(
        &mut self,
        name: &EarleyStr,
        origin: usize,
        pos: usize,
        alt_index: usize,
        dot: &DotNotation,
    ) {
        let sig = family_signature(alt_index, dot);
        self.families
            .entry((name.clone(), origin, pos))
            .or_default()
            .insert(sig);
    }

    /// Register a parent task that's waiting for a nonterminal predicted at `position`
    /// to complete. Indexed by `(name, position)` so that only completions spanning
    /// `position..` resume this parent — see the `continuations` field doc.
    fn register_waiting_parent_task(
        &mut self,
        target_nt: &str,
        position: usize,
        waiting_parent_tid: TraceId,
    ) {
        debug!(
            "..⏸️ registering parent {} waiting for {} at {}",
            self.format_task(waiting_parent_tid),
            target_nt,
            position
        );
        self.continuations
            .insert((EarleyStr::from(target_nt), position), waiting_parent_tid);
    }

    /// Fill `out` with the parent tasks waiting for a completion of `rule_name` that
    /// started at `origin`. These are exactly the parents whose dot is before
    /// `rule_name` at `origin`; the Earley-set key makes the old `pos == origin`
    /// filter unnecessary. `out` is cleared first; the caller passes a reusable
    /// buffer so the per-`complete` parent list does not allocate a fresh `Vec`.
    fn collect_waiting_parents(&self, rule_name: &str, origin: usize, out: &mut Vec<TraceId>) {
        out.clear();
        if let Some(parents) = self
            .continuations
            .get_vec(&(EarleyStr::from(rule_name), origin))
        {
            out.extend_from_slice(parents);
        }
        debug!(
            "..🔁 found {} parent tasks waiting for {} at {}",
            out.len(),
            rule_name,
            origin
        );
    }

    /// originate a completely new task (root level)
    /// Returns Some(TraceId) (unless this is a duplicate Task, in which case None is returned)
    #[allow(clippy::too_many_arguments)] // an Earley item is intrinsically this wide
    fn task(
        &mut self,
        name: &str,
        alt_index: usize,
        mark: Mark,
        alias: Option<EarleyStr>,
        origin: usize,
        pos: usize,
        dot: DotNotation,
    ) -> Option<TraceId> {
        let id = TraceId(self.arena.len());

        // Compute hash without string allocation - hash the tuple of key identity fields
        // Use dot cursor position (matched_so_far.len()) instead of full DotNotation to avoid deep hashing
        let dot_cursor = dot.matched_so_far.len();
        let hash = utils::hash_to_u64(&(name, alt_index, origin, pos, dot_cursor));

        let task = Task {
            id,
            name: EarleyStr::new(name),
            alt_index,
            mark,
            alias,
            origin,
            pos,
            dot,
            hash,
        };

        if self.have_we_seen(&task) {
            None
        } else {
            self.save_task(task);
            Some(id)
        }
    }

    /// clone a task, except advancing the cursor (storing given `MatchRec` for the piece just advanced-over)
    /// Maintains the same parentage, and position
    fn task_advance_cursor(&mut self, from: TraceId, rec: MatchRec) -> Option<TraceId> {
        let new_pos = rec.pos();

        let from_task = self.get(from);
        let new_dot = from_task.dot.advance_dot(rec);

        // If this advance completes the rule, capture its derivation family now —
        // before the dedup short-circuit below — so a second derivation that reaches
        // the same Earley item (and gets deduplicated) still registers its family.
        let completed_family = if new_dot.is_completed() {
            Some((
                (from_task.name.clone(), from_task.origin, new_pos),
                family_signature(from_task.alt_index, &new_dot),
            ))
        } else {
            None
        };

        let id = TraceId(self.arena.len());

        // Compute hash without string allocation - hash the tuple of key identity fields
        // Use dot cursor position (matched_so_far.len()) instead of full DotNotation to avoid deep hashing
        let dot_cursor = new_dot.matched_so_far.len();
        let hash = utils::hash_to_u64(&(
            &from_task.name,
            from_task.alt_index,
            from_task.origin,
            new_pos,
            dot_cursor,
        ));

        let task = Task {
            id,
            name: from_task.name.clone(),
            alt_index: from_task.alt_index, // Preserve alt_index from source task
            mark: from_task.mark,
            alias: from_task.alias.clone(),
            origin: from_task.origin,
            pos: new_pos,
            dot: new_dot,
            hash,
        };

        let result = if self.have_we_seen(&task) {
            None
        } else {
            self.save_task(task);
            Some(id)
        };

        if let Some((key, sig)) = completed_family {
            self.families.entry(key).or_default().insert(sig);
        }

        result
    }

    /// returns true if this trace had been previously seen
    /// also performs necessary bookkeeping
    ///
    /// Simple Task Deduplication Strategy:
    /// Use task identity hash based on name, alt_index, origin, pos, and dot
    fn have_we_seen(&mut self, task: &Task) -> bool {
        if let std::collections::hash_map::Entry::Vacant(e) = self.task_by_hash.entry(task.hash) {
            debug!("...caching task {}[{}]", task.name, task.alt_index);
            e.insert(task.id);
            false
        } else {
            debug!(
                "🚫 DUPLICATE TASK DETECTED: Skipping {}[{}]",
                task.name, task.alt_index
            );
            self.deduplicated_count += 1;
            true
        }
    }

    fn format_task(&self, id: TraceId) -> String {
        let task = self.get(id);
        let printable_id: String = id.0.to_string();
        format!(
            " {}) {}:{}👉 {}[{}]=( {} ) ",
            printable_id, task.origin, task.pos, task.name, task.alt_index, task.dot
        )
    }
}

struct InputIter {
    tokens: Vec<char>,
    // actual position is tracked externally, in Tasks
}

impl InputIter {
    fn new(input: &str) -> Self {
        Self {
            tokens: input.chars().collect::<Vec<_>>(),
        }
    }

    /// Get the character immediately after the given cursor position
    /// Cursor 0 is before the first character, so get_at(0) returns the first character
    /// Cursor 1 is before the second character, so get_at(1) returns the second character
    /// Like graphics coordinates: cursors are between characters, not at characters
    /// Panics if cursor position is beyond input length
    pub fn get_at(&mut self, cursor: usize) -> char {
        if cursor >= self.tokens.len() {
            panic!(
                "Parser attempted to read beyond input at cursor {}, input length is {}",
                cursor,
                self.tokens.len()
            );
        } else {
            self.tokens[cursor]
        }
    }
    // TODO: row/col machinery for input tokens
}

#[derive(Debug, Clone)]
/// in the intermediate parse indextree, tree nodes are provided thusly
// implementation detail of the build phase; see ARCHITECTURE.md ADR 1
pub(crate) enum Content {
    Root,
    Element(String),           // name
    Attribute(String, String), // name, value
    Text(String),              // value
}

impl Content {
    pub fn is_attr(&self) -> bool {
        matches!(self, Self::Attribute(_, _))
    }
    pub fn get_name(&self) -> Option<String> {
        match self {
            Self::Element(name) => Some(name.clone()),
            Self::Attribute(name, _) => Some(name.clone()),
            _ => None,
        }
    }
    pub fn get_value(&self) -> Option<String> {
        match self {
            Self::Attribute(_, value) => Some(value.clone()),
            Self::Text(value) => Some(value.clone()),
            _ => None,
        }
    }
    pub fn set_value(&mut self, value: String) {
        match self {
            Self::Attribute(name, _) => *self = Self::Attribute(name.clone(), value),
            Self::Text(_) => *self = Self::Text(value),
            _ => panic!("Setting value on content that cannot hold a value"),
        }
    }
}

/// A byte-offset span into the source an error refers to.
///
/// Offsets index the UTF-8 grammar source (for [`ErrorKind::Static`]) or the
/// input being parsed (for [`ErrorKind::Dynamic`]). Line/column are a
/// presentation concern and are deliberately not stored — derive them from the
/// source when rendering. Most errors do not yet carry a span (`None`); the
/// field is part of the public surface so positions can be populated later
/// without a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// The ixml error class. The spec distinguishes *static* errors (detectable
/// from the grammar alone, before any input is parsed) from *dynamic* errors
/// (detectable only while/after parsing a specific input). `Internal` covers
/// parser-invariant violations that should never fire for a well-formed
/// grammar + input — they indicate a bug in earleybird, not user error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    Static,
    Dynamic,
    Internal,
}

/// A specific ixml specification error code emitted by this implementation.
///
/// Only the codes earleybird actually produces are listed. The enum is
/// `#[non_exhaustive]` so codes added by spec errata (or newly implemented
/// static checks) can be introduced without a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// nonterminal used but not defined
    S02,
    /// more than one rule for a nonterminal
    S03,
    /// invalid hexadecimal value
    S06,
    /// hex value outside the Unicode code-point range
    S07,
    /// hex value denotes a surrogate or noncharacter code point
    S08,
    /// reversed character range (first code point greater than second)
    S09,
    /// unknown Unicode character category
    S10,
    /// duplicate attribute on an element
    D02,
    /// name is not a well-formed XML name
    D03,
    /// character is not permitted in XML
    D04,
    /// attribute at the root of the document
    D05,
    /// parse tree does not have exactly one top-level element
    D06,
    /// reserved attribute name (`xmlns`)
    D07,
}

impl ErrorCode {
    /// The canonical spec code string, e.g. `"S03"`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::S02 => "S02",
            Self::S03 => "S03",
            Self::S06 => "S06",
            Self::S07 => "S07",
            Self::S08 => "S08",
            Self::S09 => "S09",
            Self::S10 => "S10",
            Self::D02 => "D02",
            Self::D03 => "D03",
            Self::D04 => "D04",
            Self::D05 => "D05",
            Self::D06 => "D06",
            Self::D07 => "D07",
        }
    }

    /// The error class this code belongs to.
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::S02 | Self::S03 | Self::S06 | Self::S07 | Self::S08 | Self::S09 | Self::S10 => {
                ErrorKind::Static
            }
            Self::D02 | Self::D03 | Self::D04 | Self::D05 | Self::D06 | Self::D07 => {
                ErrorKind::Dynamic
            }
        }
    }

    /// The process exit code for this error: `100 + N` for a static code `SN`,
    /// `200 + N` for a dynamic code `DN` (e.g. `S03 -> 103`, `D02 -> 202`).
    /// Derived from [`ErrorCode::as_str`] so it cannot drift from the code name.
    /// See [`ParseError::exit_code`] for the non-coded cases.
    pub fn exit_code(&self) -> u8 {
        let s = self.as_str();
        let base = if s.starts_with('S') { 100 } else { 200 };
        let n: u8 = s[1..]
            .parse()
            .expect("ErrorCode::as_str has a numeric suffix");
        base + n
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        f.write_str(self.as_str())
    }
}

/// An error produced while compiling a grammar or parsing input against it.
///
/// Construct with [`ParseError::coded`] (a spec-coded error), [`ParseError::static_err`]
/// (a grammar error with no specific spec code), or [`ParseError::internal`] (a
/// parser-invariant violation). Inspect with [`ParseError::kind`],
/// [`ParseError::code`], [`ParseError::message`], and [`ParseError::span`].
///
/// `Display` renders `"<code>: <message>"` when a code is present, otherwise the
/// bare message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    kind: ErrorKind,
    code: Option<ErrorCode>,
    message: String,
    span: Option<Span>,
}

impl ParseError {
    /// A spec-coded error. The [`ErrorKind`] is derived from the code's class.
    pub fn coded(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            kind: code.kind(),
            code: Some(code),
            message: message.into(),
            span: None,
        }
    }

    /// A static (grammar) error with no specific spec code.
    pub fn static_err(message: &str) -> Self {
        Self {
            kind: ErrorKind::Static,
            code: None,
            message: message.to_string(),
            span: None,
        }
    }

    /// An internal parser-invariant violation: a bug in earleybird, not a user
    /// error. Replaces the former `panic!("INTERNAL ERROR…")` arms.
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            code: None,
            message: message.into(),
            span: None,
        }
    }

    /// Attach a source span (builder-style).
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    /// The error class (static / dynamic / internal).
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// The spec error code, when one applies.
    pub fn code(&self) -> Option<ErrorCode> {
        self.code
    }

    /// The human-readable message, without any code prefix.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The source span, when known.
    pub fn span(&self) -> Option<Span> {
        self.span
    }

    /// A process exit code for this error, suitable for `std::process::exit`.
    ///
    /// - A spec-coded error uses [`ErrorCode::exit_code`]: `100 + N` for static
    ///   `SN`, `200 + N` for dynamic `DN` (so the class is the hundreds digit
    ///   and the code is recoverable by subtraction).
    /// - An [`ErrorKind::Internal`] error (a parser bug) is `70` (sysexits
    ///   `EX_SOFTWARE`).
    /// - Any other code-less error is the generic `1`.
    ///
    /// Codes `0` (success) and `2`–`99` are left for the caller; usage / IO
    /// errors in the CLI stay at `1`.
    pub fn exit_code(&self) -> u8 {
        match self.code {
            Some(code) => code.exit_code(),
            None => match self.kind {
                ErrorKind::Internal => 70,
                _ => 1,
            },
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self.code {
            Some(code) => write!(f, "{}: {}", code.as_str(), self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug)]
pub struct Parser {
    grammar: Grammar,
    /// the permanent owner of all tasks, referenced by TraceId
    traces: TraceArena,
    completed_trace: Vec<TraceId>,
    /// `completed_trace` indexed by completed-item span `(name, origin, pos)`, built once
    /// before tree extraction. `filter_completed_trace` is called per output-tree node;
    /// without this index it linear-scanned the whole `completed_trace` every call
    /// (O(nodes × |trace|) — the dominant real-world cost, see docs/PROFILING.md). Each
    /// bucket preserves trace insertion order so synthesized-rule first-match still holds.
    completed_by_span: HashMap<(EarleyStr, usize, usize), Vec<TraceId>>,
    /// Length of the most recent input, used by is_ambiguous() to filter root-rule completions
    last_input_len: usize,
    stats_enabled: bool,
    /// When set, accumulate and print a per-phase wall-time breakdown (predict/scan/
    /// complete/insertion loop arms + tree unpack). Opt-in via the CLI `--stats` flag
    /// only — deliberately NOT enabled by `Grammar::from_ixml_str*`, so running the
    /// test suite (which builds a grammar per test) is not flooded with phase tables.
    phase_report: bool,
    /// Reusable buffer for the waiting-parent TraceIds gathered each `complete`
    /// (and nullable `predict`). The list must be detached from the `continuations`
    /// borrow before the loop mutates `self.traces`, but allocating a fresh `Vec`
    /// per `complete` (229k+ times on the unicode build) is pure churn; `mem::take`
    /// this buffer, fill it, drain it, and put it back to keep the capacity.
    /// See TODO.txt "Stop cloning grammar fragments in the hot loop" (d).
    parents_scratch: Vec<TraceId>,
}

/// Earley parser with LIFO prediction strategy and modified completion strategy
///
/// Queue Management Strategy:
/// - PREDICTOR: New nonterminal predictions go to front (queue_front) for depth-first exploration
/// - COMPLETER: Parent continuations go to back (queue_back) to ensure exhaustive alternative exploration
/// - SCANNER: Terminal matches go to back (queue_back) for breadth-first processing
/// - Main loop: Always processes from front (pop_front)
///
/// This ensures that:
/// 1. All alternatives at current position are explored before parent continuations
/// 2. New predictions are explored immediately (depth-first-like)
/// 3. Terminal scanning and continuations happen in input order
impl Parser {
    pub fn new(grammar: Grammar) -> Self {
        Self {
            grammar,
            traces: TraceArena::new(),
            completed_trace: Vec::new(),
            completed_by_span: HashMap::new(),
            last_input_len: 0,
            stats_enabled: true,
            phase_report: false,
            parents_scratch: Vec::new(),
        }
    }

    #[doc(hidden)]
    pub fn set_stats_enabled(&mut self, enabled: bool) {
        self.stats_enabled = enabled;
    }

    /// Enable the per-phase wall-time breakdown (see [`Parser::phase_report`]).
    #[doc(hidden)]
    pub fn set_phase_report(&mut self, enabled: bool) {
        self.phase_report = enabled;
    }

    /// Parse `input` and return the owned public output tree
    /// ([`crate::treebird::Document`]).
    ///
    /// This is the single structured, indextree-free entry point for consumers;
    /// the internal indextree build representation stays private behind the
    /// conversion at the boundary. Serialize the result with
    /// [`crate::treebird::Document::to_xml`] and check XML well-formedness with
    /// [`crate::treebird::Document::validate`].
    pub fn parse(&mut self, input: &str) -> Result<crate::treebird::Document, ParseError> {
        let arena = self.parse_to_arena(input)?;
        // D05: an attribute at the document root is unrepresentable in a
        // `Document` (attributes belong to elements) and would be silently
        // dropped by the converter — so it must be detected here, at the arena
        // boundary, before the `Document` is built.
        if let Some(root) = arena.iter().next() {
            let root_id = arena.get_node_id(root).expect("arena root has a node id");
            for child in root_id.children(&arena) {
                if arena.get(child).expect("child exists").get().is_attr() {
                    return Err(ParseError::coded(
                        ErrorCode::D05,
                        "attribute cannot appear at the root of an XML document",
                    ));
                }
            }
        }
        Ok(crate::treebird::Document::from_content_arena(&arena))
    }

    /// Parse `input` and return the internal indextree arena.
    ///
    /// Internal build representation (the arena and [`Content`] are build-phase
    /// mechanism — ARCHITECTURE.md ADR 1). Used by the grammar bootstrap and
    /// internal tests; consumers use [`Parser::parse`].
    pub(crate) fn parse_to_arena(&mut self, input: &str) -> Result<Arena<Content>, ParseError> {
        let mut session = ParseSession::default();
        // E003: strip UTF-8 BOM (U+FEFF) before parsing
        let input = input.trim_start_matches('\u{FEFF}');
        self.parse_with_session(input, &mut session)
    }

    /// Parse with explicit session for statistics tracking and infinite loop detection
    fn parse_with_session(
        &mut self,
        input: &str,
        session: &mut ParseSession,
    ) -> Result<Arena<Content>, ParseError> {
        let mut input = InputIter::new(input);
        session.input_length = input.tokens.len();
        self.last_input_len = session.input_length;
        // Baseline the allocation counters so `--stats` can report this parse's
        // allocation delta (only meaningful when built with `--features alloc-count`).
        if self.phase_report {
            session.alloc_start = crate::alloc_count::snapshot();
        }
        // Long comments can produce many finite completions at one position; keep
        // the small-input floor but scale enough to avoid false loop reports.
        session.infinite_loop_threshold = session
            .infinite_loop_threshold
            .max((session.input_length as u32).saturating_mul(4));

        // help avoid borrow-contention on *self
        let g = self.grammar.clone();

        debug!("Starting parse at position 0 (before first character)");

        // INITIALISE
        // START grammar FOR start.symbol grammar AT start.pos
        let top_rule = g
            .get_root_definition()?
            .ok_or(ParseError::static_err("No top grammar rule"))?;

        for (alt_index, alt) in top_rule.iter().enumerate() {
            let root_name = g
                .get_root_definition_name()
                .ok_or(ParseError::static_err("No top grammar rule name"))?;
            let maybe_id = self.traces.task(
                &root_name,
                alt_index,
                top_rule.mark(),
                g.get_definition_alias(&root_name)?,
                0,
                0,
                DotNotation::new(Rc::clone(alt)),
            );
            self.queue_front(maybe_id);
        }

        // WHILE more.tasks:
        //    TAKE task
        while let Some(tid) = self.traces.queue.pop_front() {
            let current_pos = self.traces.get(tid).pos;

            // Position-based infinite loop detection (much more efficient than total operation count)
            let repeat_count = session.increment_position(current_pos);
            if repeat_count > session.infinite_loop_threshold {
                return Err(ParseError::static_err(&format!(
                    "Parse exceeded {} operations at position {} (infinite loop detected). Session: {}",
                    session.infinite_loop_threshold, current_pos, session
                )));
            }

            // Record operation statistics
            session.record_operation(self.traces.queue.len());

            // Track progress and reset position tracking when advancing
            if current_pos > session.farthest_pos {
                if current_pos < session.input_length {
                    debug!(
                        "⏭ Advanced input to position {} (next char: '{}')",
                        current_pos,
                        input.get_at(current_pos)
                    );
                } else {
                    debug!("⏭ Advanced input to position {} (at end)", current_pos);
                }
                session.farthest_pos = current_pos;
                session.reset_position_tracking(); // Clear position repeat counts when advancing
            }
            debug!(
                "🔄 PROCESSING: Pulled from queue {} at {} | Queue size: {} -> {} | Queue: [{}]",
                self.traces.format_task(tid),
                current_pos,
                self.traces.queue.len() + 1,
                self.traces.queue.len(),
                self.queue_snapshot()
            );

            // SELECT:
            //    finished task:
            //       CONTINUE PARENTS task
            // Phase timing (opt-in via --stats); `None` => zero-cost no-op when disabled.
            let phase_timer = self.phase_report.then(std::time::Instant::now);
            if self.traces.get(tid).dot.is_completed() {
                self.complete(tid, &mut input)?;
                if let Some(t) = phase_timer {
                    session.phase_ns[0] += t.elapsed().as_nanos();
                    session.phase_calls[0] += 1;
                }
            } else {
                // ELSE:
                //    PUT next.symbol task, position task IN sym, pos
                //    SELECT:
                // Borrow the next factor through the rule's shared `Rc` instead of
                // cloning it: the factor lives inside the `Rc<Rule>`, so bumping the
                // refcount decouples the borrow from `self.traces` and lets us reach
                // the `&mut self` scan/predict paths below while holding `&factor`.
                // Avoids a per-pop `Factor` clone (heap `TerminalDefn` for terminals).
                let (rule, cursor) = self.traces.get(tid).dot.next_unparsed_shared();
                match &rule.factors[cursor] {
                    // grammar nonterminal sym:
                    //    START grammar FOR sym AT pos
                    Factor::Nonterm(mark, name, _alias) => {
                        self.predict(&g, tid, *mark, name)?;
                        if let Some(t) = phase_timer {
                            session.phase_ns[1] += t.elapsed().as_nanos();
                            session.phase_calls[1] += 1;
                        }
                    }
                    // sym starts (input, pos): \Terminal, matches
                    //    RECORD TERMINAL input FOR task
                    //    CONTINUE task AT (pos incremented (input, sym))
                    // ELSE:
                    //    PASS \Terminal, doesn't match
                    Factor::Terminal(tmark, matcher) => {
                        self.scan(tid, *tmark, matcher, &mut input, session)?;
                        if let Some(t) = phase_timer {
                            session.phase_ns[2] += t.elapsed().as_nanos();
                            session.phase_calls[2] += 1;
                        }
                    }
                    // Insertion: advance without consuming input
                    Factor::Insertion(tmark, text) => {
                        let current_pos = self.traces.get(tid).pos;
                        let match_rec = MatchRec::Insertion(current_pos, text.clone(), *tmark);
                        let maybe_id = self.traces.task_advance_cursor(tid, match_rec);
                        // Queue at front for immediate processing
                        self.queue_front(maybe_id);
                        if let Some(t) = phase_timer {
                            session.phase_ns[3] += t.elapsed().as_nanos();
                            session.phase_calls[3] += 1;
                        }
                    }
                }
            }
        }

        info!("🔚 QUEUE EMPTY: Parse loop exited with queue empty. Last position: {}, Input length: {}", session.farthest_pos, session.input_length);
        info!(
            "Finished parse with {} items in trace, {} total operations",
            self.traces.arena.len(),
            session.total_operations
        );
        let dedup_pct = if self.traces.deduplicated_count > 0 {
            (self.traces.deduplicated_count as f64
                / (self.traces.arena.len() as f64 + self.traces.deduplicated_count as f64)
                * 100.0) as u32
        } else {
            0
        };
        if self.stats_enabled {
            eprintln!(
                "📊 Parse stats: {} tasks created, {} deduplicated ({}%), {} operations, max queue: {}",
                self.traces.arena.len(),
                self.traces.deduplicated_count,
                dedup_pct,
                session.total_operations,
                session.max_queue_size
            );
        }

        // Tree extraction is the dominant cost on real grammars (see docs/PROFILING.md);
        // time it as its own major phase when phase reporting is on.
        let unpack_timer = self.phase_report.then(std::time::Instant::now);
        let result = self.unpack_parse_tree(session);
        if let Some(t) = unpack_timer {
            session.unpack_ns = t.elapsed().as_nanos();
            self.print_phase_breakdown(session);
        }
        result
    }

    /// Print the per-phase wall-time breakdown (parse-loop arms + tree unpack) to stderr.
    /// Gated by the opt-in `--stats` flag via [`Parser::set_phase_report`].
    #[allow(clippy::needless_range_loop)] // one index drives four parallel arrays
    fn print_phase_breakdown(&self, session: &ParseSession) {
        let labels = ["complete", "predict", "scan", "insert"];
        let loop_ns: u128 = session.phase_ns.iter().sum();
        let grand_ns = (loop_ns + session.unpack_ns).max(1);
        eprintln!("⏱ Phase breakdown:");
        for i in 0..4 {
            if session.phase_calls[i] == 0 {
                continue;
            }
            eprintln!(
                "   {:<9} {:>9.1} ms ({:>4.1}%)  {:>9} calls  {:>6.0} ns/call",
                labels[i],
                session.phase_ns[i] as f64 / 1e6,
                session.phase_ns[i] as f64 / grand_ns as f64 * 100.0,
                session.phase_calls[i],
                session.phase_ns[i] as f64 / session.phase_calls[i].max(1) as f64,
            );
        }
        eprintln!(
            "   {:<9} {:>9.1} ms ({:>4.1}%)  parse loop subtotal",
            "─ loop",
            loop_ns as f64 / 1e6,
            loop_ns as f64 / grand_ns as f64 * 100.0,
        );
        eprintln!(
            "   {:<9} {:>9.1} ms ({:>4.1}%)  tree extraction",
            "unpack",
            session.unpack_ns as f64 / 1e6,
            session.unpack_ns as f64 / grand_ns as f64 * 100.0,
        );
        eprintln!(
            "   {:<9} {:>9.1} ms             total (loop + unpack)",
            "═ total",
            grand_ns as f64 / 1e6,
        );
        // Allocation delta for this parse (loop + unpack). Only printed when the
        // counting allocator is installed (`--features alloc-count`); otherwise the
        // counters never move and the line would be a misleading row of zeros.
        if crate::alloc_count::enabled() {
            use crate::alloc_count::{ALLOCS_IDX, BYTES_IDX, DEALLOCS_IDX, REALLOCS_IDX};
            let now = crate::alloc_count::snapshot();
            let d = |i: usize| now[i].saturating_sub(session.alloc_start[i]);
            eprintln!(
                "   {:<9} {:>9} allocs  {:>9} reallocs  {:>9} deallocs  {:>8.2} MiB",
                "≈ alloc",
                d(ALLOCS_IDX),
                d(REALLOCS_IDX),
                d(DEALLOCS_IDX),
                d(BYTES_IDX) as f64 / (1024.0 * 1024.0),
            );
        }
    }

    /// COMPLETER: Handle completed tasks by continuing their parent tasks
    /// Implements: finished task: CONTINUE PARENTS task
    fn complete(&mut self, tid: TraceId, _input: &mut InputIter) -> Result<(), ParseError> {
        let current_pos = self.traces.get(tid).pos;
        debug!("COMPLETER pos={}", current_pos);
        debug_earley_pos!(
            DebugLevel::Trace,
            current_pos,
            "COMPLETER: {} completed",
            self.traces.format_task(tid)
        );

        // Record derivation family here to catch items that reached complete() without going
        // through task_advance_cursor (e.g., empty-alt root items created during initialization).
        // For non-empty rules this is a no-op (same sig already in the HashSet from
        // task_advance_cursor), but for truly-empty init items it's the only recording site.
        {
            let t = self.traces.get(tid);
            let key = (t.name.clone(), t.origin, t.pos);
            let sig = family_signature(t.alt_index, &t.dot);
            self.traces.families.entry(key).or_default().insert(sig);
        }

        self.completed_trace.push(tid);

        // Find "parent" states that predicted this nonterminal at our origin position;
        // the (name, origin) key already guarantees parent.pos == our origin. Use the
        // reusable scratch buffer (mem::take to detach it from `self` for the loop,
        // restore afterwards) so this does not allocate a fresh Vec per complete.
        let mut waiting_parents = std::mem::take(&mut self.parents_scratch);
        {
            let completed_task = self.traces.get(tid);
            self.traces.collect_waiting_parents(
                &completed_task.name,
                completed_task.origin,
                &mut waiting_parents,
            );
        }

        for &continue_id in &waiting_parents {
            debug!(
                "...deferring continuation Task... {}",
                self.traces.format_task(continue_id)
            );

            let now_finished_via_child = self.traces.get(continue_id).dot.next_unparsed();
            let match_rec = match now_finished_via_child {
                Factor::Nonterm(mark, name, alias) => {
                    MatchRec::NonTerm(name, self.traces.get(tid).pos, mark, alias)
                }
                Factor::Terminal(tmark, _ch) => {
                    // Should never happen: terminals are consumed by the Scanner, not here.
                    return Err(ParseError::internal(format!(
                        "complete(): parent task is waiting on terminal {tmark:?}; terminals must be handled by the scanner"
                    )));
                }
                Factor::Insertion(_tmark, text) => {
                    // Should never happen: insertions are advanced directly in the main loop.
                    return Err(ParseError::internal(format!(
                        "complete(): parent task is waiting on insertion {text:?}; insertions must be handled in the main loop"
                    )));
                }
            };
            trace!("MatchRec {:?}", &match_rec);

            // Child may have made progress; next item in parent seq needs to account for this
            let maybe_id = self.traces.task_advance_cursor(continue_id, match_rec);

            // CRITICAL FIX: Queue parent continuations at back to ensure exhaustive alternative exploration
            // This allows all alternatives at current position to be explored before parent propagation
            self.queue_back(maybe_id);
        }
        // Return the buffer (now holding this call's parents) for reuse next complete.
        self.parents_scratch = waiting_parents;
        Ok(())
    }

    /// PREDICTOR: Create downstream tasks from nonterminal references
    /// When processing a rule like "A: B, C." and we encounter nonterminal B,
    /// we create new tasks for all alternatives of B:
    /// A: • B, C. { <-- currently processing this rule }
    /// B: • "x".  { <-- queue up this alternative }
    /// B: • "y".  { <-- and this alternative }
    /// Implements: grammar nonterminal sym: START grammar FOR sym AT pos
    fn predict(
        &mut self,
        g: &Grammar,
        tid: TraceId,
        mark: Mark,
        name: &EarleyStr,
    ) -> Result<(), ParseError> {
        let current_pos = self.traces.get(tid).pos;
        debug!("PREDICTOR: Nonterm {mark}{name}");
        debug_earley_pos!(
            DebugLevel::Trace,
            current_pos,
            "PREDICTOR: {} predicting {}{}",
            self.traces.format_task(tid),
            mark,
            name
        );

        // Register this parent task as waiting for the child rule, indexed by the
        // position we're predicting at. A single entry per (name, position) suffices;
        // every completion of `name` starting here resumes this parent.
        self.traces
            .register_waiting_parent_task(name, current_pos, tid);

        // We can have a Mark at the point of definition,
        // as well as at the point of reference...
        // Figure out what to do with all possible combinations
        let defn_mark = g.get_definition_mark(name)?;
        let defn_alias = g.get_definition_alias(name)?;
        let effective_mark = match (defn_mark, mark) {
            (Mark::Default, Mark::Default) => Mark::Default,
            (Mark::Default, Mark::Mute) => Mark::Mute,
            (Mark::Default, Mark::Attr) => Mark::Attr,
            (Mark::Default, Mark::Unmute) => Mark::Unmute,
            (Mark::Mute, Mark::Default) => Mark::Mute,
            (Mark::Mute, Mark::Mute) => Mark::Mute,
            (Mark::Mute, Mark::Attr) => Mark::Attr,
            (Mark::Mute, Mark::Unmute) => Mark::Unmute,
            (Mark::Attr, Mark::Default) => Mark::Attr,
            (Mark::Attr, Mark::Mute) => Mark::Mute,
            (Mark::Attr, Mark::Attr) => Mark::Attr,
            (Mark::Attr, Mark::Unmute) => Mark::Unmute,
            (Mark::Unmute, Mark::Default) => Mark::Unmute,
            (Mark::Unmute, Mark::Mute) => Mark::Mute,
            (Mark::Unmute, Mark::Attr) => Mark::Attr,
            (Mark::Unmute, Mark::Unmute) => Mark::Unmute,
        };

        for (alt_index, alt) in g.get_definition(name)?.iter().enumerate() {
            let maybe_id = self.traces.task(
                name,
                alt_index,
                effective_mark,
                defn_alias.clone(),
                current_pos,
                current_pos,
                DotNotation::new(Rc::clone(alt)),
            );

            // CRITICAL FIX: Handle nullable alternatives immediately whether new or deduplicated
            // Check if this alternative is nullable (can produce epsilon) - use efficient cached method
            if g.is_alternative_nullable_by_index(name, alt_index)? {
                debug_earley_pos!(DebugLevel::Trace, current_pos, "PREDICTOR: Nullable rule {}[{}] - triggering immediate completion (Bpredict/complete)", name, alt_index);

                // For truly empty rules (zero factors), add to completed_trace here
                // They never get queued (already completed) so won't go through complete()
                // For rules with nullable factors, they complete naturally through Earley algorithm
                if let Some(task_id) = maybe_id {
                    if alt.factors.is_empty() {
                        // Truly empty rule - add initial task which is already completed
                        self.completed_trace.push(task_id);
                        // A zero-factor rule completes via task() (not task_advance_cursor),
                        // so record its (empty) derivation family here.
                        let (nm, og, ps, ai, dotc) = {
                            let t = self.traces.get(task_id);
                            (t.name.clone(), t.origin, t.pos, t.alt_index, t.dot.clone())
                        };
                        self.traces.record_family(&nm, og, ps, ai, &dotc);
                    }
                    // Else: has nullable factors, will complete naturally
                }

                // For empty rules, we need to trigger completion regardless of deduplication.
                // The child completes at current_pos, so resume parents that predicted
                // this nonterminal at current_pos (the (name, current_pos) key). Reuse the
                // scratch buffer (no `?` returns inside, so the capacity is always restored).
                let mut waiting_parents = std::mem::take(&mut self.parents_scratch);
                self.traces
                    .collect_waiting_parents(name, current_pos, &mut waiting_parents);

                for &continue_id in &waiting_parents {
                    debug!(
                        "...immediately continuing parent Task for empty rule... {}",
                        self.traces.format_task(continue_id)
                    );

                    let now_finished_via_child = self.traces.get(continue_id).dot.next_unparsed();
                    let match_rec = match now_finished_via_child {
                        Factor::Nonterm(mark, name, alias) => {
                            MatchRec::NonTerm(name, current_pos, mark, alias)
                        }
                        Factor::Terminal(tmark, _ch) => {
                            // Should never happen: terminals are consumed by the Scanner.
                            return Err(ParseError::internal(format!(
                                "predict(): parent task is waiting on terminal {tmark:?}; terminals must be handled by the scanner"
                            )));
                        }
                        Factor::Insertion(_tmark, text) => {
                            // Should never happen: insertions are advanced in the main loop.
                            return Err(ParseError::internal(format!(
                                "predict(): parent task is waiting on insertion {text:?}; insertions must be handled in the main loop"
                            )));
                        }
                    };
                    trace!("MatchRec {:?}", &match_rec);

                    // Child completed immediately; advance parent cursor
                    let maybe_continue_id = self.traces.task_advance_cursor(continue_id, match_rec);

                    // Queue parent continuations at back to ensure exhaustive alternative exploration
                    self.queue_back(maybe_continue_id);
                }
                // Restore the scratch buffer for reuse.
                self.parents_scratch = waiting_parents;
            }

            // Handle task queueing for non-empty rules or if we need to queue the task itself
            if let Some(task_id) = maybe_id {
                if !self.traces.get(task_id).dot.is_completed() {
                    // Normal rule - queue for standard processing
                    self.queue_front(Some(task_id));
                }
                // Note: empty rules are handled above and don't need to be queued
            }
        }
        Ok(())
    }

    /// SCANNER: Handle terminal scanning by matching against input characters
    /// Implements: sym starts (input, pos): RECORD TERMINAL input FOR task
    ///                                      CONTINUE task AT (pos incremented (input, sym))
    ///             ELSE: PASS \Terminal, doesn't match
    fn scan(
        &mut self,
        tid: TraceId,
        tmark: TMark,
        matcher: &TerminalDefn,
        input: &mut InputIter,
        session: &ParseSession,
    ) -> Result<(), ParseError> {
        let current_pos = self.traces.get(tid).pos;
        debug!("SCANNER: Terminal {tmark}{matcher} at pos={current_pos}");
        debug_earley_pos!(
            DebugLevel::Trace,
            current_pos,
            "SCANNER: {} scanning {}{}",
            self.traces.format_task(tid),
            tmark,
            matcher
        );

        // Bounds check: don't scan beyond input length
        if current_pos >= session.input_length {
            debug!(
                "Position {} >= input length {}; 🛑",
                current_pos, session.input_length
            );
            debug_earley_fail!(
                current_pos,
                &format!("{}", matcher),
                '∅',
                &self.queue_snapshot()
            );
            return Ok(());
        }

        if matcher.accept(input.get_at(current_pos)) {
            // Match! Advance position by 1
            let new_pos = current_pos + 1;
            let rec = MatchRec::Term(input.get_at(current_pos), new_pos, tmark);
            debug!("advance cursor SCAN");
            debug_earley_pos!(
                DebugLevel::Trace,
                current_pos,
                "SCANNER: MATCH '{}' -> advance to {}",
                input.get_at(current_pos),
                new_pos
            );
            let maybe_id = self.traces.task_advance_cursor(tid, rec);
            self.queue_back(maybe_id);
        } else {
            // Terminal doesn't match - silently drop this task (no requeue)
            // Per Earley algorithm: non-matching terminals should PASS (terminate quietly)
            debug!(
                "non-matched char '{}' (expecting {matcher}); 🛑",
                input.get_at(current_pos)
            );
        }
        Ok(())
    }

    fn queue_back(&mut self, maybe_id: Option<TraceId>) {
        if let Some(id) = maybe_id {
            let task = self.traces.get(id);
            debug!(
                "QUEUE: Adding to back S({}) (normal priority): {} | Queue: {} -> {}",
                task.pos,
                self.traces.format_task(id),
                self.traces.queue,
                self.traces.queue,
            );
            self.traces.queue.push_back(id, task.pos);
        }
    }

    fn queue_front(&mut self, maybe_id: Option<TraceId>) {
        if let Some(id) = maybe_id {
            let task = self.traces.get(id);
            debug!(
                "QUEUE: Adding to front S({}) (high priority): {} | Queue: {} -> {}",
                task.pos,
                self.traces.format_task(id),
                self.traces.queue,
                self.traces.queue,
            );
            self.traces.queue.push_front(id, task.pos);
        }
    }

    /// Generate a compact snapshot of the current queue state for debugging
    /// Shows entries from both front (LIFO) and back (FIFO) since deque has both aspects
    fn queue_snapshot(&self) -> String {
        if self.traces.queue.is_empty() {
            return "empty".to_string();
        }

        let queue_len = self.traces.queue.len();
        if queue_len <= 6 {
            // Small queue - show everything
            let snapshot: Vec<String> = self
                .traces
                .queue
                .iter()
                .map(|&tid| {
                    let task = self.traces.get(tid);
                    format!("{}@{}", task.name, task.pos)
                })
                .collect();
            return snapshot.join(",");
        }

        // Large queue - show front 3, middle indicator, back 3
        let front: Vec<String> = self
            .traces
            .queue
            .iter()
            .take(3)
            .map(|&tid| {
                let task = self.traces.get(tid);
                format!("{}@{}", task.name, task.pos)
            })
            .collect();

        let back: Vec<String> = self
            .traces
            .queue
            .iter()
            .skip(queue_len.saturating_sub(3))
            .map(|&tid| {
                let task = self.traces.get(tid);
                format!("{}@{}", task.name, task.pos)
            })
            .collect();

        format!("{}...{} ({})", front.join(","), back.join(","), queue_len)
    }

    /// Sift through and find only completed Tasks.
    /// Speeds up tree unpacking by skipping irrelevant parse states.
    ///
    /// Selection strategy:
    /// - Synthesized rules (names starting with "--"): first match in trace order.
    ///   Their alt ordering is implementation-defined; for repeat0/opt the empty alt
    ///   completes first (zero-factor, pushed immediately at prediction time), so
    ///   first-match gives the correct empty choice.
    /// - User-defined rules: prefer the lowest alt_index UNLESS that alt is self-referential
    ///   (its first matched child re-enters the rule — see [`Self::rule_reaches`]).
    ///   Self-referential alts rank below non-self-referential ones; within each category
    ///   lowest alt_index wins.  If all alts are self-referential, fall back to first-match.
    fn filter_completed_trace(&self, name: &str, origin: usize, pos: usize) -> Option<&Task> {
        // Span-indexed bucket of completed items for this exact (name, origin, pos),
        // in trace insertion order. Built once in unpack_parse_tree; avoids the former
        // per-node linear scan of the whole completed_trace.
        let bucket = self
            .completed_by_span
            .get(&(EarleyStr::from(name), origin, pos));
        let bucket = match bucket {
            Some(b) => b.as_slice(),
            None => return None,
        };

        if name.starts_with("--") {
            // Synthesized rules: return the first match (trace order).
            return bucket.first().map(|&tid| self.traces.get(tid));
        }

        // A task is "self-referential" if its first matched NonTerm child re-enters the
        // rule itself — directly (L→L•,M), through the desugaring of a repeat/option/group
        // operator (A→--A.f-plus• where `--A.f-plus` derives A), or indirectly through other
        // named rules (B→A• where A can derive B). Such alternatives produce needless nesting
        // or, for cyclic grammars, derivations cut short by the visited-guard; they rank below
        // alternatives that reach terminals without re-entering the rule. The empty alt (L→•)
        // is not self-referential.
        let is_self_ref = |t: &Task| -> bool {
            match t.dot.first_match() {
                Some(MatchRec::NonTerm(n, ..)) => {
                    n.as_str() == name || self.rule_reaches(n, name, &mut HashSet::new())
                }
                _ => false,
            }
        };

        let mut best: Option<&Task> = None;
        for &tid in bucket {
            let t = self.traces.get(tid);
            let is_better = match best {
                None => true,
                Some(b) => {
                    let t_self = is_self_ref(t);
                    let b_self = is_self_ref(b);
                    // Non-self-ref beats self-ref; within same category, lower alt_index wins.
                    (!t_self && b_self) || (t_self == b_self && t.alt_index < b.alt_index)
                }
            };
            if is_better {
                best = Some(t);
            }
        }
        best
    }

    /// True if nonterminal `from` can reach a reference to nonterminal `target` by
    /// following nonterminal references through the grammar (both user-defined and
    /// synthesized `--` rules). Used to detect self-embedding alternatives whose recursion
    /// is direct (`L: L, M`), hidden behind the desugaring of a repeat/option/group operator
    /// (`A: (A, A)+` mints `--A.f-plus` whose body references A), or indirect through other
    /// named rules (`B: A; "b"` where `A: "a"; B`, so B→A→B). `seen` guards against cycles.
    fn rule_reaches(&self, from: &str, target: &str, seen: &mut HashSet<EarleyStr>) -> bool {
        if !seen.insert(EarleyStr::new(from)) {
            return false;
        }
        let branching = match self.grammar.get_definition(from) {
            Ok(b) => b,
            Err(_) => return false,
        };
        for rule in branching.iter() {
            for factor in rule.iter() {
                if let Factor::Nonterm(_, n, _) = factor {
                    if n.as_str() == target || self.rule_reaches(n, target, seen) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Only for use in test sutes. Not guaranteed to be stable...
    #[cfg(test)]
    pub(crate) fn test_inspect_trace(&self, filter: Option<EarleyStr>) -> Vec<Task> {
        match filter {
            Some(str) => self
                .traces
                .arena
                .clone()
                .into_iter()
                .filter(|task| task.name == str)
                .collect(),
            None => self.traces.arena.clone(),
        }
    }

    fn unpack_parse_tree(&mut self, session: &ParseSession) -> Result<Arena<Content>, ParseError> {
        debug!("TRACE...");
        debug!("COMPLETED TASKS ({} total):", self.completed_trace.len());

        // Index completed items by span once, so per-node `filter_completed_trace`
        // lookups are O(bucket) instead of O(|completed_trace|). Insertion order within
        // each bucket mirrors trace order (synthesized-rule first-match relies on it).
        self.completed_by_span.clear();
        for &tid in &self.completed_trace {
            let t = self.traces.get(tid);
            self.completed_by_span
                .entry((t.name.clone(), t.origin, t.pos))
                .or_default()
                .push(tid);
        }
        for tid in &self.completed_trace {
            let task = self.traces.get(*tid);
            debug!(
                "  {} (origin={}, pos={})",
                self.traces.format_task(*tid),
                task.origin,
                task.pos
            );
        }

        // Check if we have a completed parse of our grammar's root rule that spans the entire input
        let name = self.grammar.get_root_definition_name().unwrap();
        debug!(
            "🔍 LOOKING FOR: completed parse of rule '{}' spanning (0 to {})",
            name, session.input_length
        );
        let root_completion = self.filter_completed_trace(&name, 0, session.input_length);

        if root_completion.is_none() {
            debug!(
                "❌ NO ROOT COMPLETION FOUND for '{}' spanning entire input",
                name
            );
            // Generate enhanced diagnostics for parse failures
            let mut diagnostic = format!(
                "Parse failed: no completed parse of rule '{}' spanning entire input (0 to {})\n",
                name, session.input_length
            );

            // Find the furthest position we reached
            diagnostic.push_str(&format!(
                "Furthest position reached: {}\n",
                session.farthest_pos
            ));

            // Show partial completions of the root rule
            let partial_completions: Vec<_> = self
                .completed_trace
                .iter()
                .filter_map(|&tid| {
                    let task = self.traces.get(tid);
                    if task.name == name {
                        Some((tid, task))
                    } else {
                        None
                    }
                })
                .collect();

            if !partial_completions.is_empty() {
                diagnostic.push_str("Partial completions of root rule found:\n");
                for (tid, task) in partial_completions {
                    diagnostic.push_str(&format!(
                        "  {} (origin={}, pos={})\n",
                        self.traces.format_task(tid),
                        task.origin,
                        task.pos
                    ));
                }
            }

            // Show what completions we do have near the furthest position
            let nearby_completions: Vec<_> = self
                .completed_trace
                .iter()
                .filter_map(|&tid| {
                    let task = self.traces.get(tid);
                    if task.pos >= session.farthest_pos.saturating_sub(5)
                        && task.pos <= session.farthest_pos + 5
                    {
                        Some((tid, task))
                    } else {
                        None
                    }
                })
                .take(10)
                .collect();

            if !nearby_completions.is_empty() {
                diagnostic.push_str(&format!(
                    "Completions near furthest position ({}±5):\n",
                    session.farthest_pos
                ));
                for (tid, task) in nearby_completions {
                    diagnostic.push_str(&format!(
                        "  {} (origin={}, pos={})\n",
                        self.traces.format_task(tid),
                        task.origin,
                        task.pos
                    ));
                }
            }

            // Show active tasks in queue
            if !self.traces.queue.is_empty() {
                diagnostic.push_str(&format!(
                    "Active tasks remaining in queue: {}\n",
                    self.traces.queue.len()
                ));
                for &tid in self.traces.queue.iter().take(5) {
                    let task = self.traces.get(tid);
                    diagnostic.push_str(&format!(
                        "  {} (pos={})\n",
                        self.traces.format_task(tid),
                        task.pos
                    ));
                }
                if self.traces.queue.len() > 5 {
                    diagnostic
                        .push_str(&format!("  ... and {} more\n", self.traces.queue.len() - 5));
                }
            }

            return Err(ParseError::static_err(&diagnostic));
        }

        let mut arena = Arena::new();
        let root = arena.new_node(Content::Root);
        debug!(
            "Found completed parse of '{}' from 0 to {}",
            name, session.input_length
        );
        let mut visited = HashSet::new();
        self.unpack_parse_tree_internal(
            &mut arena,
            &name,
            Mark::Default,
            None,
            0,
            session.input_length,
            root,
            &mut visited,
        );

        // the standard algorithm above leaves attribute nodes in an inconvenient state.
        // with a bare Content::Attribute node, for which one needs to plumb all descendants to find text nodes
        // below, we do that once-and-for-all for each Content::Attribute node
        let attr_node_ids = arena
            .iter()
            .filter(|n| matches!(n.get(), Content::Attribute(..)))
            .map(|n| arena.get_node_id(n).unwrap())
            .collect::<Vec<_>>();
        for attr_nid in attr_node_ids {
            let attr_val = self.unpack_attr_value(attr_nid, &mut arena);
            arena
                .get_mut(attr_nid)
                .unwrap()
                .get_mut()
                .set_value(attr_val);
        }
        // n.b. this doesn't actually delete these original descendent text nodes...
        // but you should never need to even look for them

        Ok(arena)
    }

    /// Recurse down through the tree to assemble all the text literals that comprise an attribute value
    fn unpack_attr_value(&self, attr_nid: NodeId, arena: &mut Arena<Content>) -> String {
        let mut attr_value = String::new();
        for descendant in attr_nid.descendants(arena) {
            if let Content::Text(txt) = arena.get(descendant).unwrap().get() {
                attr_value.push_str(txt.as_str());
            }
        }
        attr_value
    }

    #[allow(clippy::too_many_arguments)] // recursive tree walk threads the full span context
    fn unpack_parse_tree_internal(
        &self,
        arena: &mut Arena<Content>,
        name: &str,
        mark: Mark,
        alias: Option<&EarleyStr>,
        origin: usize,
        end: usize,
        root: NodeId,
        visited: &mut HashSet<(EarleyStr, usize, usize)>,
    ) {
        // Skip nonterminals already on the current path — cyclic grammar derivations would
        // otherwise recurse infinitely through the same (name, span) triple.
        let key = (EarleyStr::new(name), origin, end);
        if !visited.insert(key.clone()) {
            return;
        }

        let matching_trace = self.filter_completed_trace(name, origin, end);
        let mut new_root = root;
        match matching_trace {
            Some(task) => {
                let match_name = &task.name;

                let effective_mark = if mark == Mark::Default {
                    task.mark
                } else {
                    mark
                };

                if effective_mark == Mark::Mute || match_name.starts_with('-') {
                    // Skip
                    debug!("trace found {mark} {task} -- SKIPPING");
                } else {
                    // Element or Attribute
                    debug!("trace found {} {task}", task.mark);
                    let name_str = alias
                        .or(task.alias.as_ref())
                        .unwrap_or(match_name)
                        .to_string();
                    let data = if effective_mark == Mark::Attr {
                        Content::Attribute(name_str, "".to_string()) // 2nd pass will fill in the atttribute value
                    } else {
                        Content::Element(name_str)
                    };
                    let temp_root = arena.new_node(data);
                    root.append(temp_root, arena);
                    new_root = temp_root;
                }

                // CHILDREN
                let mut new_origin = origin;
                let dot = &task.dot;
                for match_rec in dot.matches_in_order() {
                    match match_rec {
                        MatchRec::Term(ch, pos, tmark) => {
                            if *tmark != TMark::Mute {
                                let new_child = arena.new_node(Content::Text(ch.to_string()));
                                new_root.append(new_child, arena);
                            }
                            new_origin = *pos;
                        }
                        MatchRec::NonTerm(nt_name, pos, mark, alias) => {
                            self.unpack_parse_tree_internal(
                                arena,
                                nt_name,
                                *mark,
                                alias.as_ref(),
                                new_origin,
                                *pos,
                                new_root,
                                visited,
                            );
                            new_origin = *pos;
                        }
                        MatchRec::Insertion(_pos, text, tmark) => {
                            // Insertions add text to output without consuming input
                            if *tmark != TMark::Mute {
                                let new_child = arena.new_node(Content::Text(text.to_string()));
                                new_root.append(new_child, arena);
                            }
                            // Note: pos doesn't change since insertion doesn't consume input
                        }
                    }
                }
            }
            None => {
                info!("  No matching traces for {}@{}:{}", name, origin, end);
            }
        }

        visited.remove(&key);

        //HOW TO SERIALISE name FROM start TO end:
        //    IF SOME task IN trace[end] HAS (symbol task = name AND finished task AND start.position task = start):
        //        WRITE "<", name, ">"
        //        CHILDREN
        //        WRITE "</", name, ">"
        //CHILDREN:
        //    PUT start IN newstart
        //    FOR (sym, pos) IN done task:
        //        SELECT:
        //            terminal sym: WRITE sym
        //            ELSE:
        //                SERIALISE sym FROM newstart TO pos
        //        PUT pos IN newstart
    }

    /// True if the last parse found root-alternative ambiguity.
    ///
    /// Structural ambiguity per the iXML spec: the accepting root item `(start, 0, n)`,
    /// or any completed item reachable from it through the derivation forest, has two or
    /// more distinct derivation families. Families are recorded during the parse (keyed by
    /// immediate cause, so bookkeeping duplicates collapse to one); this is a read-only walk.
    pub fn is_ambiguous(&self) -> bool {
        let root_name = match self.grammar.get_root_definition_name() {
            Some(n) => n,
            None => return false,
        };
        let mut visited: HashSet<(EarleyStr, usize, usize)> = HashSet::new();
        self.span_is_ambiguous(&root_name, 0, self.last_input_len, &mut visited)
    }

    /// True if this completed span, or any span reachable through its representative
    /// derivation, is locally ambiguous (>=2 families). A locally-ambiguous node returns
    /// immediately without recursing; an unambiguous node has exactly one family, so its
    /// child edges are the unique reachability set and a single representative path is exact.
    /// The walk descends into muted/synthesized rules, where repetition ambiguity lives.
    fn span_is_ambiguous(
        &self,
        name: &str,
        origin: usize,
        end: usize,
        visited: &mut HashSet<(EarleyStr, usize, usize)>,
    ) -> bool {
        let key = (EarleyStr::from(name), origin, end);
        if !visited.insert(key.clone()) {
            return false; // cycle / already-explored guard
        }
        if self.traces.families.get(&key).map_or(0, |s| s.len()) >= 2 {
            return true; // locally ambiguous
        }
        let task = match self.filter_completed_trace(name, origin, end) {
            Some(t) => t,
            None => return false,
        };
        let mut child_origin = origin;
        for rec in task.dot.matches_in_order() {
            match rec {
                MatchRec::NonTerm(nt, pos, _, _) => {
                    if self.span_is_ambiguous(nt, child_origin, *pos, visited) {
                        return true;
                    }
                    child_origin = *pos;
                }
                MatchRec::Term(_, pos, _) => child_origin = *pos,
                MatchRec::Insertion(..) => {}
            }
        }
        false
    }

    #[cfg(test)]
    pub(crate) fn tree_to_test_format(arena: &Arena<Content>) -> String {
        Self::tree_to_test_format_with_state(arena, false, false)
    }

    /// Arena → XML string with `ixml:state` stamping. Test-only helper that
    /// pins the arena→`Document`→XML path via characterization goldens; the
    /// production path is [`crate::treebird::Document::to_xml_with_state`].
    #[cfg(test)]
    pub(crate) fn tree_to_test_format_with_state(
        arena: &Arena<Content>,
        version_mismatch: bool,
        ambiguous: bool,
    ) -> String {
        // Serialization now lives on the owned treebird tree; convert at the
        // boundary and serialize from there. The indextree arena stays private.
        crate::treebird::Document::from_content_arena(arena)
            .to_xml_with_state(version_mismatch, ambiguous)
    }

    /// Helper function for working with indextree
    /// Given a `NodeId` (that should be an element) get all the Attribute nodes
    /// Returns an easily-digestiable `HashMap` of Name -> Value
    pub(crate) fn get_attributes(arena: &Arena<Content>, elem: NodeId) -> HashMap<String, String> {
        elem.children(arena)
            // from NodeId to Content...
            .map(|n| arena.get(n).unwrap().get())
            // and only Content::Attribute...
            .filter(|c| matches!(*c, Content::Attribute(..)))
            // and pair it up to put in a HashMap...
            .map(|node| (node.get_name().unwrap(), node.get_value().unwrap()))
            .collect()
    }

    /// Helper function for working with indextree
    /// get all immediate element children
    /// Returns a Vec of pairs of (Element Name , `NodeId`)
    /// Roughly like the `XPath` child axis
    pub(crate) fn get_child_elements(arena: &Arena<Content>, nid: NodeId) -> Vec<(String, NodeId)> {
        nid.children(arena)
            // fist pair up as (&Content, NodeId)
            .map(|nid| (arena.get(nid).unwrap().get(), nid))
            // and keep only elements
            .filter(|(c, _)| matches!(c, Content::Element(_)))
            // then pair up as (ElementName, NodeId)
            .map(|(c, nid)| (c.get_name().unwrap(), nid))
            .collect()
    }

    /// Helper function for working with indextree
    /// get all immediate element children matching a given name
    /// Returns a Vec of `NodeId`
    pub(crate) fn get_child_elements_named(
        arena: &Arena<Content>,
        nid: NodeId,
        name: &str,
    ) -> Vec<NodeId> {
        nid.children(arena)
            .filter(|n| {
                let content = arena.get(*n).unwrap().get();
                if let Content::Element(nam) = content {
                    nam == name
                } else {
                    false
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::Grammar;

    // ---- Serializer characterization goldens -------------------------------
    // Lock the CURRENT `Arena<Content>` -> XML string output before the treebird
    // refactor relocates serialization onto an owned Document/Node tree. These
    // exercise `Parser::tree_to_test_format*` end-to-end and MUST stay green while
    // the implementation is ported (the public fns will delegate to the new
    // serializer). They are deliberately built from hand-assembled arenas so they
    // pin the mapping itself, independent of the parse path.

    fn g_root() -> (Arena<Content>, NodeId) {
        let mut arena = Arena::new();
        let r = arena.new_node(Content::Root);
        (arena, r)
    }
    fn g_el(arena: &mut Arena<Content>, parent: NodeId, name: &str) -> NodeId {
        let n = arena.new_node(Content::Element(name.to_string()));
        parent.append(n, arena);
        n
    }
    fn g_attr(arena: &mut Arena<Content>, parent: NodeId, k: &str, v: &str) {
        let n = arena.new_node(Content::Attribute(k.to_string(), v.to_string()));
        parent.append(n, arena);
    }
    fn g_text(arena: &mut Arena<Content>, parent: NodeId, s: &str) {
        let n = arena.new_node(Content::Text(s.to_string()));
        parent.append(n, arena);
    }

    #[test]
    fn golden_element_with_text() {
        let (mut a, r) = g_root();
        let e = g_el(&mut a, r, "a");
        g_text(&mut a, e, "hi");
        assert_eq!(Parser::tree_to_test_format(&a), "<a>hi</a>");
    }

    #[test]
    fn golden_attributes_in_document_order() {
        let (mut a, r) = g_root();
        let e = g_el(&mut a, r, "a");
        g_attr(&mut a, e, "x", "1");
        g_attr(&mut a, e, "y", "2");
        g_text(&mut a, e, "hi");
        assert_eq!(Parser::tree_to_test_format(&a), r#"<a x="1" y="2">hi</a>"#);
    }

    #[test]
    fn golden_empty_element_self_closes() {
        let (mut a, r) = g_root();
        g_el(&mut a, r, "a");
        assert_eq!(Parser::tree_to_test_format(&a), "<a/>");
    }

    #[test]
    fn golden_attribute_only_element_self_closes() {
        // An element with attributes but no element/text children still self-closes.
        let (mut a, r) = g_root();
        let e = g_el(&mut a, r, "a");
        g_attr(&mut a, e, "x", "1");
        assert_eq!(Parser::tree_to_test_format(&a), r#"<a x="1"/>"#);
    }

    #[test]
    fn golden_attribute_value_escaping() {
        // Attribute values escape & < " (in that order); > is NOT escaped.
        let (mut a, r) = g_root();
        let e = g_el(&mut a, r, "a");
        g_attr(&mut a, e, "k", r#"a&b<c"d>e"#);
        assert_eq!(
            Parser::tree_to_test_format(&a),
            r#"<a k="a&amp;b&lt;c&quot;d>e"/>"#
        );
    }

    #[test]
    fn golden_text_escaping() {
        // Text escapes only & and < ; > and " are left literal.
        let (mut a, r) = g_root();
        let e = g_el(&mut a, r, "a");
        g_text(&mut a, e, r#"a&b<c>d"e"#);
        assert_eq!(
            Parser::tree_to_test_format(&a),
            r#"<a>a&amp;b&lt;c>d"e</a>"#
        );
    }

    #[test]
    fn golden_nested_elements() {
        let (mut a, r) = g_root();
        let e = g_el(&mut a, r, "a");
        let b = g_el(&mut a, e, "b");
        g_text(&mut a, b, "x");
        g_el(&mut a, e, "c");
        assert_eq!(Parser::tree_to_test_format(&a), "<a><b>x</b><c/></a>");
    }

    #[test]
    fn golden_ambiguous_state_attrs_on_root_child_only() {
        // ixml:state="ambiguous" extra attrs apply to the top-level element only,
        // never to descendants.
        let (mut a, r) = g_root();
        let e = g_el(&mut a, r, "a");
        g_el(&mut a, e, "b");
        assert_eq!(
            Parser::tree_to_test_format_with_state(&a, false, true),
            r#"<a xmlns:ixml="http://invisiblexml.org/NS" ixml:state="ambiguous"><b/></a>"#
        );
    }

    #[test]
    fn golden_version_mismatch_state_appends_after_real_attrs() {
        // Real attributes come first; version-mismatch extra attrs follow, unescaped.
        let (mut a, r) = g_root();
        let e = g_el(&mut a, r, "a");
        g_attr(&mut a, e, "x", "1");
        assert_eq!(
            Parser::tree_to_test_format_with_state(&a, true, false),
            r#"<a x="1" xmlns="" xmlns:ixml="http://invisiblexml.org/NS" ixml:state="version-mismatch"/>"#
        );
    }
    // ---- end serializer characterization goldens --------------------------

    #[test]
    fn test_offset_bounds_violation_protection() {
        // Test that parser doesn't advance beyond input length
        let grammar_str = r#"test: "a"."#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Failed to parse grammar");
        let mut parser = Parser::new(grammar);

        // This should not hang or panic - should complete gracefully
        let result = parser.parse_to_arena("a");
        assert!(result.is_ok(), "Simple parse should succeed");
    }

    #[test]
    fn test_empty_line_pattern_bounds() {
        // Test the specific pattern that caused infinite loop: line++lf with nullable lines
        let grammar_str = r#"
            input: line++lf.
            line: ~[#a | #d]*.
            lf: -#a | -#d, -#a.
        "#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Failed to parse grammar");
        let mut parser = Parser::new(grammar);

        // This previously caused infinite loop - should now complete (may fail parsing but shouldn't hang)
        let input = "Now is the time\nFor all good people\nTo have fun.";
        let result = parser.parse_to_arena(input);
        // We don't care if it succeeds or fails, just that it doesn't hang
        let _ = result;
    }

    #[test]
    fn test_offset_never_exceeds_input_length() {
        // Create a simple grammar that will exercise position advancement
        let grammar_str = r#"letters: letter+. letter: ["a"-"z"]."#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Failed to parse grammar");
        let mut parser = Parser::new(grammar);

        let input = "abc";
        let _ = parser.parse_to_arena(input);

        // Check that no task in the trace has a position > input.len()
        let trace = parser.test_inspect_trace(None);
        let input_len = input.len();

        for task in trace {
            assert!(
                task.pos <= input_len,
                "Task position {} exceeds input length {}",
                task.pos,
                input_len
            );
            assert!(
                task.origin <= input_len,
                "Task origin {} exceeds input length {}",
                task.origin,
                input_len
            );
        }
    }

    #[test]
    fn test_bounds_check_with_zero_length_input() {
        let grammar_str = r#"test: "a"?."#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Failed to parse grammar");
        let mut parser = Parser::new(grammar);

        // Empty input should not cause bounds violations
        let result = parser.parse_to_arena("");
        let _ = result; // May succeed or fail, but shouldn't hang

        // Verify no positions exceed 0 (the length of empty input)
        let trace = parser.test_inspect_trace(None);
        for task in trace {
            assert!(
                task.pos == 0,
                "Task position {} exceeds empty input length",
                task.pos
            );
        }
    }

    #[test]
    fn test_bounds_check_with_single_char() {
        let grammar_str = r#"test: "a", "b"?."#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Failed to parse grammar");
        let mut parser = Parser::new(grammar);

        // Single character input
        let result = parser.parse_to_arena("a");
        let _ = result; // May succeed or fail

        // Verify no positions exceed 1
        let trace = parser.test_inspect_trace(None);
        for task in trace {
            assert!(
                task.pos <= 1,
                "Task position {} exceeds input length 1",
                task.pos
            );
        }
    }

    #[cfg(test)]
    #[test]
    fn test_single_char_string_attribute_extraction() {
        // This test demonstrates the bug where single-character string members
        // in character sets get corrupted during attribute extraction.
        // Expected: ["A"] should create a character set matching only 'A'
        // Actual: ["A"] gets corrupted to ["AB"] or similar, matching unintended characters

        let grammar_str = r#"test: ["A"]."#;
        let result = Grammar::from_ixml_str(grammar_str);

        match result {
            Ok(grammar) => {
                // Print the grammar to see what was actually generated
                println!("Generated grammar: {}", grammar);

                let mut parser = Parser::new(grammar);

                // The grammar should match 'A'
                let input_a = parser.parse_to_arena("A");
                assert!(input_a.is_ok(), "Grammar should match 'A'");

                // The grammar should NOT match 'B' - this is the key test
                let mut parser = Parser::new(Grammar::from_ixml_str(grammar_str).unwrap());
                let input_b = parser.parse_to_arena("B");
                if input_b.is_ok() {
                    panic!("BUG DETECTED: Grammar incorrectly matches 'B' when it should only match 'A'. This indicates the attribute extraction bug where single-character strings get corrupted.");
                }
            }
            Err(e) => {
                // If parsing fails, this also demonstrates the bug
                panic!("Grammar parsing failed: {}. This could be due to the bootstrap parsing issue with single-character strings.", e);
            }
        }
    }

    #[cfg(test)]
    #[test]
    fn test_synthesized_repeat_ambiguity_handling() {
        // Test the specific issue where synthesized repeat constructs with ambiguous alternatives
        // fail to properly backtrack. This tests the core bootstrap grammar parsing issue.
        //
        // The pattern `(member, s)**(-[";|"], s)` creates alternatives where:
        // - `member → string` can match `"0"` at position 39
        // - `member → range` needs `"0"-"9"` spanning positions 39-44
        //
        // The parser should successfully parse both alternatives within the repeat construct.

        // This should parse successfully but currently fails due to ambiguity handling
        let problematic_grammar = r#"test: ["0"-"9"]."#;

        println!("Testing problematic grammar: {}", problematic_grammar);
        let result = Grammar::from_ixml_str(problematic_grammar);

        match result {
            Ok(grammar) => {
                println!("✅ Grammar parsed successfully (this is unexpected!)");
                let mut parser = Parser::new(grammar);

                // Test that it correctly matches digits
                let digit_result = parser.parse_to_arena("5");
                assert!(digit_result.is_ok(), "Should match digit '5'");

                // Test that it rejects non-digits
                let mut parser2 = Parser::new(Grammar::from_ixml_str(problematic_grammar).unwrap());
                let letter_result = parser2.parse_to_arena("A");
                assert!(letter_result.is_err(), "Should not match letter 'A'");
            }
            Err(e) => {
                println!("❌ Grammar parsing failed as expected: {}", e);
                // This demonstrates the bug - the repeat construct ambiguity prevents successful parsing
                assert!(
                    e.to_string().contains("no completed parse"),
                    "Expected bootstrap parsing failure, got: {}",
                    e
                );
            }
        }

        // Test workaround: simpler patterns that should work
        let simple_patterns = vec![
            r#"test: [#30-#39]."#, // Hex range for digits
            r#"test: ["a"]."#,     // Single string member
        ];

        for pattern in simple_patterns {
            println!("Testing workaround pattern: {}", pattern);
            let result = Grammar::from_ixml_str(pattern);
            if let Err(e) = result {
                println!("  ❌ Even simple pattern failed: {}", e);
            } else {
                println!("  ✅ Simple pattern works");
            }
        }
    }

    #[cfg(test)]
    #[test]
    fn test_queue_priority_for_ambiguous_alternatives() {
        // Test that completions get priority over new predictions
        // This ensures ambiguous cases like ["A"] vs ranges are handled correctly

        let test_cases = vec![
            // Single char string vs range ambiguity
            (r#"test: ["A"]."#, "A", true),
            (r#"test: ["A"]."#, "B", false),
            // Mixed character set with ambiguity
            (r#"test: ["A"; "B"]."#, "A", true),
            (r#"test: ["A"; "B"]."#, "B", true),
            (r#"test: ["A"; "B"]."#, "C", false),
            // Hex vs string ambiguity
            (r#"test: [#41]."#, "A", true), // #41 = 'A'
            (r#"test: [#41]."#, "B", false),
            // Range vs single member ambiguity
            (r#"test: ["A"-"C"]."#, "B", true),
            (r#"test: ["A"-"C"]."#, "D", false),
        ];

        for (grammar_str, input, should_match) in test_cases {
            let result = Grammar::from_ixml_str(grammar_str);
            assert!(result.is_ok(), "Grammar should parse: {}", grammar_str);

            let mut parser = Parser::new(result.unwrap());
            let parse_result = parser.parse_to_arena(input);

            if should_match {
                assert!(
                    parse_result.is_ok(),
                    "Should match: '{}' with grammar {}",
                    input,
                    grammar_str
                );
            } else {
                assert!(
                    parse_result.is_err(),
                    "Should NOT match: '{}' with grammar {}",
                    input,
                    grammar_str
                );
            }
        }
    }

    #[cfg(test)]
    #[test]
    fn test_multi_char_string_works_correctly() {
        // This test shows that multi-character strings work correctly,
        // demonstrating that the bug is specific to single-character strings

        let grammar_str = r#"test: ["AB"]."#;
        let result = Grammar::from_ixml_str(grammar_str);

        // Multi-character strings should work
        assert!(
            result.is_ok(),
            "Multi-character string grammar should parse correctly"
        );

        if let Ok(grammar) = result {
            let mut parser = Parser::new(grammar);

            // Should match 'A' (first character of "AB")
            let input_a = parser.parse_to_arena("A");
            assert!(input_a.is_ok(), "Grammar should match 'A' from [\"AB\"]");

            // Should also match 'B' (second character of "AB")
            let mut parser = Parser::new(Grammar::from_ixml_str(grammar_str).unwrap());
            let input_b = parser.parse_to_arena("B");
            assert!(input_b.is_ok(), "Grammar should match 'B' from [\"AB\"]");
        }
    }

    #[cfg(test)]
    #[test]
    fn test_hand_built_vs_parsed_range_grammar() {
        // Test case based on ixml/tests/correct/range.ixml
        // This grammar has character ranges: ["0"-"9"] and hex ranges: [#0-#9]
        // Input: "5\t." should parse as range1='5', range2='\t', literal='.'

        use crate::grammar::{Grammar, RuleContext};

        // Hand-build the expected grammar for:
        // data: range1, range2, -".".
        // range1: ["0"-"9"].
        // range2: [#0-#9].

        let mut hand_built = Grammar::new();

        // data: range1, range2, -".".
        let ctx = RuleContext::new("data");
        hand_built.define(
            "data",
            ctx.seq()
                .nt("range1")
                .nt("range2")
                .mark_ch('.', crate::grammar::TMark::Mute),
        );

        // range1: ["0"-"9"].
        let ctx = RuleContext::new("range1");
        hand_built.define("range1", ctx.seq().ch_range('0', '9'));

        // range2: [#0-#9].
        let ctx = RuleContext::new("range2");
        hand_built.define("range2", ctx.seq().ch_range('\u{0000}', '\u{0009}')); // hex 0 to hex 9

        println!("Hand-built grammar:\n{}", hand_built);

        // Test with input "5\t."
        let mut parser = Parser::new(hand_built);
        let result = parser.parse_to_arena("5\t.");
        assert!(
            result.is_ok(),
            "Hand-built range grammar should parse '5\\t.'"
        );
    }

    #[cfg(test)]
    #[test]
    fn test_minimal_ixml_subset_bootstrap() {
        // Create a minimal subset of ixml grammar that focuses on rule parsing
        // Based on: ixml: s, rule++RS, s.
        // This should help isolate the exact bootstrap parsing failure

        use crate::grammar::{Grammar, Mark, RuleContext, TMark};

        println!("=== Building minimal ixml subset grammar ===");

        let mut mini_ixml = Grammar::new();

        // ixml: s, rule++RS, s.
        let ctx = RuleContext::new("ixml");
        mini_ixml.define(
            "ixml",
            ctx.seq()
                .nt("s")
                .repeat1_sep(ctx.seq().nt("rule"), ctx.seq().nt("RS"))
                .nt("s"),
        );

        // rule: name, s, -":", s, -".", s.  (simplified - no alts for now)
        let ctx = RuleContext::new("rule");
        mini_ixml.define(
            "rule",
            ctx.seq()
                .nt("name")
                .nt("s")
                .mark_ch(':', TMark::Mute)
                .nt("s")
                .mark_ch('.', TMark::Mute)
                .nt("s"),
        );

        // name: namestart, namefollower*.  (this is the problematic one!)
        let ctx = RuleContext::new("name");
        mini_ixml.mark_define(
            Mark::Attr,
            "name",
            ctx.seq()
                .nt("namestart")
                .repeat0(ctx.seq().nt("namefollower")),
        );

        // namestart: ["a"-"z"; "A"-"Z"].  (simplified)
        let ctx = RuleContext::new("namestart");
        mini_ixml.mark_define(Mark::Mute, "namestart", ctx.seq().ch_range('a', 'z'));
        mini_ixml.mark_define(Mark::Mute, "namestart", ctx.seq().ch_range('A', 'Z'));

        // namefollower: namestart.  (simplified - no numbers/symbols)
        let ctx = RuleContext::new("namefollower");
        mini_ixml.mark_define(Mark::Mute, "namefollower", ctx.seq().nt("namestart"));

        // s: whitespace*.  (optional spacing)
        let ctx = RuleContext::new("s");
        mini_ixml.mark_define(
            Mark::Mute,
            "s",
            ctx.seq().repeat0(ctx.seq().nt("whitespace")),
        );

        // RS: whitespace+.  (required spacing)
        let ctx = RuleContext::new("RS");
        mini_ixml.mark_define(
            Mark::Mute,
            "RS",
            ctx.seq().repeat1(ctx.seq().nt("whitespace")),
        );

        // whitespace: " "; "\n".  (simplified)
        let ctx = RuleContext::new("whitespace");
        mini_ixml.mark_define(Mark::Mute, "whitespace", ctx.seq().ch(' '));
        mini_ixml.mark_define(Mark::Mute, "whitespace", ctx.seq().ch('\n'));

        println!("Minimal ixml subset grammar:");
        println!("{}", mini_ixml);

        // Test 1: Try to parse our minimal failing case
        let input = "a: . b: .";
        println!("\n=== Testing input: '{}' ===", input);

        let mut parser = Parser::new(mini_ixml);
        let result = parser.parse_to_arena(input);

        match result {
            Ok(arena) => {
                let output = Parser::tree_to_test_format(&arena);
                println!("SUCCESS: {}", output);
                // If this succeeds, the issue is elsewhere (no assertion needed).
            }
            Err(e) => {
                println!("FAILURE: {:?}", e);
                // If this fails, we've reproduced the bootstrap issue in minimal form
                println!("Reproduced bootstrap parsing failure in minimal subset!");
                // Don't panic - this is expected and useful for debugging
            }
        }
    }

    #[cfg(test)]
    #[test]
    fn test_empty_alternative_in_grammar() {
        // Test that empty alternatives are properly parsed by bootstrap grammar
        // Grammar: S has two alternatives - empty and "done"
        let grammar_str = r#"S: ; "done"."#;

        match Grammar::from_ixml_str(grammar_str) {
            Ok(grammar) => {
                // Print entire grammar structure
                println!("\n=== GRAMMAR STRUCTURE ===");
                for rule_name in grammar.defn_order.iter() {
                    if let Ok(rule_def) = grammar.get_definition(rule_name) {
                        println!("Rule: {} (mark: {:?})", rule_name, rule_def.mark());
                        for (i, alt) in rule_def.iter().enumerate() {
                            println!("  Alt[{}]: {} factors", i, alt.factors.len());
                            for (j, factor) in alt.factors.iter().enumerate() {
                                println!("    Factor[{}]: {:?}", j, factor);
                            }
                        }
                    }
                }
                println!("=== END GRAMMAR ===\n");

                // Should have rule S with 2 alternatives
                let s_def = grammar.get_definition("S").expect("Should have rule S");
                let alts: Vec<_> = s_def.iter().collect();

                println!("Found {} alternatives for rule S", alts.len());
                for (i, alt) in alts.iter().enumerate() {
                    println!(
                        "  Alt {}: {} factors: {:?}",
                        i,
                        alt.factors.len(),
                        alt.factors
                    );
                }

                assert_eq!(
                    alts.len(),
                    2,
                    "Rule S should have 2 alternatives, got {}",
                    alts.len()
                );

                // First alternative should be empty (0 factors)
                assert_eq!(
                    alts[0].factors.len(),
                    0,
                    "First alternative should be empty, got {} factors",
                    alts[0].factors.len()
                );

                // Second alternative should have 4 factors (string "done" gets expanded to chars)
                assert_eq!(
                    alts[1].factors.len(),
                    4,
                    "Second alternative should have 4 factors (d,o,n,e), got {}",
                    alts[1].factors.len()
                );

                // Now test that we can actually USE the grammar to parse input
                // The empty alternative should match empty input
                let mut parser = Parser::new(grammar.clone());
                match parser.parse_to_arena("") {
                    Ok(_) => {
                        println!("✓ Successfully parsed empty input with empty alternative");
                    }
                    Err(e) => {
                        panic!(
                            "Failed to parse empty input with empty alternative: {:?}",
                            e
                        );
                    }
                }

                // And the non-empty alternative should match "done"
                let mut parser2 = Parser::new(grammar.clone());
                match parser2.parse_to_arena("done") {
                    Ok(_) => {
                        println!("✓ Successfully parsed 'done' with non-empty alternative");
                    }
                    Err(e) => {
                        panic!("Failed to parse 'done': {:?}", e);
                    }
                }
            }
            Err(e) => {
                panic!("Failed to parse grammar with empty alternative: {:?}", e);
            }
        }
    }

    #[test]
    fn test_premature_queue_empty_debug() {
        // Debug test case for premature queue empty issue
        // Grammar: doc: item++space. item: "x". space: " ".
        // Input: "x x"
        // Should parse as: <doc><item>x</item> <item>x</item></doc>

        use crate::debug::{DebugConfig, DebugLevel};
        use crate::grammar::{Grammar, RuleContext};

        // Set up trace debugging
        let debug_config = DebugConfig {
            level: DebugLevel::Trace,
            position_filter: None,
            failure_only: false,
            trace_file: Some("log/premature_queue_debug.log".to_string()),
            enabled_categories: None,
        };
        crate::debug::set_debug_config(debug_config);

        // Hand-code the grammar: doc: item++space. item: "x". space: " ".
        let mut g = Grammar::new();
        let ctx = RuleContext::new("doc");

        // doc: item++space (equivalent to repeat1_sep)
        g.define(
            "doc",
            ctx.seq().repeat1_sep(
                ctx.seq().nt("item"),  // repeated element
                ctx.seq().nt("space"), // separator
            ),
        );

        // item: "x"
        g.define("item", ctx.seq().ch('x'));

        // space: " "
        g.define("space", ctx.seq().ch(' '));

        println!("=== PREMATURE QUEUE EMPTY DEBUG ===");
        println!("Grammar: doc: item++space. item: \"x\". space: \" \".");
        println!("Input: x x");
        println!();

        // Parse input "x x"
        let mut parser = Parser::new(g);
        let input = "x x";

        match parser.parse_to_arena(input) {
            Ok(tree) => {
                println!("✓ Parse successful!");
                let xml_output = Parser::tree_to_test_format(&tree);
                println!("Result: {}", xml_output);
            }
            Err(e) => {
                println!("❌ Parse failed: {}", e);
                println!("See log/premature_queue_debug.log for trace details");
                panic!("Expected parse to succeed, got error: {}", e);
            }
        }
    }
}
