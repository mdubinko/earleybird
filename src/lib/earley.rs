use std::collections::{HashMap, VecDeque};
use smol_str::SmolStr;

// const POSIMARK: char = '‸'; // "\u2038"

type Token = char; // maybe u32 later?
type RuleRef = usize;

#[derive(Clone, Debug)]
pub enum Rule {
    Empty,
    LitChar(char),
    LitCharOneOf(SmolStr),
    //LitCharRange(char, char),
    //LitCharUnicodeCat(UnicodeRange),
    //LitStr(SmolStr),
    Nonterm(SmolStr),
    Seq(Vec<RuleRef>),
    OneOf(Vec<RuleRef>),
    //Unmatchable,
}

#[derive(Debug, Clone)]
pub enum SentenceMark {
    Attr,
    Quiet,
}

#[derive(Debug, Clone)]
pub struct Sentence {
    subj: SmolStr,
    rule: RuleRef,
    mark: Option<SentenceMark>,
}

/// the primary owner of all grammar data structures
#[derive(Debug)]
pub struct Grammar {
    all_rules: Vec<Rule>,
    all_sentences: HashMap<SmolStr, Sentence>,
}

impl Grammar {

    const EMPTY_RULE: RuleRef = 0;

    fn new() -> Self {
        // rule 0 is always Rule::Empty
        Self { all_rules: vec![Rule::Empty], all_sentences: HashMap::new() }
    }

    fn add_rule(&mut self, rule: Rule) -> RuleRef {
        self.all_rules.push(rule);
        self.all_rules.len() - 1
    }

    fn add_nonterminal(&mut self, name: &str) -> RuleRef {
        self.add_rule(Rule::Nonterm(SmolStr::new(name)))
    }

    fn add_literal(&mut self, value: char) -> RuleRef {
        self.add_rule(Rule::LitChar(value))
    }

    fn add_sequence(&mut self, rules: Vec<RuleRef>) -> RuleRef {
        self.add_rule(Rule::Seq(rules))
    }

    fn add_alternation(&mut self, rules: Vec<RuleRef>) -> RuleRef {
        self.add_rule(Rule::OneOf(rules))
    }

    fn get_rule(&self, id: RuleRef) -> Rule {
        self.all_rules[id].clone()
    }

    /// gets a rule, but split out alts, as needed in the parser use case
    fn get_rule_alts(&self, id: RuleRef) -> Vec<RuleRef> {
        match self.get_rule(id) {
            Rule::OneOf(rules) => rules,       
            _ => vec![id]
        }
    }

    fn get_rule_len(&self, id: RuleRef) -> usize {
        match self.get_rule(id) {
            Rule::Seq(v) => v.len(),
            _ => 1
        }
    }

    fn add_sentence_internal(&mut self, sentence: Sentence) {
        self.all_sentences.insert(sentence.subj.clone(), sentence);
    }

    fn add_sentence(&mut self, subj: &str, id: RuleRef) {
        self.add_sentence_internal(Sentence {subj: SmolStr::new(subj), rule: id, mark: None});
    }

    fn get_sentence(&self, subj: &str) -> (SmolStr, RuleRef) {
        let sentence = &self.all_sentences[subj];
        (sentence.subj.clone(), sentence.rule)
    }

    fn get_sentence_ruleref(&self, subj: &str) -> RuleRef {
        self.all_sentences[subj].rule
    }

    fn get_sentence_rule(&self, subj: &str) -> Rule {
        self.get_rule(self.get_sentence_ruleref(subj))
    }

    fn get_sentence_rule_alts(&self, subj: &str) -> Vec<RuleRef> {
        self.get_rule_alts(self.get_sentence_ruleref(subj))
    }

}

/// Name, origin, position, complete_rule, rule_len, rule_cursor
#[derive(Debug, Clone)]
pub struct Task(SmolStr, usize, usize, RuleRef, usize, usize);

struct TaskLong {
    name: SmolStr,    // The name of the rule that this alternative is a part of (e.g. statement);
    origin: usize,    // The position in the input that it started;
    pos: usize,       // The position that it is currently at;
    rule: RuleRef,    //nee The list of items in the alternative that have so far been successfully parsed, with their start position;
    todo: RuleRef,    // The list of items still to be processed.
    prog: usize,      // the "dot" location within this rule. 0=unstarted
}

