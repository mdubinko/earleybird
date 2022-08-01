use std::{collections::{HashMap, VecDeque, HashSet}, fmt};
use smol_str::SmolStr;
use string_builder::Builder;


// const POSIMARK: char = '‸'; //&& "\u2038"

type Token = char; // maybe u32 later?
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct ExprRef(usize);

impl ExprRef {
    /// not directly using Display trait due to the dependency on &Grammar
    pub fn print_dot_notation(&self, g: &Grammar, cursor: Option<usize>) -> String {
        let dot = "•";

        // the following only apply to single-item exprs
        let at_start = if let Some(0) = cursor { dot } else { "" };
        let at_end = if let Some(1) = cursor { dot } else { "" };

        let expr = g.get_expr(ExprRef(self.0));
        match expr {
            Expr::Empty => String::from("ε"),
            Expr::LitChar(c) => format!("{}\"{}\"{}", at_start, c, at_end),
            Expr::LitCharOneOf(s) => format!("{}[ \"{}\" ]{}", at_start, s, at_end),
            Expr::Nonterm(n) => format!("{} {} {}", at_start, n, at_end),
            Expr::Seq(vec) => {
                match cursor {
                    Some(pos) => {
                        let (pre, post) = vec.split_at(pos);
                        let pre_lst = self.print_vec(pre, &g);
                        let post_lst = self.print_vec(post, &g);
                        format!("[ {} {} {} ]", pre_lst, dot, post_lst)
                    }
                    None => {
                        let lst = self.print_vec(&vec, &g);
                        format!("[ {} ]", lst)
                    }
                }
            }
            Expr::OneOf(vec) => {
                let lst = &vec.iter()
                    .map(|x: &ExprRef| x.print_dot_notation(g, None))
                    .collect::<Vec<String>>()
                    .join(" | ");
                format!("{} ( {} ) {}", at_start, lst, at_end)
            }
        }
    }

