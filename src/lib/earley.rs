use std::{collections::{HashMap, VecDeque}, iter, fmt};
use smol_str::SmolStr;
use string_builder::Builder;

// const POSIMARK: char = '‸'; // "\u2038"

type Token = char; // maybe u32 later?
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct ExprRef(usize);

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
}

impl Grammar {

    pub const EMPTY_EXPR: ExprRef = ExprRef(0);

    fn new() -> Self {
        // pre-seed ExprId 0 == Expr::Empty
        Self { all_exprs: vec![Expr::Empty], all_rules: HashMap::new() }
    }

    fn add_expr(&mut self, expr: Expr) -> ExprRef {
        self.all_exprs.push(expr);
        ExprRef(self.all_exprs.len() - 1)
    }

    fn add_nonterm(&mut self, name: &str) -> ExprRef {
        self.add_expr(Expr::Nonterm(SmolStr::new(name)))
    }

    fn add_litchar(&mut self, value: char) -> ExprRef {
        self.add_expr(Expr::LitChar(value))
    }

    fn add_seq(&mut self, exprs: Vec<ExprRef>) -> ExprRef {
        self.add_expr(Expr::Seq(exprs))
    }

    fn add_oneof(&mut self, exprs: Vec<ExprRef>) -> ExprRef {
        self.add_expr(Expr::OneOf(exprs))
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

    fn add_rule(&mut self, subj: &str, id: ExprRef) {
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

    /// clone a task, except the new one is at the next input position
    fn at_next_input(from: &Task) -> Task {
        Task { name: from.name.clone(), orig: from.orig, pos: from.pos + 1, expr: from.expr.clone(), len: from.len, completed: from.completed.clone() }
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
            println!("Pulled from queue {}",task);
            self.record_trace(&task);
            // TODO: check for already-cached

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
                self.pos += 1;
                intok = *tokens.get(self.pos).unwrap_or(&Parser::EOF);
                println!("Input now at position {} '{}'", self.pos, intok);
                // TODO: detect and account for newline tokens
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
                Expr::Nonterm(name) => {
                    println!("PREDICTOR (nonterm={})", name);
                    // go one level deeper to see what this nonterminal expands to
                    let nt_defn = self.grammar.get_rule_exprref(name.as_str());
                    for downexpr_id in self.grammar.get_expr_alts(nt_defn) {
                        self.record_parentage(downexpr_id, self.pos, &task);
                        self.queue.push_front(
                            Task::new(task.name.as_str(), self.pos, self.pos, downexpr_id, self.grammar.get_expr_len(downexpr_id))
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
                Expr::Empty => { unreachable!("Don't queue empty exprs") }
                Expr::OneOf(_) => { unreachable!("Alternates should't get queued like this")}
            }
        }
        println!("Finished parse with {} items in trace", self.trace.len());
        &self.trace
    }

    pub fn unpack_parse_tree(&self, name: &str) -> String {
        println!("TRACE...");
        for task in &self.trace {
            println!("{}", task);
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
                builder.append("<");
                builder.append(elem_name.to_string());
                builder.append(">");

                // CHILDREN
                let mut new_start = start;
                let positions = &task.completed;
                let exprs = grammar.get_expr_seq(task.expr);
                println!("trace children {}, positions {:?}, exprs {:?}", &task, positions, exprs);
                for i in 0..exprs.len() {
                    let newpos = positions[i];
                    let newexpr = exprs[i];
                    if grammar.is_terminal(newexpr) {
                        if let Expr::LitChar(ch) = grammar.get_expr(newexpr) {
                            builder.append(ch);
                        }
                    } else {
                        if let Expr::Nonterm(named) = grammar.get_expr(newexpr) {
                            self.unpack_parse_tree_internal(builder, named.as_str(), new_start, newpos);
                        }
                    }
                    new_start = newpos;
                }

                builder.append("</");
                builder.append(elem_name.to_string());
                builder.append(">");
    
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

    //fn trace_key(&self, task: &Task) -> String {
    //    format!("{}[{},{}]", task.name, task.orig, task.pos)
    //}

    /// record a trace
    /// returns true if this trace had been previously recorded
    fn record_trace(&mut self, task: &Task) -> bool {
        if task.pos > self.farthest_pos {
            self.farthest_pos = task.pos;
        }
        self.trace.push(task.clone());
        false
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

// temporary hack to get hands-onß

pub fn get_simple1_grammer() -> Grammar {
    // doc = "a", "b"
    let mut grammar = Grammar::new();
    let a = grammar.add_litchar('a');
    let b = grammar.add_litchar('b');
    let seq = grammar.add_seq(vec![a,b]);
    grammar.add_rule("doc", seq);
    grammar
}

pub fn get_simple2_grammer() -> Grammar {
    // doc = "a" | "b"
    let mut grammar = Grammar::new();
    let a = grammar.add_litchar('a');
    let b = grammar.add_litchar('b');
    let choice = grammar.add_oneof(vec![a,b]);
    grammar.add_rule("doc", choice);
    grammar
}

pub fn get_simple3_grammer() -> Grammar {
    // doc = a, b
    // a = "a" | "A"
    // b = "b" | "B"
    let mut grammar = Grammar::new();
    let aref = grammar.add_nonterm("a");
    let bref = grammar.add_nonterm("b");
    let a = grammar.add_litchar('a');
    let a_ = grammar.add_litchar('A');
    let a_choice = grammar.add_oneof(vec![a,a_]);
    let b = grammar.add_litchar('b');
    let b_ = grammar.add_litchar('B');
    let b_choice = grammar.add_oneof(vec![b, b_]);
    let seq = grammar.add_seq(vec![aref, bref]);
    grammar.add_rule("doc", seq);
    grammar.add_rule("a", a_choice);
    grammar.add_rule("b", b_choice);
    grammar
}

pub fn get_wiki_grammar() -> Grammar {
    // <P> ::= <S>
    // <S> ::= <S> "+" <M> | <M>
    // <M> ::= <M> "*" <T> | <T>
    // <T> ::= "1" | "2" | "3" | "4"
    let mut grammar = Grammar::new();
    let s_expr = grammar.add_nonterm("S");
    grammar.add_rule("P", s_expr);

    let m_expr = grammar.add_nonterm("M");
    let t_expr = grammar.add_nonterm("T");
    let plus = grammar.add_litchar('+');
    let star = grammar.add_litchar('*');
    let seq1 = grammar.add_seq(vec![s_expr, plus, m_expr]);
    let alt1 = grammar.add_oneof(vec![seq1, m_expr]);
    let seq2 = grammar.add_seq(vec![m_expr, star, t_expr]);
    let alt2 = grammar.add_oneof(vec![seq2, t_expr]);
    let digit = grammar.add_expr(Expr::LitCharOneOf(SmolStr::new("1234")));

    grammar.add_rule("S", alt1);
    grammar.add_rule("M", alt2);
    grammar.add_rule("T", digit);
    grammar
}
