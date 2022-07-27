use std::{collections::{HashMap, VecDeque, HashSet}, iter, fmt};
use smol_str::SmolStr;
use string_builder::Builder;


// const POSIMARK: char = '‸'; //&& "\u2038"

type Token = char; // maybe u32 later?
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct ExprRef(usize);

impl ExprRef {
    /// not directly using Display trait due to the dependency on &Grammar
    pub fn pretty_print(&self, grammar: &Grammar) -> String {
        let expr = grammar.get_expr(ExprRef(self.0));
        match expr {
            Expr::Empty => format!("Empty"),
            Expr::LitChar(c) => format!("\"{}\"", c),
            Expr::LitCharOneOf(s) => format!("[ \"{}\" ]", s),
            Expr::Nonterm(n) => format!("Nonterm {}", n),
            Expr::Seq(vec) => {
                let lst = &vec.iter()
                    .map(|x: &ExprRef| x.pretty_print(grammar))
                    .collect::<Vec<String>>()
                    .join(", ");
                format!("Seq [ {} ]", lst)
            }
            Expr::OneOf(vec) => {
                let lst = &vec.iter()
                    .map(|x: &ExprRef| x.pretty_print(grammar))
                    .collect::<Vec<String>>()
                    .join(" | ");
                format!("OneOf ( {} )", lst)
            }
        }
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
        let s = format!("--{}{}", self.next_generated_rule, hint);
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
    pub fn add_repeat0(&mut self, f: ExprRef) -> ExprRef {
        let genrule = self.internal_id("repeat0");
        let f_star = self.add_nonterm(genrule.as_str());
        let seq = self.add_oneof(vec![f, f_star]);
        let opt = self.add_optional(seq);
        self.add_rule(genrule.as_str(), opt);
        f_star
    }
        
    /// One or more repetitions:
    /// f+ ⇒ f-plus
    /// -f-plus: f, f*.
    pub fn add_repeat1(&mut self, f: ExprRef) -> ExprRef {
        let f_star = self.add_repeat0(f);
        self.add_seq(vec![f, f_star])
    }

    /// One or more repetitions with separator:
    /// f++sep ⇒ f-plus-sep
    /// -f-plus-sep: f, (sep, f)*.
    pub fn add_repeat1_sep(&mut self, f: ExprRef, sep: ExprRef) -> ExprRef {
        let inner_seq = self.add_seq(vec![sep, f]);
        let inner_seq_star = self.add_repeat0(inner_seq);
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
            Expr::Seq(vec) => vec,
            _ => vec![id]
        }
    }

    fn get_expr_len(&self, id: ExprRef) -> usize {
        match self.get_expr(id) {
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

    fn get_rule(&self, subj: &str) -> (SmolStr, ExprRef) {
        let rule = &self.all_rules[subj];
        (rule.subj.clone(), rule.expr)
    }

    fn get_rule_exprref(&self, subj: &str) -> ExprRef {
        self.all_rules[subj].expr
    }

    fn get_rule_expr(&self, subj: &str) -> Expr {
        self.get_expr(self.get_rule_exprref(subj))
    }

    fn get_rule_expr_alts(&self, subj: &str) -> Vec<ExprRef> {
        self.get_expr_alts(self.get_rule_exprref(subj))
    }

}

/// Name, origin, position, expr_entire, expr_len, expr_cursor
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Task {
    name: SmolStr,        // rule name
    orig: usize,          // starting position in the input
    pos: usize,           // current position in the input
    expr: ExprRef,        // expression
    len: usize,           // numer of items in the expr
    
   /// completed.len() is the cursor position within the expr
   /// completed[n] is the starting position in the input, of the nth piece
    completed: Vec<usize>,   
                          
}
//pub struct Task(SmolStr, usize, usize, ExprRef, usize, usize);

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Task({} {}:{} {:?} len={} cursor={})",
            self.name, self.orig, self.pos, self.expr, self.len, self.completed.len())
    }
}

impl Task {
    fn new(name: &str, orig: usize, pos: usize, expr: ExprRef, len: usize) -> Task {
        Task{ name: SmolStr::new(name), orig, pos, expr, len, completed: Vec::new()}
    }

    /// clone a task, except advancing the cursor (storing given position data for the piece just advanced-over)
    fn at_new_cursor(from: &Task, subseq_pos: usize) -> Task {
        let mut new_vec = from.completed.clone();
        new_vec.push(subseq_pos);
        Task { name: from.name.clone(), orig: from.orig, pos: from.pos + 1, expr: from.expr.clone(), len: from.len, completed: new_vec }
    }

}