    fn print_vec(&self, vec: &[ExprRef], g: &Grammar) -> String {
        let s = &vec.iter()
        .map(|x: &ExprRef| x.print_dot_notation(g, None))
        .collect::<Vec<String>>()
        .join(", ");
        s.clone()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Expr {
    Empty,
    LitChar(char),
    LitCharOneOf(SmolStr),
    //LitCharRange(char, char),
    //LitCharUnicodeCat(UnicodeRange),
    //LitStr(SmolStr),
    Nonterm(SmolStr),
    Seq(Vec<ExprRef>),
    OneOf(Vec<ExprRef>),
    //Unmatchable,
}

#[derive(Debug, Clone)]
pub enum RuleMark {
    Attr,
    Quiet,
}

#[derive(Debug, Clone)]
pub struct Rule {
    subj: SmolStr,
    expr: ExprRef,
    mark: Option<RuleMark>,
}

/// the primary owner of all grammar data structures
#[derive(Debug)]
pub struct Grammar {
    all_exprs: Vec<Expr>,
    all_rules: HashMap<SmolStr, Rule>,
    next_generated_rule: i32,
}

impl Grammar {

    pub const EMPTY_EXPR: ExprRef = ExprRef(0);

    pub fn new() -> Self {
        // pre-seed ExprId 0 == Expr::Empty
        Self { all_exprs: vec![Expr::Empty], all_rules: HashMap::new(), next_generated_rule: 0 }
    }

    fn internal_id(&mut self, hint: &str) -> String {
        let s = format!("--{}{}", hint, self.next_generated_rule);
        self.next_generated_rule += 1;
        s
    }

    fn add_expr(&mut self, expr: Expr) -> ExprRef {
        self.all_exprs.push(expr);
        ExprRef(self.all_exprs.len() - 1)
    }

    pub fn add_nonterm(&mut self, name: &str) -> ExprRef {
        self.add_expr(Expr::Nonterm(SmolStr::new(name)))
    }

    pub fn add_litchar(&mut self, value: char) -> ExprRef {
        self.add_expr(Expr::LitChar(value))
    }

    pub fn add_litcharoneof(&mut self, list: &str) -> ExprRef {
        self.add_expr(Expr::LitCharOneOf(SmolStr::new(list)))
    }

    pub fn add_seq(&mut self, exprs: Vec<ExprRef>) -> ExprRef {
        self.add_expr(Expr::Seq(exprs))
    }

    pub fn add_oneof(&mut self, exprs: Vec<ExprRef>) -> ExprRef {
        self.add_expr(Expr::OneOf(exprs))
    }

    /// Optional factor:
    /// f? ⇒ f-option
    /// -f-option: f; ().
    pub fn add_optional(&mut self, f: ExprRef) -> ExprRef {
        self.add_oneof(vec![f, Grammar::EMPTY_EXPR])
    }

    /// Zero or more repetitions:
    /// f* ⇒ f-star
    /// -f-star: (f, f-star)?.
    fn add_repeat0_internal(&mut self, f: ExprRef, hint: &str) -> ExprRef {
        let genrule = self.internal_id(hint);
        let f_star = self.add_nonterm(genrule.as_str());
        let seq = self.add_seq(vec![f, f_star]);
        let opt = self.add_optional(seq);
        self.add_rule(genrule.as_str(), opt);
        f_star
    }

    pub fn add_repeat0(&mut self, f: ExprRef) -> ExprRef {
        self.add_repeat0_internal(f, "f-star")
    }
        
    /// One or more repetitions:
    /// f+ ⇒ f-plus
    /// -f-plus: f, f*.
    pub fn add_repeat1(&mut self, f: ExprRef) -> ExprRef {
        let f_star = self.add_repeat0_internal(f, "f-plus");
        self.add_seq(vec![f, f_star])
    }

    /// One or more repetitions with separator:
    /// f++sep ⇒ f-plus-sep
    /// -f-plus-sep: f, (sep, f)*.
    pub fn add_repeat1_sep(&mut self, f: ExprRef, sep: ExprRef) -> ExprRef {
        let inner_seq = self.add_seq(vec![sep, f]);
        let inner_seq_star = self.add_repeat0_internal(inner_seq, "f++sep");
        self.add_seq(vec![f, inner_seq_star])
    }

    /// Zero or more repetitions with separator:
    /// f**sep ⇒ f-star-sep
    /// -f-star-sep: (f++sep)?.
    pub fn add_repeat0_sep(&mut self, f: ExprRef, sep: ExprRef) -> ExprRef {
        let f_plusplus = self.add_repeat1_sep(f, sep);
        self.add_optional(f_plusplus)
    }

    fn get_expr(&self, id: ExprRef) -> Expr {
        let ExprRef(idx) = id; // Destructuring
        self.all_exprs[idx].clone()
    }

    fn is_terminal(&self, id: ExprRef) -> bool {
        match self.get_expr(id) {
            Expr::LitChar(_) | Expr::LitCharOneOf(_) => true,
            _ => false
        }
    }

    /// gets an expr, but split out alts, as needed in the parser use case
    fn get_expr_alts(&self, id: ExprRef) -> Vec<ExprRef> {
        match self.get_expr(id) {
            Expr::OneOf(exprs) => exprs,       
            _ => vec![id]
        }
    }

    /// gets an expression in iterable form
    fn get_expr_seq(&self, id: ExprRef) -> Vec<ExprRef> {
        match self.get_expr(id) {
            Expr::Empty => Vec::new(),
            Expr::Seq(vec) => vec,
            _ => vec![id]
        }
    }

    fn get_expr_len(&self, id: ExprRef) -> usize {
        match self.get_expr(id) {
            Expr::Empty => 0,
            Expr::Seq(v) => v.len(),
            _ => 1
        }
    }

    fn add_rule_internal(&mut self, rule: Rule) {
        self.all_rules.insert(rule.subj.clone(), rule);
    }

    pub fn add_rule(&mut self, subj: &str, id: ExprRef) {
        self.add_rule_internal(Rule {subj: SmolStr::new(subj), expr: id, mark: None});
    }

    fn get_rule(&self, subj: &str) -> ExprRef {
        self.all_rules[subj].expr
    }

    fn get_rule_expr_alts(&self, subj: &str) -> Vec<ExprRef> {
        self.get_expr_alts(self.get_rule(subj))
    }

}


#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ParsedItem {
    Empty(usize),                 // successfully "matched" an empty rule
    Terminal(char, usize),        // matched this character @ starting position FOR FOLLOWING ITEMS
    NonTerminal(SmolStr, usize),  // matched this nonterminal @ starting position FOR FOLLOWING ITEMS
}

impl fmt::Display for ParsedItem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParsedItem::Empty(pos) => {
                write!(f, "()@{}", pos)
            }
            ParsedItem::Terminal(ch, pos) => {
                write!(f, "'{}'to{}", ch, pos)
            }
            ParsedItem::NonTerminal(str, pos) => {
                write!(f, "{}to{}", str, pos)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Task {
    name: SmolStr,        // rule name
    begin: usize,         // starting position in the input
    pos: usize,           // current position in the input
    expr: ExprRef,        // expression
    len: usize,           // numer of items in the expr
    
   /// consumed.len() is the cursor position within the expr
   /// consumed[n] is ParsedItem where ch is the actual character(s) consumed
   /// and pos isthe starting position in the input
   /// , of the nth piece
    consumed: Vec<ParsedItem>,
                          
}
//pub struct Task(SmolStr, usize, usize, ExprRef, usize, usize);

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let consumed: String = self.consumed.iter()
            .map(|x| format!("{}", x))
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "Task({} {}:{} {:?} len={} cursor={} {})",
            self.name, self.begin, self.pos, self.expr, self.len, self.consumed.len(), consumed)
    }
}