#[derive(Debug)]
pub struct HacklesParser {
    queue: VecDeque<Task>,
    trace: HashMap<String, Task>, // cache key is "rulename[origin,pos]" where rulename = str, origin & pos are int
    parentage: HashMap<String, Vec<Task>>, // cache key is "ruleid@origin"
    pos: usize, // input position
    //token_line: u16,
    //token_col: u16,
}

/// Earley parser
impl HacklesParser {
    const EOF: char = '\x1f';

    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            trace: HashMap::new(),
            parentage: HashMap::new(),
            pos: 0,
        }
    }

    pub fn parse(&mut self, input: &str, grammar: Grammar, start: &str) -> HashMap<String, Task> {
        
        let tokens = input.chars().map(|x| x as Token).collect::<Vec<_>>();
        self.queue = VecDeque::new();
        self.trace = HashMap::new();

        let mut intok = tokens[0];
        self.pos = 0;
        //self.token_line = 1;
        //self.token_col = 1;

        // Seed with starting rule
        for rule_id in grammar.get_sentence_rule_alts(start) {
            self.queue.push_front(
                Task(SmolStr::new(start), 0, 0, rule_id, grammar.get_rule_len(rule_id), 0)
            );
        }
        
        // work through the queue
        while let Some(task) = self.queue.pop_front() {
            println!("{:#?}",task);

            let rule_subj = task.0.clone();
            let rule_origin = task.1;
            let rule_pos = task.2;
            let rule_complete = task.3;
            let rule_len = task.4;
            let rule_cursor = task.5;
            let rule = grammar.get_rule(rule_complete);

            // task in completed state? 
            if rule_cursor == rule_len {
                println!("COMPLETER rule:{} origin:{} pos:{} rule:{} len:{} cursor:{}", rule_subj, rule_origin, rule_pos, rule_complete, rule_len, rule_cursor);
                // find “parent” states at same origin that can produce this rule;
                let parents = self.get_parentage(rule_complete, rule_origin);
                // queue parent at next position
                for parent in parents {
                        self.queue.push_front(
                        Task(parent.0.clone(), parent.1 , parent.2 + 1, parent.3, parent.4, parent.5 + 1 )
                    )
                }

                // advance through input
                self.pos += 1;
                intok = *tokens.get(self.pos).unwrap_or(&HacklesParser::EOF);
                println!("Input now at position {}", self.pos);
                // TODO: detect and account for newline tokens
                continue;
            }

            match rule {
                Rule::Seq(child_ids) => {
                    println!("PREDICTOR (seq@{})", rule_cursor);
                    // advance through sequence; queue item at cursor pos
                    let next = child_ids[rule_cursor];
                    for downrule_id in grammar.get_rule_alts(next) {
                        self.record_parentage(downrule_id, rule_origin, &task);
                        self.queue.push_front(
                            Task(rule_subj.clone(), rule_origin, rule_pos, downrule_id, grammar.get_rule_len(downrule_id), 0)
                        );
                    }
                }
                Rule::Nonterm(name) => {
                    println!("PREDICTOR (nonterm={})", name);
                    // go one level deeper to see what this nonterminal expands to
                    let nt_defn = grammar.get_sentence_ruleref(name.as_str());
                    for downrule_id in grammar.get_rule_alts(nt_defn) {
                        self.record_parentage(downrule_id, rule_origin, &task);
                        self.queue.push_front(
                            Task(name.clone(), rule_origin, rule_pos, downrule_id, grammar.get_rule_len(downrule_id), 0)
                        );
                    }
                }
                Rule::LitChar(ch) => {
                    println!("SCANNER (char='{}')", ch);
                    if ch == intok {
                        // hit!
                        self.trace(task);
                        // CONTINUE task AT (pos incremented (input, sym))
                        self.queue.push_back(
                            Task(rule_subj.clone(), rule_origin, rule_pos, rule_complete, rule_len, rule_cursor + 1)
                        );
                    } else {
                        println!("non-matched char '{}' (expecting '{}'); terminating task", intok, ch);
                    }
                }
                Rule::LitCharOneOf(str) => {
                    println!("SCANNER (char one of '{}')", str);
                    if str.contains(intok) {
                        // hit!
                        self.trace(task);
                        self.queue.push_back(
                            Task(rule_subj.clone(), rule_origin, rule_pos, rule_complete, rule_len, rule_cursor + 1)
                        );
                    } else {
                        println!("non-matched char '{}' (expecting one of '{}'); terminating task", intok, str);
                    }
                }
                Rule::Empty => { unreachable!("Don't queue empty rules") }
                Rule::OneOf(_) => { unreachable!("Alternates should't get queued like this")}
            }

        }
        self.trace.clone()
    }

    fn trace_key(&self, task: &Task) -> String {
        format!("{}[{},{}]", task.0, task.1, task.2)
    }

    fn trace(&mut self, task: Task) -> () {
        self.trace.insert(self.trace_key(&task), task.clone());
    }

    fn parentage_key(&self, rule: RuleRef, pos: usize) -> String {
        format!("{}@{}", rule, pos)
    }

    fn record_parentage(&mut self, rule: RuleRef, pos: usize, task: &Task) {
        let key = self.parentage_key(rule, pos);
        let vec = self.parentage.entry(key).or_insert(Vec::new());
        vec.push(task.clone());
    }

    /// given a ruleref and position, find all the sentences that could've produced it
    fn get_parentage(&self, rule: RuleRef, pos: usize) -> Vec<Task> {
        dbg!(&self.parentage);
        dbg!(rule);
        dbg!(pos);
        let val = self.parentage.get(&self.parentage_key(rule, pos));
        val.map(|x| (*x).clone()).unwrap_or(Vec::new())
    }

}