#[derive(Debug)]
pub struct Parser {
    grammar: Grammar,
    queue: VecDeque<Task>,
    trace: Vec<Task>,
    parentage: HashMap<String, Vec<Task>>, // cache key is "exprid@origin"
    pos: usize, // input position
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
            parentage: HashMap::new(),
            pos: 0,
            farthest_pos: 0,
            trace_hashes: HashSet::new(),

        }
    }

    pub fn parse(&mut self, input: &str, start: &str) -> &Vec<Task> {
        let tokens = input.chars().chain(iter::once(Parser::EOF))
            .map(|x| x as Token).collect::<Vec<_>>();

        let mut intok = tokens[0];
        self.pos = 0;
        println!("Input now at position {} '{}'", self.pos, intok);
        //self.token_line = 1;
        //self.token_col = 1;

        // Seed with starting expr
        for expr_id in self.grammar.get_rule_expr_alts(start) {
            self.queue.push_front(
                Task::new(start, 0, 0, expr_id, self.grammar.get_expr_len(expr_id))
            );
        }
        
        // work through the queue
        while let Some(task) = self.queue.pop_front() {
            println!("Pulled from queue {} {}",task, task.expr.pretty_print(&self.grammar));
            if self.record_trace(&task) {
                // already cached...
                continue;
            }

            let expr_cursor = task.completed.len();
            let expr = self.grammar.get_expr(task.expr);

            // task in completed state? 
            if expr_cursor == task.len {
                println!("COMPLETER");
                // find “parent” states at same origin that can produce this expr;
                let parents = self.get_parentage(task.expr, task.orig);
                // queue parent at next position
                for parent in parents {
                        self.queue.push_front(
                            Task::at_new_cursor(&parent, parent.pos)
                    );
                }

                // advance through input
                if self.grammar.is_terminal(task.expr) {
                    self.pos += 1;
                    intok = *tokens.get(self.pos).unwrap_or(&Parser::EOF);
                    println!("Input advanced to position {} '{}'", self.pos, intok);
                    // TODO: detect and account for newline tokens
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
                            Task::new(task.name.as_str(), self.pos, self.pos, downexpr_id, self.grammar.get_expr_len(downexpr_id))
                        );
                    }
                }
                Expr::OneOf(vec) => { 
                    println!("PREDICTOR OneOf {}", task.expr.pretty_print(&self.grammar));
                    // for each alt, queue it up
                    for downexpr_id in vec {
                        self.record_parentage(downexpr_id, self.pos, &task);
                        self.queue.push_front(
                            Task::new(task.name.as_str(), task.orig, task.pos, downexpr_id, self.grammar.get_expr_len(downexpr_id))
                        );
                    }
                }
                Expr::Nonterm(name) => {
                    println!("PREDICTOR (nonterm={})", name);
                    // go one level deeper to see what this nonterminal expands to
                    let nt_defn = self.grammar.get_rule_exprref(name.as_str());
                    for downexpr_id in self.grammar.get_expr_alts(nt_defn) {
                        self.record_parentage(downexpr_id, self.pos, &task);
                        self.queue.push_front(
                            Task::new(name.as_str(), self.pos, self.pos, downexpr_id, self.grammar.get_expr_len(downexpr_id))
                        );
                    }
                }
                Expr::LitChar(ch) => {
                    println!("SCANNER (char='{}')", ch);
                    if ch == intok {
                        // hit!
                        // CONTINUE task AT (pos incremented (input, sym))
                        self.queue.push_back(
                            Task::at_new_cursor(&task, task.pos)
                            //Task(task_subj.clone(), expr_origin, expr_pos, expr_entire, expr_len, expr_cursor + 1)
                        );
                    } else {
                        println!("non-matched char '{}' (expecting '{}'); terminating task", intok, ch);
                    }
                }
                Expr::LitCharOneOf(str) => {
                    println!("SCANNER (char one of '{}')", str);
                    if str.contains(intok) {
                        self.queue.push_back(
                            Task::at_new_cursor(&task, task.pos)
                        );
                    } else {
                        println!("non-matched char '{}' (expecting one of '{}'); terminating task", intok, str);
                    }
                }
                Expr::Empty => { 
                    println!("SCANNER Empty");
                    self.queue.push_back(
                        Task::at_new_cursor(&task, task.pos)
                    );
                 }
            }
        }
        println!("Finished parse with {} items in trace", self.trace.len());
        &self.trace
    }

    pub fn unpack_parse_tree(&self, name: &str) -> String {
        println!("TRACE...");
        for task in &self.trace {
            println!("{} {}", task, task.expr.pretty_print(&self.grammar));
        }
        let mut builder = Builder::default();
        println!("assuming ending pos of {}", self.farthest_pos);
        self.unpack_parse_tree_internal(&mut builder, name, 0, self.farthest_pos);
        builder.string().unwrap_or("Internal error generating parse".to_string())
    }

    fn unpack_parse_tree_internal(&self, builder: &mut Builder, name: &str, start: usize, end: usize) -> () {
        let grammar = &self.grammar;
        let elem = &self.trace.iter().find(
            |t|t.name==name && t.orig==start && t.pos==end && t.len==t.completed.len());

        match elem {
            Some(task) => {
                let elem_unwrap = elem.unwrap();
                let elem_name = &elem_unwrap.name;
                println!("trace found {}", elem_unwrap);
                if !elem_name.starts_with("-") {
                    builder.append("<");
                    builder.append(elem_name.to_string());
                    builder.append(">");
                }

                // CHILDREN
                let mut new_start = start;
                let positions = &task.completed;
                let exprs = grammar.get_expr_seq(task.expr);
                println!("trace children {}, positions {:?}, exprs {:?}", &task, positions, exprs);
                for i in 0..exprs.len() {
                    let newpos = positions[i] + 1;  // + length of this terminal
                    let newexpr = exprs[i];
                    if grammar.is_terminal(newexpr) {
                        if let Expr::LitChar(ch) = grammar.get_expr(newexpr) {
                            builder.append(ch);
                        }
                    } else {
                        if let Expr::Nonterm(named) = grammar.get_expr(newexpr) {
                            println!("HEREHEREHERE {} {} {}", named, new_start, newpos);
                            self.unpack_parse_tree_internal(builder, named.as_str(), new_start, newpos);
                        }
                    }
                    new_start = newpos;
                }

                if !elem_name.starts_with("-") {
                    builder.append("</");
                    builder.append(elem_name.to_string());
                    builder.append(">");
                }
    
            }
            None => {}
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
        println!("computed hash {}", hash);
        if self.trace_hashes.contains(&hash) {
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

