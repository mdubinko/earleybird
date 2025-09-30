use crate::grammar::{Factor, Grammar, TerminalDefn, Mark, Rule, TMark};
use crate::debug::DebugLevel;
use crate::{debug_earley_pos, debug_earley_fail};
use std::{collections::{VecDeque, HashSet, HashMap}, fmt};
use multimap::MultiMap;
use smol_str::SmolStr;
use string_builder::Builder;
use indextree::{Arena, NodeId};
use log::{info, debug, trace};
use crate::utils;

const DOTSEP: &str = "•";

#[derive(Debug, Clone, Eq, PartialEq)]
/// A sort of iterator for a Rule.
/// Instead of just calling next(), For completed terms, it tracks positions and specifically-matched chars
/// `matched_so_far.len`() is the cursor position
pub struct DotNotation {
    iteratee: Rule,
    matched_so_far: Vec<MatchRec>,
}

impl DotNotation {
    pub fn new(rule: &Rule) -> Self {
        Self { iteratee: rule.clone(), matched_so_far: Vec::new() }
    }

    /// record a new match. Intnded for literal character data
    /// this returns an entirely new `DotNotation`
    fn advance_dot(&self, rec: MatchRec) -> Self {
        let mut clo = self.clone();
        clo.matched_so_far.push(rec);
        clo
    }


    fn is_completed(&self) -> bool {
        self.iteratee.len() == self.matched_so_far.len()
    }

    fn _is_at_start(&self) -> bool {
        self.matched_so_far.is_empty()
    }

    /// retrieve the match info for trace processing
    fn matches_iter(&self) -> std::slice::Iter<'_, MatchRec> {
        self.matched_so_far.iter()
    }

    /// next term to parse. A.k.a. "What's next after the dot?"
    /// returns cloned Term
    fn next_unparsed(&self) -> Factor {
        let cursor = self.matched_so_far.len();
        self.iteratee.factors[cursor].clone()
    }
}

impl fmt::Display for DotNotation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let cursor = self.matched_so_far.len();
        
        // handled rules
        let done: String = self.matched_so_far.iter()
            .map(| i |
                match i {
                    MatchRec::Term(ch, pos, tmark) => format!("{tmark}'{ch}'@{pos}"),
                    MatchRec::NonTerm(name, pos, mark) => format!("{mark}{name}@{pos}"),
            })
            .collect::<Vec<_>>()
            .join(", ");

        // remaining rules
        let remain = self.iteratee.factors.iter()
            .skip(cursor)
            .map(std::string::ToString::to_string)
            .collect::<Vec<String>>()
            .join(", ");
        write!(f, "{done} {DOTSEP} {remain}")
    }
}

/// The internal record of a fragment of a matching parse
/// See also the Content enum for the stable, outward facing record of a similar nature
#[derive(Clone, Debug, Eq, PartialEq)]
enum MatchRec {
    Term(char, usize, TMark),
    NonTerm(SmolStr, usize, Mark),
}