impl Task {
    fn new(name: &str, orig: usize, pos: usize, expr: ExprRef, len: usize) -> Task {
        Task{ name: SmolStr::new(name), begin: orig, pos, expr, len, consumed: Vec::new()}
    }

    /// clone a task, except advancing the cursor (storing given position data for the piece just advanced-over)
    fn advance_cursor(from: &Task, item: ParsedItem, new_pos: usize) -> Task {
        let mut new_vec = from.consumed.clone();
        new_vec.push(item);
        Task { name: from.name.clone(), begin: from.begin, pos: new_pos, expr: from.expr.clone(), len: from.len, consumed: new_vec }
    }
}

#[derive(Debug)]
pub struct Parser {
    grammar: Grammar,
    queue: VecDeque<Task>,
    trace: Vec<Task>,
    completed_trace: Vec<Task>,
    parentage: HashMap<String, Vec<Task>>, // cache key is "exprid@origin"
    pos: usize,    // input position
    pos_eof: bool, // input reached end #TODO
    farthest_pos: usize,
    //token_line: u16,
    //token_col: u16,
    trace_hashes: HashSet<String>,
}

/// Earley parser
impl Parser {
    const EOF: char = '\x1f';

    pub fn new(grammar: Grammar) -> Self {
        Self {
            grammar,
            queue: VecDeque::new(),
            trace: Vec::new(),
            completed_trace: Vec::new(),
            parentage: HashMap::new(),
            pos: 0,
            pos_eof: false,
            farthest_pos: 0,
            trace_hashes: HashSet::new(),

        }
    }

