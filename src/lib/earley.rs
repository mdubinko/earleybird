use std::collections::HashMap;
use smol_str::SmolStr;

// const POSIMARK: char = '‸'; // "\u2038"

type Token = char; // maybe u32 later?
type RuleRef = usize;
type SentenceRef = usize;
type ParseChart = Vec<HashMap<SmolStr, Vec<DotRule>>>;

#[derive(Clone)]
pub enum Rule {
    Literal(SmolStr),
    LiteralCharOneOf(SmolStr),
    Nonterminal(SmolStr),
    SequenceOf3(RuleRef, RuleRef, RuleRef),
    OneOf2(RuleRef, RuleRef),
}

#[derive(Clone)]
pub struct Sentence {
    subj: SmolStr,
    rule: RuleRef,
}

pub struct Grammar {
    allRules: Vec<Rule>,
    allSentences: Vec<Sentence>,
}

impl Grammar {
    fn new() -> Grammar {
        Self { allRules: Vec::new(), allSentences: Vec::new() }
    }

    fn add_rule(&mut self, rule: Rule) -> RuleRef {
        self.allRules.push(rule);
        self.allRules.len()
    }

    fn add_nonterminal(&mut self, name: &str) -> RuleRef {
        self.add_rule(Rule::Nonterminal(SmolStr::new(name)))
    }

    fn add_literal(&mut self, value: &str) -> RuleRef {
        self.add_rule(Rule::Literal(SmolStr::new(value)))
    }

    fn add_sequence(&mut self, s1: RuleRef, s2: RuleRef, s3: RuleRef) -> RuleRef {
        self.add_rule(Rule::SequenceOf3(s1, s2, s3))
    }

    fn add_alternation(&mut self, a1: RuleRef, a2: RuleRef) -> RuleRef {
        self.add_rule(Rule::OneOf2(a1, a2))
    }

    fn get_rule(&self, id: RuleRef) -> Option<&Rule> {
        self.allRules.get(id)
    }

    fn add_sentence_internal(&mut self, sentence: Sentence) -> SentenceRef {
        self.allSentences.push(sentence);
        self.allSentences.len()
    }

    fn add_sentence(&mut self, subj: &str, id: RuleRef) -> SentenceRef {
        self.add_sentence_internal(Sentence {subj: SmolStr::new(subj), rule: id})
    }

    fn get_sentence(&self, id: SentenceRef) -> (SmolStr, RuleRef) {
        let sentence = &self.allSentences[id];
        (sentence.subj.clone(), sentence.rule)
    }
}

#[derive(Clone)]
enum DotRule {
    // a tuple (X → α • β, i), consisting of
    // 1. the current production X → α β
    // 2. the current position in that production (represented by the •)
    // 3. the position i in the input at which the matching of this production began: aka the origin position
    Residue(SmolStr, RuleRef, usize),
}

#[allow(dead_code)]
pub fn get_test_grammar() -> Grammar {
    // temporary hack to get hands-on
    // <P> ::= <S>      # the start rule
    // <S> ::= <S> "+" <M> | <M>
    // <M> ::= <M> "*" <T> | <T>
    // <T> ::= "1" | "2" | "3" | "4"
    let mut grammar = Grammar::new();
    let s_rule = grammar.add_nonterminal("S");
    grammar.add_sentence("P", s_rule);

    let m_rule = grammar.add_nonterminal("M");
    let t_rule = grammar.add_nonterminal("T");
    let plus = grammar.add_literal("+");
    let star = grammar.add_literal("*");
    let seq1 = grammar.add_sequence(s_rule, plus, m_rule);
    let alt1 = grammar.add_alternation(seq1, m_rule);
    let seq2 = grammar.add_sequence(m_rule, star, t_rule);
    let alt2 = grammar.add_alternation(seq2, t_rule);
    let digit = grammar.add_rule(Rule::LiteralCharOneOf(SmolStr::new("1234")));

    grammar.add_sentence("S", alt1);
    grammar.add_sentence("M", alt2);
    grammar.add_sentence("T", digit);
    grammar
}

pub fn parse(input: &str, grammar: Grammar) {
    
    // internal state
    // grammar from fn parameter, as-is
    // immutable tokens; parser input
    let tokens: Vec<Token> = input.chars().map(|x| x as Token).collect::<Vec<_>>();
    // current processing position, indexed into tokens Vec
    let mut pos = 0;
    // The S[k] chart of all parser states. Under each k, like names are grouped under a map key
    let mut chart = init_chart(input.len());
    // starting rule
    let (subj, ruleId) = grammar.get_sentence(pos);
    // let task_queue = ... TODO

    //chart[0].insert(&start.subj, DotRule::Residue(&start, 0 ));
    add_to_set(DotRule::Residue(subj, ruleId, pos), &mut chart, &grammar);

    //function EARLEY_PARSE(words, grammar)
    //INIT(words)
    //ADD_TO_SET((γ → •S, 0), S[0])
    //for k ← from 0 to LENGTH(words) do
    //    for each state in S[k] do  // S[k] can expand during this loop
    //        if not FINISHED(state) then
    //            if NEXT_ELEMENT_OF(state) is a nonterminal then
    //                PREDICTOR(state, k, grammar)         // non_terminal
    //            else do
    //                SCANNER(state, k, words)             // terminal
    //        else do
    //            COMPLETER(state, k)
    //    end
    //end
    //return chart
}


fn init_chart(size: usize) -> ParseChart {
    //function INIT(words)
    //S ← CREATE_ARRAY(LENGTH(words) + 1)
    //for k ← from 0 to LENGTH(words) do
    //    S[k] ← EMPTY_ORDERED_SET
    (0..size+1).map(|_| { HashMap::new() } ).collect::<Vec<_>>()
}

fn add_to_set(dotrule: DotRule, chart: &mut ParseChart, grammar: &Grammar) {
    match dotrule {
        DotRule::Residue(ref name, id, pos) => {
            let rules = chart[pos].entry(name.clone()).or_insert(Vec::new());
            rules.push(dotrule);
        },
    }
}

/*
procedure PREDICTOR((A → α•Bβ, j), k, grammar)
    for each (B → γ) in GRAMMAR_RULES_FOR(B, grammar) do
        ADD_TO_SET((B → •γ, k), S[k])
    end

procedure SCANNER((A → α•aβ, j), k, words)
    if a ⊂ PARTS_OF_SPEECH(words[k]) then
        ADD_TO_SET((A → αa•β, j), S[k+1])
    end

procedure COMPLETER((B → γ•, x), k)
    for each (A → α•Bβ, j) in S[x] do
        ADD_TO_SET((A → αB•β, j), S[k])
    end
*/

pub fn test() -> &'static str {
    "Test"
}