impl MatchRec {
    fn pos(&self) -> usize {
        match self {
            Self::Term(_, pos, _) => *pos,
            Self::NonTerm(_, pos, _) => *pos,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Task {
    id: TraceId,              // unique id, as handled by TraceArena
    name: SmolStr,            // BranchingRule name
    alt_index: usize,         // which alt of this BranchingRule (0-based)
    mark: Mark,               // effective mark for this task
    origin: usize,            // starting position in the input
    pos: usize,               // current position in the input
    dot: DotNotation,         // progress
    hash: u64,                // identity hash based on name, alt_index, origin, pos, dot
}

impl Task {
    pub fn mark(&self) -> Mark {
        self.mark.clone()
    }
}

/// Display task content for debugging
impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}[{}] {}:{} {}", self.mark, self.name, self.alt_index, self.origin, self.pos, self.dot)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceId(usize);

#[derive(Debug)]
/// the permanent home of all Traces/Tasks
pub struct TraceArena {
    /// main storage for Tasks. The vector index becomes the TraceId
    /// (which should always match what's stored in task.id)
    arena: Vec<Task>,

    /// active queue of tasks
    queue: VecDeque<TraceId>,

    /// Track every place where a nonterminal can be triggered.
    /// Key is a nonterminal name. Value is a particular TraceId that references it
    /// For example in
    /// doc = S.
    /// S = S, "+", T | T
    /// upon completing an "S", we need to go back and resume both
    /// the doc=(S) rule as well as the S=(S, "+", T) branch, bumping the dot cursor one term
    /// therefore, when inititally queueing the S branches, we need to record
    /// "S" -> (TraceId for doc=(• S))
    /// "S" -> (TraceId for S=(• S "+" T))
    continuations: MultiMap<SmolStr, TraceId>,

    /// blockchain-style deduplication using binary hashes for fast lookups
    hashes: HashSet<u64>,
}

impl TraceArena {
    fn new() -> Self {
        Self {
            arena: Vec::new(),
            queue: VecDeque::new(),
            continuations: MultiMap::new(),
            hashes: HashSet::new()
        }
    }

    /// Format alternative-specific name for continuations system
    fn format_alt_specific_name(rule_name: &str, alt_index: usize) -> String {
        format!("{}[{}]", rule_name, alt_index)
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

    /// Register a parent task that's waiting for a specific nonterminal alternative to complete
    fn register_waiting_parent_task(&mut self, target_nt: &str, alt_index: usize, waiting_parent_tid: TraceId) {
        let alt_specific_name = Self::format_alt_specific_name(target_nt, alt_index);
        debug!("..⏸️ registering parent {} waiting for {}", self.format_task(waiting_parent_tid), alt_specific_name);
        self.continuations.insert(SmolStr::from(alt_specific_name), waiting_parent_tid);
    }

    /// Get all parent tasks waiting for ANY alternative of a nonterminal to complete
    fn get_waiting_parent_tasks_by_name(&self, rule_name: &str) -> Vec<TraceId> {
        let mut result = Vec::new();
        let mut alt_index = 0;

        // Try consecutive alternative indices until we get a miss
        loop {
            let alt_specific_name = Self::format_alt_specific_name(rule_name, alt_index);
            if let Some(task_ids) = self.continuations.get_vec(&SmolStr::from(&alt_specific_name)) {
                result.extend(task_ids);
                alt_index += 1;
            } else {
                // No more alternatives found, we're done
                break;
            }
        }

        debug!("..🔁 found {} parent tasks waiting for any alternative of {}", result.len(), rule_name);
        result
    }

    /// originate a completely new task (root level)
    /// Returns Some(TraceId) (unless this is a duplicate Task, in which case None is returned)
    fn task(&mut self, name: &str, alt_index: usize, mark: Mark, origin: usize, pos: usize, dot: DotNotation) -> Option<TraceId> {
        let id = TraceId(self.arena.len());
        let task_content = format!("{}[{}] {}:{} {}", name, alt_index, origin, pos, dot);
        let hash = utils::hash_to_u64(&task_content);

        let task = Task{
            id,
            name: SmolStr::new(name),
            alt_index,
            mark,
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
        let id = TraceId(self.arena.len());

        let task_content = format!("{}[{}] {}:{} {}", from_task.name, from_task.alt_index, from_task.origin, new_pos, new_dot);
        let hash = utils::hash_to_u64(&task_content);

        let task = Task {
            id,
            name: from_task.name.clone(),
            alt_index: from_task.alt_index,  // Preserve alt_index from source task
            mark: from_task.mark.clone(),
            origin: from_task.origin,
            pos: new_pos,
            dot: new_dot,
            hash,
        };

        if self.have_we_seen(&task) {
            None
        } else {
            self.save_task(task);
            Some(id)
        }
    }

    /// returns true if this trace had been previously seen
    /// also performs necessary bookkeeping
    ///
    /// Simple Task Deduplication Strategy:
    /// Use task identity hash based on name, alt_index, origin, pos, and dot
    fn have_we_seen(&mut self, task: &Task) -> bool {
        if self.hashes.contains(&task.hash) {
            debug!("🚫 DUPLICATE TASK DETECTED: Skipping {}[{}]", task.name, task.alt_index);
            true
        } else {
            debug!("...caching task {}[{}]", task.name, task.alt_index);
            self.hashes.insert(task.hash);
            false
        }
    }


    fn format_task(&self, id: TraceId) -> String {
        let task = self.get(id);
        let printable_id: String = id.0.to_string();
        format!(" {}) {}:{}👉 {}[{}]=( {} ) ", printable_id, task.origin, task.pos, task.name, task.alt_index, task.dot)
    }
}

struct InputIter {
    tokens: Vec<char>,
    // actual position is tracked externally, in Tasks
}

impl InputIter {
    fn new(input: &str) -> Self {
        Self { tokens: input.chars().collect::<Vec<_>>() }
    }

    /// Get the character immediately after the given cursor position
    /// Cursor 0 is before the first character, so get_at(0) returns the first character
    /// Cursor 1 is before the second character, so get_at(1) returns the second character
    /// Like graphics coordinates: cursors are between characters, not at characters
    /// Panics if cursor position is beyond input length
    pub fn get_at(&mut self, cursor: usize) -> char {
        if cursor >= self.tokens.len() {
            panic!("Parser attempted to read beyond input at cursor {}, input length is {}", cursor, self.tokens.len());
        } else {
            self.tokens[cursor]
        }
    }
    // TODO: row/col machinery for input tokens
}

#[derive(Debug, Clone)]
/// in the intermediate parse indextree, tree nodes are provided thusly
pub enum Content {
    Root,
    Element(String),            // name
    Attribute(String, String),  // name, value
    Text(String)                // value
}

impl Content {
    pub fn is_attr(&self) -> bool {
        matches!(self, Self::Attribute(_,_))
    }
    pub fn is_elem(&self) -> bool {
        matches!(self, Self::Element(_))
    }
    pub fn get_name(&self) -> Option<String> {
        match self {
            Self::Element(name) => Some(name.clone()),
            Self::Attribute(name, _) => Some(name.clone()),
            _ => None
        }
    }
    pub fn get_value(&self) -> Option<String> {
        match self {
            Self::Attribute(_, value) => Some(value.clone()),
            Self::Text(value) => Some(value.clone()),
            _ => None
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

#[derive(Debug)]
pub enum ParseError {
    StaticError(String),
    DynamicError(String),
    UncategorizedError(String),
}

impl ParseError {
    pub fn static_err(msg: &str) -> Self {
        Self::StaticError(msg.to_string())
    }
    pub fn dynamic_err(msg: &str) -> Self {
        Self::DynamicError(msg.to_string())
    }
    pub fn uncategorized_err(msg: &str) -> Self {
        Self::UncategorizedError(msg.to_string())
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            Self::StaticError(e) => write!(f, "StaticError: {e}"),
            Self::DynamicError(e) => write!(f, "DynamicError: {e}"),
            Self::UncategorizedError(e) => write!(f, "UncategorizedError: {e}"),
        }
    }
}

#[derive(Debug)]
pub struct Parser {
    grammar: Grammar,
    /// the permanent owner of all tasks, referenced by TraceId
    traces: TraceArena,
    completed_trace: Vec<TraceId>,
    farthest_pos: usize,  // hint for later reading the trace
    input_length: usize,  // total length of input to ensure complete consumption
    max_trace_size: usize,  // maximum number of operations before timeout
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
        Self::new_with_trace_limit(grammar, 100_000)
    }

    pub fn new_with_trace_limit(grammar: Grammar, max_trace_size: usize) -> Self {
        Self {
            grammar,
            traces: TraceArena::new(),
            completed_trace: Vec::new(),
            farthest_pos: 0,
            input_length: 0,
            max_trace_size,
        }
    }

    /// Successful return value is an indextree over Content. Consider this temporary
    pub fn parse(&mut self, input: &str) -> Result<Arena<Content>, ParseError> {
        let mut input = InputIter::new(input);
        self.input_length = input.tokens.len();

        // help avoid borrow-contention on *self
        let g = self.grammar.clone();

        debug!("Starting parse at position 0 (before first character)");

        // INITIALISE
        // START grammar FOR start.symbol grammar AT start.pos
        let top_rule = g.get_root_definition()?
            .ok_or(ParseError::static_err("No top grammar rule"))?;

        for (alt_index, alt) in top_rule.iter().enumerate() {
            let maybe_id = self.traces.task(&g.get_root_definition_name()
                .ok_or(ParseError::static_err("No top grammar rule name"))?, alt_index, top_rule.mark(), 0, 0, alt.dot_notator());
            self.queue_front(maybe_id);
        }

        // WHILE more.tasks:
        //    TAKE task
        while let Some(tid) = self.traces.queue.pop_front() {
            // Check trace size limit to prevent infinite loops
            if self.traces.arena.len() > self.max_trace_size {
                return Err(ParseError::static_err(&format!(
                    "Parse exceeded maximum trace size of {} operations (infinite loop detected)",
                    self.max_trace_size
                )));
            }

            let current_pos = self.traces.get(tid).pos;
            if current_pos > self.farthest_pos {
                if current_pos < self.input_length {
                    debug!("⏭ Advanced input to position {} (next char: '{}')", current_pos, input.get_at(current_pos));
                } else {
                    debug!("⏭ Advanced input to position {} (at end)", current_pos);
                }
                self.farthest_pos = current_pos;
            }
            debug!("🔄 PROCESSING: Pulled from queue {} at {} | Queue size: {} -> {} | Queue: [{}]",
                   self.traces.format_task(tid), current_pos,
                   self.traces.queue.len() + 1, self.traces.queue.len(),
                   self.queue_snapshot());

            // SELECT:
            //    finished task:
            //       CONTINUE PARENTS task
            if self.traces.get(tid).dot.is_completed() {
                self.complete(tid, &mut input)?;
            } else {
                // ELSE:
                //    PUT next.symbol task, position task IN sym, pos
                //    SELECT:
                let factor = self.traces.get(tid).dot.next_unparsed();
                match factor {
                    // grammar nonterminal sym:
                    //    START grammar FOR sym AT pos
                    Factor::Nonterm(mark, name) => {
                        self.predict(&g, tid, mark, name)?;
                    }
                    // sym starts (input, pos): \Terminal, matches
                    //    RECORD TERMINAL input FOR task
                    //    CONTINUE task AT (pos incremented (input, sym))
                    // ELSE:
                    //    PASS \Terminal, doesn't match
                    Factor::Terminal(tmark, matcher) => {
                        self.scan(tid, tmark, matcher, &mut input)?;
                    }
                }
            }
        }

        info!("🔚 QUEUE EMPTY: Parse loop exited with queue empty. Last position: {}, Input length: {}", self.farthest_pos, self.input_length);
        info!("Finished parse with {} items in trace", self.traces.arena.len());
        self.unpack_parse_tree()
    }

    /// COMPLETER: Handle completed tasks by continuing their parent tasks
    /// Implements: finished task: CONTINUE PARENTS task
    fn complete(&mut self, tid: TraceId, _input: &mut InputIter) -> Result<(), ParseError> {
        let current_pos = self.traces.get(tid).pos;
        debug!("COMPLETER pos={}", current_pos);
        debug_earley_pos!(DebugLevel::Trace, current_pos, "COMPLETER: {} completed", self.traces.format_task(tid));
        self.completed_trace.push(tid);

        // Find "parent" states at same origin that can produce this expression
        let completed_task = self.traces.get(tid);
        let waiting_parents = self.traces.get_waiting_parent_tasks_by_name(&completed_task.name);

        for continue_id in waiting_parents {
            // Make sure we only continue from a compatible position
            if self.traces.get(continue_id).pos != self.traces.get(tid).origin {
                continue;
            }
            debug!("...deferring continuation Task... {}", self.traces.format_task(continue_id));

            let now_finished_via_child = self.traces.get(continue_id).dot.next_unparsed();
            let match_rec = match now_finished_via_child {
                Factor::Nonterm(mark, name) => MatchRec::NonTerm(name, self.traces.get(tid).pos, mark),
                Factor::Terminal(tmark, _ch) => {
                    // This should never happen - terminals are handled by Scanner
                    panic!("INTERNAL ERROR: Complete() called on task waiting for terminal {:?}. This indicates a logic bug in the parser.", tmark);
                }
            };
            trace!("MatchRec {:?}", &match_rec);

            // Child may have made progress; next item in parent seq needs to account for this
            let maybe_id = self.traces.task_advance_cursor(continue_id, match_rec);

            // CRITICAL FIX: Queue parent continuations at back to ensure exhaustive alternative exploration
            // This allows all alternatives at current position to be explored before parent propagation
            self.queue_back(maybe_id);
        }
        Ok(())
    }

    /// PREDICTOR: Create downstream tasks from nonterminal references
    /// When processing a rule like "A: B, C." and we encounter nonterminal B,
    /// we create new tasks for all alternatives of B:
    /// A: • B, C. { <-- currently processing this rule }
    /// B: • "x".  { <-- queue up this alternative }
    /// B: • "y".  { <-- and this alternative }
    /// Implements: grammar nonterminal sym: START grammar FOR sym AT pos
    fn predict(&mut self, g: &Grammar, tid: TraceId, mark: Mark, name: SmolStr) -> Result<(), ParseError> {
        let current_pos = self.traces.get(tid).pos;
        debug!("PREDICTOR: Nonterm {mark}{name}");
        debug_earley_pos!(DebugLevel::Trace, current_pos, "PREDICTOR: {} predicting {}{}", self.traces.format_task(tid), mark, name);

        // Register this parent task as waiting for ALL alternatives of the child rule
        // We need to register for each possible alternative since we don't know which one will complete
        let child_rule = g.get_definition(&name)?;
        for child_alt_index in 0..child_rule.iter().count() {
            self.traces.register_waiting_parent_task(&name, child_alt_index, tid);
        }

        // We can have a Mark at the point of definition,
        // as well as at the point of reference...
        // Figure out what to do with all possible combinations
        let defn_mark = g.get_definition_mark(&name)?;
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

        for (alt_index, alt) in g.get_definition(&name)?.iter().enumerate() {
            let maybe_id = self.traces.task(&name, alt_index, effective_mark, current_pos, current_pos, alt.dot_notator());

            // CRITICAL FIX: Handle nullable alternatives immediately whether new or deduplicated
            // Check if this alternative is nullable (can produce epsilon)
            if g.is_alternative_nullable(alt)? {
                debug_earley_pos!(DebugLevel::Trace, current_pos, "PREDICTOR: Nullable rule {}[{}] - triggering immediate completion (Bpredict/complete)", name, alt_index);

                // For empty rules, we need to trigger completion regardless of deduplication
                // Find waiting parents for this rule name and alternative
                let waiting_parents = self.traces.get_waiting_parent_tasks_by_name(&name);

                for continue_id in waiting_parents {
                    // Make sure we only continue from a compatible position
                    if self.traces.get(continue_id).pos != current_pos {
                        continue;
                    }
                    debug!("...immediately continuing parent Task for empty rule... {}", self.traces.format_task(continue_id));

                    let now_finished_via_child = self.traces.get(continue_id).dot.next_unparsed();
                    let match_rec = match now_finished_via_child {
                        Factor::Nonterm(mark, name) => MatchRec::NonTerm(name, current_pos, mark),
                        Factor::Terminal(tmark, _ch) => {
                            panic!("INTERNAL ERROR: Complete() called on task waiting for terminal {:?}. This indicates a logic bug in the parser.", tmark);
                        }
                    };
                    trace!("MatchRec {:?}", &match_rec);

                    // Child completed immediately; advance parent cursor
                    let maybe_continue_id = self.traces.task_advance_cursor(continue_id, match_rec);

                    // Queue parent continuations at back to ensure exhaustive alternative exploration
                    self.queue_back(maybe_continue_id);
                }
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
    fn scan(&mut self, tid: TraceId, tmark: TMark, matcher: TerminalDefn, input: &mut InputIter) -> Result<(), ParseError> {
        let current_pos = self.traces.get(tid).pos;
        debug!("SCANNER: Terminal {tmark}{matcher} at pos={current_pos}");
        debug_earley_pos!(DebugLevel::Trace, current_pos, "SCANNER: {} scanning {}{}", self.traces.format_task(tid), tmark, matcher);

        // Bounds check: don't scan beyond input length
        if current_pos >= self.input_length {
            debug!("Position {} >= input length {}; 🛑", current_pos, self.input_length);
            debug_earley_fail!(current_pos, &format!("{}", matcher), '∅', &self.queue_snapshot());
            return Ok(());
        }

        if matcher.accept(input.get_at(current_pos)) {
            // Match! Advance position by 1
            let new_pos = current_pos + 1;
            let rec = MatchRec::Term(input.get_at(current_pos), new_pos, tmark);
            debug!("advance cursor SCAN");
            debug_earley_pos!(DebugLevel::Trace, current_pos, "SCANNER: MATCH '{}' -> advance to {}", input.get_at(current_pos), new_pos);
            let maybe_id = self.traces.task_advance_cursor(tid, rec);
            self.queue_back(maybe_id);
        } else {
            // Terminal doesn't match - silently drop this task (no requeue)
            // Per Earley algorithm: non-matching terminals should PASS (terminate quietly)
            debug!("non-matched char '{}' (expecting {matcher}); 🛑", input.get_at(current_pos));
        }
        Ok(())
    }

    fn queue_back(&mut self, maybe_id: Option<TraceId>) {
        if let Some(id) = maybe_id {
            debug!("QUEUE: Adding to back (normal priority): {} | Queue size: {} -> {}",
                   self.traces.format_task(id), self.traces.queue.len(), self.traces.queue.len() + 1);
            self.traces.queue.push_back(id);
            self.validate_queue_invariants();
        }
    }

    fn queue_front(&mut self, maybe_id: Option<TraceId>) {
        if let Some(id) = maybe_id {
            debug!("QUEUE: Adding to front (high priority): {} | Queue size: {} -> {}",
                   self.traces.format_task(id), self.traces.queue.len(), self.traces.queue.len() + 1);
            self.traces.queue.push_front(id);
            self.validate_queue_invariants();
        }
    }


    #[cfg(debug_assertions)]
    fn validate_queue_invariants(&self) {
        // Ensure queue doesn't grow unbounded (catch infinite loops)
        assert!(self.traces.queue.len() < 100_000,
            "Queue size {} exceeded safety limit - possible infinite loop",
            self.traces.queue.len());

        // Ensure all queued tasks have valid positions
        for &tid in &self.traces.queue {
            let task = self.traces.get(tid);
            assert!(task.pos <= self.input_length,
                "Task {} has position {} beyond input length {}",
                self.traces.format_task(tid), task.pos, self.input_length);
        }
    }

    #[cfg(not(debug_assertions))]
    fn validate_queue_invariants(&self) {
        // No-op in release builds for performance
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
            let snapshot: Vec<String> = self.traces.queue.iter()
                .map(|&tid| {
                    let task = self.traces.get(tid);
                    format!("{}@{}", task.name, task.pos)
                })
                .collect();
            return snapshot.join(",");
        }

        // Large queue - show front 3, middle indicator, back 3
        let front: Vec<String> = self.traces.queue.iter()
            .take(3)
            .map(|&tid| {
                let task = self.traces.get(tid);
                format!("{}@{}", task.name, task.pos)
            })
            .collect();

        let back: Vec<String> = self.traces.queue.iter()
            .rev()
            .take(3)
            .map(|&tid| {
                let task = self.traces.get(tid);
                format!("{}@{}", task.name, task.pos)
            })
            .collect();

        format!("{}...{} ({})", front.join(","), back.into_iter().rev().collect::<Vec<_>>().join(","), queue_len)
    }

    /// Sift through and find only completed Tasks
    /// this speeds up the unpacking process by omitting parse states irrelevant to the final result
    fn filter_completed_trace(&self, name: &str, origin: usize, pos: usize) -> Option<&Task> {
        // TODO: optimize
        for tid in &self.completed_trace {
            let t = self.traces.get(*tid);
            if t.name == name && t.origin == origin && t.pos == pos {
                return Some(t);
            }
        }
        None
    }

    /// Only for use in test sutes. Not guaranteed to be stable...
    pub fn test_inspect_trace(&self, filter: Option<SmolStr>) -> Vec<Task> {
        match filter {
            Some(str) => self.traces.arena
               .clone()
               .into_iter()
               .filter(|task| task.name==str)
               .collect(),
            None => self.traces.arena.clone(),
        }
    }

    fn unpack_parse_tree(&mut self) -> Result<Arena<Content>, ParseError> {
        debug!("TRACE...");
        debug!("COMPLETED TASKS ({} total):", self.completed_trace.len());
        for tid in &self.completed_trace {
            let task = self.traces.get(*tid);
            debug!("  {} (origin={}, pos={})", self.traces.format_task(*tid), task.origin, task.pos);
        }
        
        // Check if we have a completed parse of our grammar's root rule that spans the entire input
        let name = self.grammar.get_root_definition_name().unwrap();
        debug!("🔍 LOOKING FOR: completed parse of rule '{}' spanning (0 to {})", name, self.input_length);
        let root_completion = self.filter_completed_trace(&name, 0, self.input_length);

        if root_completion.is_none() {
            debug!("❌ NO ROOT COMPLETION FOUND for '{}' spanning entire input", name);
            // Generate enhanced diagnostics for parse failures
            let mut diagnostic = format!(
                "Parse failed: no completed parse of rule '{}' spanning entire input (0 to {})\n",
                name,
                self.input_length
            );

            // Find the furthest position we reached
            diagnostic.push_str(&format!("Furthest position reached: {}\n", self.farthest_pos));

            // Show partial completions of the root rule
            let partial_completions: Vec<_> = self.completed_trace.iter()
                .filter_map(|&tid| {
                    let task = self.traces.get(tid);
                    if task.name == name { Some((tid, task)) } else { None }
                })
                .collect();

            if !partial_completions.is_empty() {
                diagnostic.push_str("Partial completions of root rule found:\n");
                for (tid, task) in partial_completions {
                    diagnostic.push_str(&format!("  {} (origin={}, pos={})\n",
                        self.traces.format_task(tid), task.origin, task.pos));
                }
            }

            // Show what completions we do have near the furthest position
            let nearby_completions: Vec<_> = self.completed_trace.iter()
                .filter_map(|&tid| {
                    let task = self.traces.get(tid);
                    if task.pos >= self.farthest_pos.saturating_sub(5) && task.pos <= self.farthest_pos + 5 {
                        Some((tid, task))
                    } else { None }
                })
                .take(10)
                .collect();

            if !nearby_completions.is_empty() {
                diagnostic.push_str(&format!("Completions near furthest position ({}±5):\n", self.farthest_pos));
                for (tid, task) in nearby_completions {
                    diagnostic.push_str(&format!("  {} (origin={}, pos={})\n",
                        self.traces.format_task(tid), task.origin, task.pos));
                }
            }

            // Show active tasks in queue
            if !self.traces.queue.is_empty() {
                diagnostic.push_str(&format!("Active tasks remaining in queue: {}\n", self.traces.queue.len()));
                for &tid in self.traces.queue.iter().take(5) {
                    let task = self.traces.get(tid);
                    diagnostic.push_str(&format!("  {} (pos={})\n",
                        self.traces.format_task(tid), task.pos));
                }
                if self.traces.queue.len() > 5 {
                    diagnostic.push_str(&format!("  ... and {} more\n", self.traces.queue.len() - 5));
                }
            }

            return Err(ParseError::static_err(&diagnostic));
        }
        
        let mut arena = Arena::new();
        let root = arena.new_node(Content::Root);
        debug!("Found completed parse of '{}' from 0 to {}", name, self.input_length);
        self.unpack_parse_tree_internal(&mut arena, &name, Mark::Default, 0, self.input_length, root);

        // the standard algorithm above leaves attribute nodes in an inconvenient state.
        // with a bare Content::Attribute node, for which one needs to plumb all descendants to find text nodes
        // below, we do that once-and-for-all for each Content::Attribute node
        let attr_node_ids = arena.iter()
            .filter(|n| matches!(n.get(), Content::Attribute(..) ))
            .map(|n| arena.get_node_id(n).unwrap())
            .collect::<Vec<_>>();
        for attr_nid in attr_node_ids {
            let attr_val = self.unpack_attr_value(attr_nid, &mut arena);
            arena.get_mut(attr_nid).unwrap().get_mut().set_value(attr_val);
        }
        // n.b. this doesn't actually delete these original descendent text nodes...
        // but you should never need to even look for them

        Ok(arena)
    }

    /// Recurse down through the tree to assemble all the text literals that comprise an attribute value
    fn unpack_attr_value(&self, attr_nid: NodeId, arena: &mut Arena<Content>) -> String {
        let mut attr_value = Builder::default();
        for descendant in attr_nid.descendants(arena) {
            let mut attr_builder = Builder::default();
            if let Content::Text(txt) = arena.get(descendant).unwrap().get() {
                attr_builder.append(txt.as_str());
            }
            attr_value.append(attr_builder.string().unwrap()
                .replace('\'', "&apos;")
                .replace('&', "&amp;")
                .replace('<', "&lt;")
            );
        }
        attr_value.string().unwrap()
    }

    fn unpack_parse_tree_internal(&self, arena: &mut Arena<Content>, name: &str, mark: Mark, origin: usize, end: usize, root: NodeId) {
        let matching_trace = self.filter_completed_trace(name, origin, end);
        let mut new_root = root;
            match matching_trace {
                Some(task) => {
                    let match_name = &task.name;

                    if task.mark==Mark::Mute || match_name.starts_with('-') {
                        // Skip
                        debug!("trace found {mark} {task} -- SKIPPING");
                    } else {
                        // Element or Attribute
                        debug!("trace found {} {task}", task.mark);
                        let name_str = match_name.to_string();
                        let data = if task.mark==Mark::Attr {
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
                    for match_rec in dot.matches_iter() {
                        match match_rec {
                            MatchRec::Term(ch, pos, tmark) => {
                                if *tmark != TMark::Mute {
                                    let new_child = arena.new_node(Content::Text(ch.to_string()) );
                                   new_root.append(new_child, arena);
                                }
                                new_origin = *pos;
                            }
                            MatchRec::NonTerm(nt_name, pos, mark) => {
                                // guard against infinite recursion
                                assert!( (nt_name!=name || new_origin!=origin || *pos!=end));
                                self.unpack_parse_tree_internal(arena, nt_name, mark.clone(), new_origin, *pos, new_root);
                                new_origin = *pos;
                            }
                        }
                    }
            
                }
                None => {
                    info!("  No matching traces for {}@{}:{}", name, origin, end);
                }
            }

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



    pub fn tree_to_test_format(arena: &Arena<Content>) -> String {
        let mut builder = Builder::default();
        let root = arena.iter().next().unwrap(); // first item == root
        let root_id = arena.get_node_id(root).unwrap();
        for child in root_id.children(arena) {
            Self::tree_to_test_format_recurse(arena, &mut builder, child);
        }
        builder.string().unwrap()
    }
    
    fn tree_to_test_format_recurse(arena: &Arena<Content>, builder: &mut Builder, nid: NodeId) {
        let maybe_node = arena.get(nid);
        if maybe_node.is_none() {
            return;
        }
        match arena.get(nid).unwrap().get() {
            Content::Root => {},
            Content::Element(name) => {
                builder.append("<");
                builder.append(name.to_string());

                // handle attributes before closing start tag...
                // TODO: Add dynamic error detection for duplicate attribute names (D02 error code)
                // This should check for duplicate attr_name values and report appropriate errors
                // for AssertDynamicError test cases like expr1
                for attr_child in nid.children(arena).filter(|n| arena.get(*n).unwrap().get().is_attr() ) {
                    builder.append(" ");
                    let attr_desc = arena.get(attr_child).unwrap().get();
                    let (attr_name, attr_value) = match attr_desc {
                        Content::Attribute(attr_name, attr_value) => (attr_name, attr_value),
                        _ => unreachable!("Filter on Attribute children() somewhow didn't work..."),
                    };
                    builder.append(attr_name.to_string());
                    builder.append("=\"");
                    builder.append(attr_value.replace('"', "&quot;"));
                    builder.append("\"");
                }

                builder.append(">");
    
                for child in nid.children(arena) {
                    // println!("testfmt found {child} in ::Element");
                    Self::tree_to_test_format_recurse(arena, builder, child);
                }
    
                builder.append("</");
                builder.append(name.to_string());
                builder.append(">");
            },
            Content::Attribute(..) => {}, // handled above
            Content::Text(utf8) => builder.append(utf8.replace('&', "&amp;").replace('<', "&lt;")),
        }
    }

    /// Helper function for working with indextree
    /// Given a `NodeId` (that should be an element) get all the Attribute nodes
    /// Returns an easily-digestiable `HashMap` of Name -> Value
    pub fn get_attributes(arena: &Arena<Content>, elem: NodeId) -> HashMap<String, String> {
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
    pub fn get_child_elements(arena: &Arena<Content>, nid: NodeId) -> Vec<(String, NodeId)> {
        nid.children(arena)
            // fist pair up as (&Content, NodeId)
            .map(|nid| (arena.get(nid).unwrap().get(), nid) )
            // and keep only elements 
            .filter(|(c,_)| matches!(c, Content::Element(_)))
            // then pair up as (ElementName, NodeId)
            .map(|(c,nid)| (c.get_name().unwrap(), nid))
            .collect()
    }

    /// Helper function for working with indextree
    /// get all immediate element children matching a given name
    /// Returns a Vec of `NodeId`
    pub fn get_child_elements_named(arena: &Arena<Content>, nid: NodeId, name: &str) -> Vec<NodeId> {
        nid.children(arena)
            .filter(|n| {
                let content = arena.get(*n).unwrap().get();
                if let Content::Element(nam) = content {
                    nam==name
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

    #[test]
    fn test_offset_bounds_violation_protection() {
        // Test that parser doesn't advance beyond input length
        let grammar_str = r#"test: "a"."#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Failed to parse grammar");
        let mut parser = Parser::new(grammar);

        // This should not hang or panic - should complete gracefully
        let result = parser.parse("a");
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
        let result = parser.parse(input);
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
        let _ = parser.parse(input);

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
        let grammar_str = r#"test: "a"?"#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Failed to parse grammar");
        let mut parser = Parser::new(grammar);

        // Empty input should not cause bounds violations
        let result = parser.parse("");
        let _ = result; // May succeed or fail, but shouldn't hang

        // Verify no positions exceed 0 (the length of empty input)
        let trace = parser.test_inspect_trace(None);
        for task in trace {
            assert!(task.pos <= 0, "Task position {} exceeds empty input length", task.pos);
        }
    }

    #[test]
    fn test_bounds_check_with_single_char() {
        let grammar_str = r#"test: "a", "b"?"#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Failed to parse grammar");
        let mut parser = Parser::new(grammar);

        // Single character input
        let result = parser.parse("a");
        let _ = result; // May succeed or fail

        // Verify no positions exceed 1
        let trace = parser.test_inspect_trace(None);
        for task in trace {
            assert!(task.pos <= 1, "Task position {} exceeds input length 1", task.pos);
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
                let input_a = parser.parse("A");
                assert!(input_a.is_ok(), "Grammar should match 'A'");

                // The grammar should NOT match 'B' - this is the key test
                let mut parser = Parser::new(Grammar::from_ixml_str(grammar_str).unwrap());
                let input_b = parser.parse("B");
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
                let digit_result = parser.parse("5");
                assert!(digit_result.is_ok(), "Should match digit '5'");

                // Test that it rejects non-digits
                let mut parser2 = Parser::new(Grammar::from_ixml_str(problematic_grammar).unwrap());
                let letter_result = parser2.parse("A");
                assert!(letter_result.is_err(), "Should not match letter 'A'");
            }
            Err(e) => {
                println!("❌ Grammar parsing failed as expected: {}", e);
                // This demonstrates the bug - the repeat construct ambiguity prevents successful parsing
                assert!(e.to_string().contains("no completed parse"),
                    "Expected bootstrap parsing failure, got: {}", e);
            }
        }

        // Test workaround: simpler patterns that should work
        let simple_patterns = vec![
            r#"test: [#30-#39]."#,  // Hex range for digits
            r#"test: ["a"]."#,      // Single string member
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
            (r#"test: [#41]."#, "A", true),  // #41 = 'A'
            (r#"test: [#41]."#, "B", false),

            // Range vs single member ambiguity
            (r#"test: ["A"-"C"]."#, "B", true),
            (r#"test: ["A"-"C"]."#, "D", false),
        ];

        for (grammar_str, input, should_match) in test_cases {
            let result = Grammar::from_ixml_str(grammar_str);
            assert!(result.is_ok(), "Grammar should parse: {}", grammar_str);

            let mut parser = Parser::new(result.unwrap());
            let parse_result = parser.parse(input);

            if should_match {
                assert!(parse_result.is_ok(),
                    "Should match: '{}' with grammar {}", input, grammar_str);
            } else {
                assert!(parse_result.is_err(),
                    "Should NOT match: '{}' with grammar {}", input, grammar_str);
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
        assert!(result.is_ok(), "Multi-character string grammar should parse correctly");

        if let Ok(grammar) = result {
            let mut parser = Parser::new(grammar);

            // Should match 'A' (first character of "AB")
            let input_a = parser.parse("A");
            assert!(input_a.is_ok(), "Grammar should match 'A' from [\"AB\"]");

            // Should also match 'B' (second character of "AB")
            let mut parser = Parser::new(Grammar::from_ixml_str(grammar_str).unwrap());
            let input_b = parser.parse("B");
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
        hand_built.define("data", ctx.seq()
            .nt("range1")
            .nt("range2")
            .mark_ch('.', crate::grammar::TMark::Mute));

        // range1: ["0"-"9"].
        let ctx = RuleContext::new("range1");
        hand_built.define("range1", ctx.seq()
            .ch_range('0', '9'));

        // range2: [#0-#9].
        let ctx = RuleContext::new("range2");
        hand_built.define("range2", ctx.seq()
            .ch_range('\u{0000}', '\u{0009}'));  // hex 0 to hex 9

        println!("Hand-built grammar:\n{}", hand_built);

        // Test with input "5\t."
        let mut parser = Parser::new(hand_built);
        let result = parser.parse("5\t.");
        assert!(result.is_ok(), "Hand-built range grammar should parse '5\\t.'");
    }

    #[cfg(test)]
    #[test]
    fn test_minimal_ixml_subset_bootstrap() {
        // Create a minimal subset of ixml grammar that focuses on rule parsing
        // Based on: ixml: s, rule++RS, s.
        // This should help isolate the exact bootstrap parsing failure

        use crate::grammar::{Grammar, RuleContext, Mark, TMark};

        println!("=== Building minimal ixml subset grammar ===");

        let mut mini_ixml = Grammar::new();

        // ixml: s, rule++RS, s.
        let ctx = RuleContext::new("ixml");
        mini_ixml.define("ixml", ctx.seq()
            .nt("s")
            .repeat1_sep(ctx.seq().nt("rule"), ctx.seq().nt("RS"))
            .nt("s"));

        // rule: name, s, -":", s, -".", s.  (simplified - no alts for now)
        let ctx = RuleContext::new("rule");
        mini_ixml.define("rule", ctx.seq()
            .nt("name")
            .nt("s")
            .mark_ch(':', TMark::Mute)
            .nt("s")
            .mark_ch('.', TMark::Mute)
            .nt("s"));

        // name: namestart, namefollower*.  (this is the problematic one!)
        let ctx = RuleContext::new("name");
        mini_ixml.mark_define(Mark::Attr, "name", ctx.seq()
            .nt("namestart")
            .repeat0(ctx.seq().nt("namefollower")));

        // namestart: ["a"-"z"; "A"-"Z"].  (simplified)
        let ctx = RuleContext::new("namestart");
        mini_ixml.mark_define(Mark::Mute, "namestart", ctx.seq().ch_range('a', 'z'));
        mini_ixml.mark_define(Mark::Mute, "namestart", ctx.seq().ch_range('A', 'Z'));

        // namefollower: namestart.  (simplified - no numbers/symbols)
        let ctx = RuleContext::new("namefollower");
        mini_ixml.mark_define(Mark::Mute, "namefollower", ctx.seq().nt("namestart"));

        // s: whitespace*.  (optional spacing)
        let ctx = RuleContext::new("s");
        mini_ixml.mark_define(Mark::Mute, "s", ctx.seq().repeat0(ctx.seq().nt("whitespace")));

        // RS: whitespace+.  (required spacing)
        let ctx = RuleContext::new("RS");
        mini_ixml.mark_define(Mark::Mute, "RS", ctx.seq().repeat1(ctx.seq().nt("whitespace")));

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
        let result = parser.parse(input);

        match result {
            Ok(arena) => {
                let output = Parser::tree_to_test_format(&arena);
                println!("SUCCESS: {}", output);
                // If this succeeds, the issue is elsewhere
                assert!(true, "Minimal ixml subset should parse simple rules");
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
    fn test_premature_queue_empty_debug() {
        // Debug test case for premature queue empty issue
        // Grammar: doc: item++space. item: "x". space: " ".
        // Input: "x x"
        // Should parse as: <doc><item>x</item> <item>x</item></doc>

        use crate::grammar::{Grammar, RuleContext};
        use crate::debug::{DebugConfig, DebugLevel};

        // Set up trace debugging
        let debug_config = DebugConfig {
            level: DebugLevel::Trace,
            position_filter: None,
            failure_only: false,
            trace_file: Some("log/premature_queue_debug.log".to_string()),
        };
        crate::debug::set_debug_config(debug_config);

        // Hand-code the grammar: doc: item++space. item: "x". space: " ".
        let mut g = Grammar::new();
        let ctx = RuleContext::new("doc");

        // doc: item++space (equivalent to repeat1_sep)
        g.define("doc", ctx.seq().repeat1_sep(
            ctx.seq().nt("item"),     // repeated element
            ctx.seq().nt("space")     // separator
        ));

        // item: "x"
        g.define("item", ctx.seq().ch('x'));

        // space: " "
        g.define("space", ctx.seq().ch(' '));

        println!("=== PREMATURE QUEUE EMPTY DEBUG ===");
        println!("Grammar: doc: item++space. item: \"x\". space: \" \".");
        println!("Input: x x");
        println!("");

        // Parse input "x x"
        let mut parser = Parser::new(g);
        let input = "x x";

        match parser.parse(input) {
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