    pub fn parse(&mut self, input: &str, top_rule: &str) -> &Vec<Task> {
        let tokens = input.chars()
            .map(|x| x as Token).collect::<Vec<_>>();

        self.pos = 0;
        let mut intok = Parser::EOF;
        if !tokens.is_empty() {
            intok = tokens[self.pos];
        }
        println!("Input now at position {} '{}'", self.pos, intok);
        //self.token_line = 1;
        //self.token_col = 1;

        // Seed with top expr
        for expr_id in self.grammar.get_rule_expr_alts(top_rule) {
            self.queue.push_front(
                Task::new(top_rule, 0, 0, expr_id, self.grammar.get_expr_len(expr_id))
            );
        }
        
        // work through the queue
        while let Some(task) = self.queue.pop_front() {
            println!("Pulled from queue {} {}",task, task.expr.print_dot_notation(&self.grammar, Some(task.consumed.len())));
            if self.record_trace(&task) {
                // already cached...
                continue;
            }

            let expr_cursor = task.consumed.len();
            let expr = self.grammar.get_expr(task.expr);

            // task in completed state? 
            if expr_cursor == task.len {
                println!("COMPLETER");
                // find “parent” states at same origin that can produce this expr;
                let parents = self.get_parentage(task.expr, task.begin);

                // queue parent at next position
                // eg.
                // if previously this expr got processed
                // Source: ( • a, "b" )
                // causing the nonterminal a to get processed. Eventually it completes
                // First item in seq done: a •
                // right here ^^^ is where we are at.
                // Need to retrieve the parent rule "Source", but advanced to the next cursor
                // (and holding the input position that came from the downstream parsing)
                // Goal: ( a, • 'b" )
                for parent in parents {
                    println!("parent looks like... {}", parent);

                    match &expr {
                        Expr::Empty => {
                            println!("completing Empty");
                            self.queue.push_front(
                                Task::advance_cursor(&parent, ParsedItem::Empty(parent.pos), task.pos)
                            );
                        }
                        Expr::LitChar(ch) => {
                            println!("completing LitChar {}", ch);
                            self.queue.push_front(
                                Task::advance_cursor(&parent, ParsedItem::Terminal(*ch, parent.pos + 1), task.pos)
                            );
                        }
                        Expr::LitCharOneOf(str) => {
                            println!("completing LitCharOneOf");
                            self.queue.push_front(
                                Task::advance_cursor(&parent, ParsedItem::Terminal('?', parent.pos + 1), task.pos)
                            );
                        }
                        Expr::Nonterm(name) => {
                            println!("completing Nonterm {}", name);
                            self.queue.push_front(
                                Task::advance_cursor(&parent, ParsedItem::NonTerminal(name.clone(), task.pos), task.pos)
                            );
                        }
                        Expr::Seq(vec) => {
                            println!("completing seq vec.len={}", vec.len());
                            self.queue.push_front(
                                Task::advance_cursor(&parent, ParsedItem::NonTerminal(parent.name.clone(), task.pos), task.pos)
                            )
                        }
                        Expr::OneOf(vec) => {
                            println!("completing OneOf vec.len={}", vec.len());
                            unimplemented!("(Not) completing OneOf");
                        }
                    }




                    //self.queue.push_front(
                    //    Task::advance_cursor(&parent, ParsedItem::Terminal(intok, parent.pos), task.pos)
                    //    //Task::advance_cursor(&parent, '💩', parent.pos, task.pos)
                    //);
                }
                // advance input if needed
                if self.grammar.is_terminal(task.expr) {
                    if tokens.len() > self.pos + 1 {
                        self.pos += 1;
                        intok = tokens[self.pos];
                        println!("Input advanced to position {} '{}'", self.pos, intok);
                        // TODO: detect and account for newline tokens    
                    } else {
                        // Need to be able to advance one-past-the-end
                        println!("*** End of input *** EOF.");
                        self.pos += 1;
                        intok = Parser::EOF;
                    }
                }
                continue;
            }

            match expr {
                Expr::Seq(child_ids) => {
                    println!("PREDICTOR (seq@{}) @{}", expr_cursor, &self.pos);
                    // advance through sequence; queue item at cursor pos
                    let next = child_ids[expr_cursor];
                    for downexpr_id in self.grammar.get_expr_alts(next) {
                        self.record_parentage(downexpr_id, self.pos, &task);
                        self.queue.push_front(
                            // TODO !!!!!!!! orig: self.pos vs task.pos
                            Task::new(task.name.as_str(), task.pos, task.pos, downexpr_id, self.grammar.get_expr_len(downexpr_id))
                        );
                    }
                }
                Expr::OneOf(vec) => { 
                    println!("PREDICTOR OneOf {}", task.expr.print_dot_notation(&self.grammar, Some(task.consumed.len())));
                    // for each alt, queue it up
                    for downexpr_id in vec {
                        self.record_parentage(downexpr_id, self.pos, &task);
                        self.queue.push_front(
                            Task::new(task.name.as_str(), task.begin, task.pos, downexpr_id, self.grammar.get_expr_len(downexpr_id))
                        );
                    }
                }
                Expr::Nonterm(name) => {
                    println!("PREDICTOR (nonterm={})", name);
                    // go one level deeper to see what this nonterminal expands to
                    let nt_defn = self.grammar.get_rule(name.as_str());
                    for downexpr_id in self.grammar.get_expr_alts(nt_defn) {
                        self.record_parentage(downexpr_id, self.pos, &task);
                        self.queue.push_front(
                            // TODO !!!!!!!! orig: self.pos vs task.pos
                            Task::new(name.as_str(), task.pos, task.pos, downexpr_id, self.grammar.get_expr_len(downexpr_id))
                        );
                    }
                }
                Expr::LitChar(ch) => {
                    println!("SCANNER (char='{}')", ch);
                    if ch == intok {
                        // hit!
                        // CONTINUE task AT (pos incremented (input, sym))
                        self.queue.push_back(
                            Task::advance_cursor(&task, ParsedItem::Terminal(ch, task.pos + 1), task.pos + 1)
                        );
                    } else {
                        println!("non-matched char '{}' (expecting '{}'); terminating task", intok, ch);
                    }
                }
                Expr::LitCharOneOf(str) => {
                    println!("SCANNER (char one of '{}')", str);
                    if str.contains(intok) {
                        self.queue.push_back(
                            Task::advance_cursor(&task, ParsedItem::Terminal(intok, task.pos + 1), task.pos + 1)
                        );
                    } else {
                        println!("non-matched char '{}' (expecting one of '{}'); terminating task", intok, str);
                    }
                }
                Expr::Empty => { 
                    println!("SCANNER Empty");
                    unimplemented!("How did we get here?");
                    // Empty should never occur in a sequence.
                    //self.queue.push_back(
                    //    Task::advance_cursor(&task, task.pos, task.pos)
                    //);
                 }
            }
        }
        println!("Finished parse with {} items in trace", self.trace.len());
        &self.trace
    }