// temporary hack to get hands-onß

pub fn get_simple1_grammer() -> Grammar {
    // doc = "a", "b"
    let mut grammar = Grammar::new();
    let a = grammar.add_literal('a');
    let b = grammar.add_literal('b');
    let seq = grammar.add_sequence(vec![a,b]);
    grammar.add_sentence("doc", seq);
    grammar
}

pub fn get_simple2_grammer() -> Grammar {
    // doc = "a" | "b"
    let mut grammar = Grammar::new();
    let a = grammar.add_literal('a');
    let b = grammar.add_literal('b');
    let choice = grammar.add_alternation(vec![a,b]);
    grammar.add_sentence("doc", choice);
    grammar
}

pub fn get_simple3_grammer() -> Grammar {
    // doc = a, b
    // a = "a" | "A"
    // b = "b" | "B"
    let mut grammar = Grammar::new();
    let aref = grammar.add_nonterminal("a");
    let bref = grammar.add_nonterminal("b");
    let a = grammar.add_literal('a');
    let a_ = grammar.add_literal('A');
    let a_choice = grammar.add_alternation(vec![a,a_]);
    let b = grammar.add_literal('b');
    let b_ = grammar.add_literal('B');
    let b_choice = grammar.add_alternation(vec![b, b_]);
    let seq = grammar.add_sequence(vec![aref, bref]);
    grammar.add_sentence("doc", seq);
    grammar.add_sentence("a", a_choice);
    grammar.add_sentence("b", b_choice);
    grammar
}

pub fn get_wiki_grammar() -> Grammar {
    // <P> ::= <S>
    // <S> ::= <S> "+" <M> | <M>
    // <M> ::= <M> "*" <T> | <T>
    // <T> ::= "1" | "2" | "3" | "4"
    let mut grammar = Grammar::new();
    let s_rule = grammar.add_nonterminal("S");
    grammar.add_sentence("P", s_rule);

    let m_rule = grammar.add_nonterminal("M");
    let t_rule = grammar.add_nonterminal("T");
    let plus = grammar.add_literal('+');
    let star = grammar.add_literal('*');
    let seq1 = grammar.add_sequence(vec![s_rule, plus, m_rule]);
    let alt1 = grammar.add_alternation(vec![seq1, m_rule]);
    let seq2 = grammar.add_sequence(vec![m_rule, star, t_rule]);
    let alt2 = grammar.add_alternation(vec![seq2, t_rule]);
    let digit = grammar.add_rule(Rule::LitCharOneOf(SmolStr::new("1234")));

    grammar.add_sentence("S", alt1);
    grammar.add_sentence("M", alt2);
    grammar.add_sentence("T", digit);
    grammar
}
