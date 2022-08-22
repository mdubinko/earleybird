use crate::grammar::{Grammar, Rule, Factor, TMark, Mark};
use std::{collections::{VecDeque, HashSet}, fmt};
use multimap::{MultiMap};
use smol_str::SmolStr;
use string_builder::Builder;
use indextree::{Arena, NodeId};
use log::{info, debug, trace};

// TODO: logs ^^^
// TODO: just make parser.parse return indextree::Arena

const DOTSEP: &'static str = "•";

#[derive(Debug, Clone, Eq, PartialEq)]
/// A sort of iterator for a Rule.
/// Instead of just calling next(), For completed terms, it tracks positions and specifically-matched chars
/// matched_so_far.len() is the cursor position
pub struct DotNotation {
    iteratee: Rule,
    matched_so_far: Vec<MatchRec>,
}

impl DotNotation {
    pub fn new(rule: &Rule) -> DotNotation {
        DotNotation { iteratee: rule.clone(), matched_so_far: Vec::new() }
    }

    /// record a new match. Intnded for literal character data
    /// this returns an entirely new DotNotation
    fn advance_dot(&self, rec: MatchRec) -> DotNotation {
        let mut clo = self.clone();
        clo.matched_so_far.push(rec);
        clo
    }

    fn is_completed(&self) -> bool {
        self.iteratee.len() == self.matched_so_far.len()
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
                    MatchRec::Term(ch, pos, tmark) => format!("'{tmark}{ch}'@{pos}"),
                    MatchRec::NonTerm(name, pos, mark) => format!("{mark}{name}@{pos}"),
            })
            .collect::<Vec<_>>()
            .join(", ");

        // remaining rules
        let remain = self.iteratee.factors.iter()
            .skip(cursor)
            .map(|t| t.to_string())
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
            MatchRec::Term(_, pos, _) => pos.clone(),
            MatchRec::NonTerm(_, pos, _) => pos.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Task {
    id: TraceId,              // unique id, as handled by TraceArena
    name: SmolStr,            // rule name
    mark: Mark,               // effective mark for this task
    origin: usize,            // starting position in the input
    pos: usize,               // current position in the input
    dot: DotNotation,         // progress
}

impl Task {
    /// returns a cloned MatchRec
    fn last_completed(&self) -> Option<MatchRec> {
        self.dot.matched_so_far.last().map(|x| x.clone())
    }
    pub fn mark(&self) -> Mark {
        self.mark.clone()
    }
}

/// This is currently ONLY used as a hash of Task, and can probably be optimized
impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "( {}{} {}:{} {}) ", self.mark, self.name, self.origin, self.pos, self.dot)
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

    /// a simple yes/no test if we've seen this exact Task before
    hashes: HashSet<String>,
}