    pub fn unpack_parse_tree(&mut self, name: &str) -> String {
        // from this point, all we care about are completed traces
        self.completed_trace.clear();
        for item in &self.trace {
            if item.len==item.consumed.len() {
                self.completed_trace.push(item.clone());
            }
        }

        //self.completed_trace.extend(
        //    self.trace
        //        .iter()
        //        .filter(|t|*t.len==*t.consumed.len())
        //);

        println!("TRACE...");
        for task in &self.completed_trace {
            println!("{}    {}", task, task.expr.print_dot_notation(&self.grammar, Some(task.consumed.len())));
        }
        let mut builder = Builder::default();
        println!("assuming ending pos of {}", self.farthest_pos);
        self.unpack_parse_tree_internal(&mut builder, name, 0, self.farthest_pos);
        builder.string().unwrap_or("Internal error generating parse".to_string())
    }

    fn unpack_parse_tree_internal(&self, builder: &mut Builder, name: &str, begin: usize, end: usize) -> () {
        let g = &self.grammar;
        let matching_trace = &self.completed_trace.iter().find(
            |t|t.name==name && t.begin==begin && t.pos==end);

            match matching_trace {
                Some(task) => {
                    let match_name = &task.name;
                    println!("trace found {}", task);
                    if !match_name.starts_with("-") {
                        builder.append("<");
                        builder.append(match_name.to_string());
                        builder.append(">");
                    }
            
                    // CHILDREN
                    let mut new_begin = begin;
                    let consumed = &task.consumed;
                    for item in consumed {
                        match item {
                            ParsedItem::Empty(pos) => {
                                println!("Empty @ {}", pos);
                                new_begin = *pos;
                            }
                            ParsedItem::Terminal(chr, pos) => {
                                println!("Terminal {}@{}", chr, *pos);
                                builder.append(chr.to_string());
                                new_begin = *pos;
                            }
                            ParsedItem::NonTerminal(ntname, pos) => {
                                println!("Nonterminal {}@{}", ntname, pos);
                                // insure no infinite recurse - don't call self with same params
                                assert!( !(ntname==name && new_begin==begin && *pos==end));
                                self.unpack_parse_tree_internal(builder, ntname.as_str(), new_begin, *pos);
                                new_begin = *pos;
                            }
                        }
                    }
            
                    if !match_name.starts_with("-") {
                        builder.append("</");
                        builder.append(match_name.to_string());
                        builder.append(">");
                    }
            
                }
                None => {
                    println!("  No matching traces for {}@{}:{}", name, begin, end);
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

    /// record a trace
    /// returns true if this trace had been previously recorded
    fn record_trace(&mut self, task: &Task) -> bool {
        if task.pos > self.farthest_pos {
            self.farthest_pos = task.pos;
        }
        let hash = task.to_string();
        if self.trace_hashes.contains(&hash) {
            println!("...Skipping this task -- previously seen");
            true
        } else {
            self.trace_hashes.insert(hash);
            self.trace.push(task.clone());
            false
        }
    }

    fn parentage_key(&self, expr: ExprRef, pos: usize) -> String {
        format!("{:?}@{}", expr, pos)
    }

    /// HOWTO get back to a rule's parent:
    /// For an Expr at pos, it was spawned by a certain Task
    /// 
    fn record_parentage(&mut self, expr: ExprRef, pos: usize, task: &Task) {
        let key = self.parentage_key(expr, pos);
        let vec = self.parentage.entry(key).or_insert(Vec::new());
        vec.push(task.clone());
    }

    /// given a exprref and position, find all the rules that could've produced it
    fn get_parentage(&self, expr: ExprRef, pos: usize) -> Vec<Task> {
        let val = self.parentage.get(&self.parentage_key(expr, pos));
        val.map(|x| (*x).clone()).unwrap_or(Vec::new())
    }

}