impl TraceArena {
    fn new() -> TraceArena {
        TraceArena {
            arena: Vec::new(),
            queue: VecDeque::new(),
            continuations: MultiMap::new(),
            hashes: HashSet::new()
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

    /// record the continuation of a Task
    fn save_continuation(&mut self, target_nt: &str, tid: TraceId) {
        println!("..⏸️ saving continuation {target_nt}->{:?}", tid);
        self.continuations.insert(SmolStr::from(target_nt), tid);
    }

    /// retrieve the continuation for a Task
    /// if nothing found, returns an empty Vec
    fn get_continuations_for(&self, target_nt: SmolStr) -> Vec<TraceId> {
        let maybe_val = self.continuations.get_vec(&target_nt);
        let result = maybe_val.unwrap_or(&Vec::new()).to_vec();
        println!("..🔁 retrieving continuation {target_nt} containing {} entries", result.len());
        result
    }

    /// originate a completely new task
    /// Returns Some(TraceId) (unless this is a duplicate Task, in which case None is returned)
    fn task(&mut self, name: &str, mark: Mark, origin: usize, pos: usize, dot: DotNotation) -> Option<TraceId> {
        let id = TraceId(self.arena.len());
        let task = Task{ id, name: SmolStr::new(name), mark, origin, pos, dot };
        if self.have_we_seen(&task) {
            None
        } else {
            self.save_task(task);
            Some(id)
        }
    }
    
    /// originate a new Task, "downstream" from another task, like
    /// ... = x { <-- processing this rule }
    /// x = ... { <-- so queue up this one next, at same pos, etc. }
    /// Returns Some(TraceId) (unless this is a duplicate Task, in which case None is returned)
    fn task_downstream(&mut self, name: &str, mark: Mark, origin: usize, pos: usize, dot: DotNotation) -> Option<TraceId> {
        let id = TraceId(self.arena.len());
        let task = Task{ id, name: SmolStr::new(name), mark, origin, pos, dot };
        if self.have_we_seen(&task) {
            None
        } else {
            self.save_task(task);
            Some(id)
        }
    }
    
    /// clone a task, except advancing the cursor (storing given MatchRec for the piece just advanced-over)
    /// Maintains the same parentage, and position
    fn task_advance_cursor(&mut self, from: TraceId, rec: MatchRec) -> Option<TraceId> {
        let new_pos = rec.pos();
        
        let from_task = self.get(from);
        let new_dot = from_task.dot.advance_dot(rec);
        let id = TraceId(self.arena.len());
        // use from_task.mark? Or take from MatchRec?
        let task = Task { id, name: from_task.name.clone(), mark: from_task.mark.clone(), origin: from_task.origin, pos: new_pos, dot: new_dot };
        if self.have_we_seen(&task) {
            None
        } else {
            self.save_task(task);
            Some(id)
        }
    }

    /// returns true if this trace had been previously seen
    /// also performs necessary bookkeeping
    fn have_we_seen(&mut self, task: &Task) -> bool {
        let hash = task.to_string();
        if self.hashes.contains(&hash) {
            println!("...Skipping this task -- previously seen {} @ {}:{} {}", task.name, task.origin, task.pos, hash);
            true
        } else {
            println!("...caching task {}", hash);
            self.hashes.insert(hash);
            false
        }
    }

    fn format_task(&self, id: TraceId) -> String {
        let task = self.get(id);
        let printable_id: String = id.0.to_string();
        format!(" {}) {}:{}👉 {}=( {} ) ", printable_id, task.origin, task.pos, task.name, task.dot)
    }
}

struct InputIter {
    pos: usize,
    farthest_pos: usize,
    at_eof: bool,
    tokens: Vec<char>,
}

impl InputIter {
    fn new(input: &str) -> InputIter {
        InputIter {
            pos: 0,
            farthest_pos: 0,
            at_eof: 0 == input.len(),
            tokens: input.chars().collect::<Vec<_>>() }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn farthest_pos(&self) -> usize {
        self.farthest_pos
    }

    pub fn at_eof(&self) -> bool {
        self.at_eof 
    }

    pub fn get_tok(&self) -> char {
        if self.at_eof {
            '\x1f' // EOF char
        } else {
            self.tokens[self.pos]
        }
    }

    pub fn next(&mut self, amount: usize) -> (char, usize) {
        let new_pos = self.pos + amount;
        if new_pos >= self.tokens.len() {
            self.pos = self.tokens.len(); // one past end
            self.at_eof = true;
            println!("Reached EOF at position {}", self.pos());
        } else {
            self.pos += amount;
            println!("⏭ Advanced input to position {} (='{}')", self.pos(), self.get_tok());
        }
        self.farthest_pos = self.pos;
        (self.get_tok(), self.pos())
    }

    // TODO: row/col machinery for input tokens
}

#[derive(Debug, Clone)]
/// in the intermediate parse indextree, tree nodes are provided thusly
pub enum Content {
    Root,
    Element(String),            // name
    Attribute(String),          // name
    Text(String)                // value
}

impl Content {
    pub fn is_attr(&self) -> bool {
        match self {
            Content::Attribute(_) => true,
            _ => false,
        }
    }
    pub fn is_elem(&self) -> bool {
        match self {
            Content::Element(_) => true,
            _=> false,
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
}

/// Earley parser
impl Parser {

    pub fn new(grammar: Grammar) -> Self {
        Self {
            grammar,
            traces: TraceArena::new(),
            completed_trace: Vec::new(),
            farthest_pos: 0,
        }
    }

    pub fn parse(&mut self, input: &str) {
        let mut input = InputIter::new(input);

        // help avoid borrow-contention on *self
        let g = self.grammar.clone();
    
        println!("Input now at position {} '{}'", input.pos(), input.get_tok());

        // Seed with top expr
        let top_rule = g.get_root_definition();
        for alt in top_rule.iter() {
            let maybe_id = self.traces.task(g.get_root_definition_name(), top_rule.mark(), 0, 0, alt.dot_notator());
            self.queue_front(maybe_id);
        }
        // work through the queue
        while let Some(tid) = self.traces.queue.pop_front() {
            println!("Pulled from queue {}", self.traces.format_task(tid));

            let is_completed = self.traces.get(tid).dot.is_completed();

            // task in completed state?
            if is_completed {
                println!("COMPLETER pos={}", self.traces.get(tid).pos);
                self.completed_trace.push(tid);

                // find “parent” states at same origin that can produce this expr;
                let continuations_here = self.traces.get_continuations_for(self.traces.get(tid).name.clone());
                //let maybe_parent =  self.traces.get(tid).parent;

                for continue_id in continuations_here {
                    // make sure we only continue from a compatible position
                    if self.traces.get(continue_id).pos != self.traces.get(tid).origin {
                        continue;
                    }
                    println!("...continuing Task... {}", self.traces.format_task(continue_id));

                    let now_finished_via_child = self.traces.get(continue_id).dot.next_unparsed();
                    let match_rec = 
                    match now_finished_via_child {
                        Factor::Nonterm(mark, name) => MatchRec::NonTerm(name, self.traces.get(tid).pos, mark),
                        Factor::Terminal(tmark, _ch ) => MatchRec::Term('?', self.traces.get(tid).pos, tmark),
                    };
                    println!("MatchRec {:?}", &match_rec);
                    // child may have made progress; next item in parent seq needs to account for this
                    //let new_origin = self.traces.get(parent).begin;
                    let maybe_id = self.traces.task_advance_cursor(continue_id, match_rec);
                    //self.queue_front(maybe_id);
                    self.queue_back(maybe_id);
                }
                continue;
            }

            // PREDICTOR
            // task is not in a completed state. Take the next item from the list and process it
            let factor = self.traces.get(tid).dot.next_unparsed();

            match factor {
                Factor::Nonterm(mark, name) => {
                    // go one level deeper
                    println!("PREDICTOR: Nonterm {mark}{name}");

                    self.traces.save_continuation(&name, tid);


                    // We can have a Mark at the point of definiton,
                    // as well as at the point of reference...
                    // Figure out what to do with all possible combinations
                    let defn_mark = g.get_definition_mark(&name);
                    let effective_mark = match (defn_mark, mark) {
                        (Mark::Default, Mark::Default) => Mark::Default,
                        (Mark::Mute, Mark::Unmute) => Mark::Unmute,       // can 'undo' marking Mute
                        (Mark::Attr, _) | (_, Mark::Attr) => Mark::Attr,  // attributes all the way down
                        (Mark::Mute, _) | (_, Mark::Mute) => Mark::Mute,
                        (Mark::Unmute, _) | (_, Mark::Unmute) => Mark::Unmute,
                    };

                    for rule in g.get_definition(&name).iter() {
                        // TODO: propertly account for rule-level Mark
                        let dot = rule.dot_notator();
                        let new_pos = self.traces.get(tid).pos;
                        // "origin" for this downstream task now matches current pos
                        let maybe_id = self.traces.task_downstream(&name, effective_mark.clone(), new_pos, new_pos, dot);
                        //self.queue_front(maybe_id);
                        self.queue_back(maybe_id);
                    }
                }
                Factor::Terminal(tmark, matcher) => {
                    // record terminal
                    println!("SCANNER: Terminal {tmark}{matcher}");
                    if matcher.accept(input.get_tok()) {
                        // Match!
                        let rec = MatchRec::Term(input.get_tok(), input.pos() + 1, tmark);
                        println!("advance cursor SCAN");
                        let maybe_id = self.traces.task_advance_cursor(tid, rec);
                        self.queue_back(maybe_id);

                        input.next(1);

                    } else {
                        println!("non-matched char '{}' (expecting {matcher}); 🛑", input.get_tok());
                    }
                }
            }
        } // while
        self.farthest_pos = input.farthest_pos();
        println!("Finished parse with {} items in trace", self.traces.arena.len());
        //&self.trace
    }

    fn queue_front(&mut self, maybe_id: Option<TraceId>) {
        for id in maybe_id {
            self.traces.queue.push_front(id)
        }
    }

    fn  queue_back(&mut self, maybe_id: Option<TraceId>) {
        for id in maybe_id {
            self.traces.queue.push_back(id)
        }
    }

    /// Sift through and find only completed Tasks
    /// this speeds up the unpacking process by omitting parse states irrelevant to the final result
    fn find_completed_trace(&self, name: &str, origin: usize, pos: usize) -> Option<&Task> {
        // TODO: optimize
        for tid in &self.completed_trace {
            let t = self.traces.get(*tid);
            if t.name == name && t.origin == origin && t.pos == pos {
                return Some(t);
            }
        }
        None
    }

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

    pub fn unpack_parse_tree(&mut self, name: &str) -> Arena<Content> {
        println!("TRACE...");
        for tid in &self.completed_trace {
            println!("{}", self.traces.format_task(*tid));
        }
        let mut arena = Arena::new();
        let root = arena.new_node(Content::Root);
        println!("assuming ending pos of {}", self.farthest_pos);
        self.unpack_parse_tree_internal(&mut arena, name, Mark::Default, 0, self.farthest_pos, root);
        arena
    }

    fn unpack_parse_tree_internal(&self, arena: &mut Arena<Content>, name: &str, mark: Mark, origin: usize, end: usize, root: NodeId) -> () {
        let matching_trace = self.find_completed_trace(name, origin, end);
        let mut new_root = root;
            match matching_trace {
                Some(task) => {
                    let match_name = &task.name;

                    if task.mark==Mark::Mute || match_name.starts_with("-") {
                        // Skip
                        println!("trace found {mark} {task} -- SKIPPING");
                    } else {
                        // Element or Attribute
                        println!("trace found {} {task}", task.mark);
                        let name_str = match_name.to_string();
                        let data = if task.mark==Mark::Attr {
                            Content::Attribute(name_str)
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
                                let new_child = arena.new_node(Content::Text(ch.to_string()) );
                                new_root.append(new_child, arena);
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
                    println!("  No matching traces for {}@{}:{}", name, origin, end);
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



    pub fn tree_to_testfmt(arena: &Arena<Content>) -> String {
        let mut builder = Builder::default();
        let root = arena.iter().next().unwrap(); // first item == root
        let root_id = arena.get_node_id(root).unwrap();
        for child in root_id.children(arena) {
            Parser::tree_to_testfmt_recurse(arena, &mut builder, child);
        }
        builder.string().unwrap()
    }
    
    fn tree_to_testfmt_recurse(arena: &Arena<Content>, builder: &mut Builder, nid: NodeId) {
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
                for attr_child in nid.children(arena).filter(|n| arena.get(n.clone()).unwrap().get().is_attr() ) {
                    builder.append(" ");
                    let attr_desc = arena.get(attr_child).unwrap().get();
                    let attr_name = match attr_desc {
                        Content::Attribute(attr_name) => attr_name,
                        _ => unreachable!("Filter on children() somewhow didn't work..."),
                    };
                    builder.append(attr_name.to_string());
                    builder.append("=\"");

                    // aggregate descendent text nodes; escape &quot;
                    for descendant in attr_child.descendants(arena) {
                        let mut attr_builder = Builder::default();
                        match arena.get(descendant).unwrap().get() {
                            Content::Text(txt) => attr_builder.append(txt.as_str()),
                            _ => {}
                        }
                        builder.append(attr_builder.string().unwrap().replace("\"", "&quot;"));
                    }

                    builder.append("\"");
                }

                builder.append(">");
    
                for child in nid.children(arena) {
                    println!("testfmt found {child} in ::Element");
                    Parser::tree_to_testfmt_recurse(arena, builder, child);
                }
    
                builder.append("</");
                builder.append(name.to_string());
                builder.append(">");
            },
            Content::Attribute(_) => {}, // handled above
            Content::Text(utf8) => builder.append(utf8.clone()),
        }
    }

}
