use std::{fmt, collections::HashMap};
use smol_str::SmolStr;

use crate::parser::DotNotation;

// TODO: add CharMatchers at the Grammar level, with builders similar to Rule::build().x().y().z()

/// the primary owner of all grammar data structures
#[derive(Debug, Clone)]
pub struct Grammar {
    definitions: HashMap<SmolStr, BranchingRule>,
    top_rule_name: SmolStr,
}

impl Grammar {
    pub fn new(top_rule_name: &str) -> Self {
        Self { definitions: HashMap::new(), top_rule_name: SmolStr::new(top_rule_name) }
    }

    /// merge contents of RuleBuilder (which might include entire synthesized named rules) into Grammar
    /// Consumes the RuleBuilder
    pub fn define(&mut self, name: &str, rb: RuleBuilder) {
        // 1) the main rule 
        let mark = Mark::Default; // TODO: get actual Mark
        let main_rule = Rule::new(rb.terms);
        let branching_rule = self.definitions.entry(SmolStr::new(name))
            .or_insert(BranchingRule::new(mark));
        branching_rule.add_alt_branch(main_rule);
        
        // 2) synthesized rules
        for (syn_name, builders) in rb.syn_rules {
            for builder in builders {
                let syn_branching_rule = self.definitions.entry(syn_name.clone())
                    .or_insert(BranchingRule::new(Mark::Skip));
                syn_branching_rule.add_alt_branch(Rule::new(builder.terms));
            }
        }
    }

    pub fn get_branching_rule<'a>(&'a self, name: &str) -> &'a BranchingRule {
        &self.definitions[name]
    }

    pub fn get_top_branching_rule<'a>(&'a self) -> &'a BranchingRule {
        self.get_branching_rule(&self.top_rule_name)
    }

    pub fn get_top_rule_name(&self) -> &str {
        &self.top_rule_name
    }
}

impl fmt::Display for Grammar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = string_builder::Builder::default();
        for (name, branching_rule) in &self.definitions {
            builder.append(name.to_string());
            builder.append(": ");
            let rules: Vec<String> = branching_rule.alts.clone().iter()
                .map(|r| r.to_string())
                .collect();
            builder.append(rules.join(" | "));
            builder.append(".\n");
        }
        write!(f, "{}", builder.string().unwrap())
    }
}

/// within a BranchingRule, iterate through the available Rules (branches)
pub struct RuleIter<'a>(&'a Vec<Rule>, usize);

impl<'a> Iterator for RuleIter<'a> {
    type Item = &'a Rule;
    fn next(&mut self) -> Option<Self::Item> {
        let rc = self.0.get(self.1);
        self.1 += 1;
        rc
    }
}

/// within a Rule, iterate through individual Terms
pub struct TermIter<'a>(&'a Vec<Term>, usize);

impl<'a> Iterator for TermIter<'a> {
    type Item = &'a Term;
    fn next(&mut self) -> Option<Self::Item> {
        let rc = self.0.get(self.1);
        self.1 += 1;
        rc
    }
}

/// all branches of a rule.
/// For example doc = a | b. { the part after the = }
/// would be repesented by two different entries in self.alts (each of which would be its own sequence of terms)
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BranchingRule {
    mark: Mark,
    alts: Vec<Rule>,
    is_internal: bool,
}

impl BranchingRule {
    pub fn new(mark: Mark) -> BranchingRule {
        BranchingRule { mark, alts: Vec::new(), is_internal: false }
    }

    fn add_alt_branch(&mut self, alt: Rule) {
        self.alts.push(alt);
    }

    pub fn iter(&self) -> RuleIter<'_> {
        RuleIter(&self.alts, 0)
    }
}

/// Representation of marks on rules or terms. These get used everywhere, so the varient names are kept short
/// @ for attribute
/// - for hidden
/// ^ for visible (default)
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Mark {
    Default,
    Viz,
    Skip,
    Attr,
}

impl fmt::Display for Mark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mark::Default => write!(f, ""),
            Mark::Viz => write!(f, "^"),
            Mark::Skip => write!(f, "-"),
            Mark::Attr => write!(f, "@"),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Rule {
    pub factors: Vec<Term>,
}

/// A single sequence of terms ( = various terminals or a nonterminal )
/// In many cases, individual terms need to get simplified.
/// For example foo = a, (b | c), d
/// would need to get broken into two Rules, like:
/// foo = a, --synthesized_nt, d
/// --synthesized_nt = b | c
impl Rule {
    pub fn new(terms: Vec<Term>) -> Rule {
        Rule { factors: terms }
    }

    pub fn len(&self) -> usize {
        self.factors.len()
    }

    pub fn dot_notator(&self) -> crate::parser::DotNotation {
        DotNotation::new(&self)
        //crate::parser::DotNotation { iteratee: self.clone(), matched_so_far: Vec::new() }
    }

    pub fn add_term(&mut self, term: Term) {
        self.factors.push(term);
    }

    pub fn iter(&self) -> TermIter<'_> {
        TermIter(&self.factors, 0)
    }

    /// builder interface
    pub fn build() -> RuleBuilder {
        RuleBuilder::new()
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s: String = self.factors.clone().iter()
            .map(|f| {
                format!("{:?}", f)
            })
            .collect::<Vec<_>>().join(", ");
        write!(f,"{}", s)
    }
}


#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Term {
    Term(Mark, CharMatcher),
    Nonterm(Mark, SmolStr),
}

/// At thit low level, an individual term is either a terminal or a nonterminal
impl Term {
    pub fn lit(ch: char) -> Term {
        Term::Term(Mark::Default, CharMatcher::Exact(ch))
    }

   pub fn nonterm(name: &str) -> Term {
        Term::Nonterm(Mark::Default, SmolStr::new(name))
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Term::Term(mark, cm) => write!(f, "{cm}"),
            Term::Nonterm(mark, str) => write!(f, "{str}"),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CharMatcher {
    Exact(char),
    OneOf(SmolStr),
    Range(char, char),
    // TODO: UnicodeRange(SmolStr),
    // TODO: Exclude(&CharMatcher),
    // TODO: Union(&CharMatcher, &CharMatcher)
}

impl CharMatcher {
    pub fn accept(&self, test: char) -> bool {
        match self {
            CharMatcher::Exact(ch) => *ch==test,
            CharMatcher::OneOf(lst) => lst.contains(test),
            CharMatcher::Range(bot, top) => test <= *top && test >= *bot,
            //_ => false,
        }
    }
}

impl fmt::Display for CharMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CharMatcher::Exact(ch) => write!(f, "'{ch}'"),
            CharMatcher::OneOf(str) => write!(f, "[\"{str}\"]"),
            CharMatcher::Range(bot, top) => write!(f, "[{bot}-{top}]"),
            // _ => write!(f, "?"),
        }
    }
}

static mut NEXT_ID: i32 = 0;

/// build rules in an ergonomic and efficient fashion
/// this format is explicitly accepted (merged) by grammar.add_rule
#[derive(Clone)]
pub struct RuleBuilder {
    /// the "main" TermList being built here
    terms: Vec<Term>,

    /// in the course of building a rule, we may end up synthesizing additional rules.
    /// These need to eventually get added into the resulting grammar
    syn_rules: HashMap<SmolStr, Vec<RuleBuilder>>,
}

impl RuleBuilder {
    pub fn new() -> RuleBuilder {
        RuleBuilder { terms: Vec::new(), syn_rules: HashMap::new() }
    }

    /// accept a single char
    pub fn ch(mut self, ch: char) -> RuleBuilder {
        self.mark_ch(ch, Mark::Default)
    }

    /// accept a single char, with specified mark
    pub fn mark_ch(mut self, ch: char, mark: Mark) -> RuleBuilder {
        let term = Term::Term(mark, CharMatcher::Exact(ch));
        self.terms.push(term);
        self
    }

    /// accept a single char out of a list
    pub fn ch_in(mut self, chrs: &str) -> RuleBuilder {
        self.mark_ch_in(chrs, Mark::Default)
    }

    /// accept a single char out of a list, with specified mark
    pub fn mark_ch_in(mut self, chrs: &str, mark: Mark) -> RuleBuilder {
        let term = Term::Term(mark, CharMatcher::OneOf(SmolStr::new(chrs)));
        self.terms.push(term);
        self
    }

    /// accept a single character within a range
    pub fn ch_range(mut self, bot: char, top: char) -> RuleBuilder {
        self.mark_ch_range(bot, top, Mark::Default)
    }

    /// accept a single character within a range, with specified mark
    pub fn mark_ch_range(mut self, bot: char, top: char, mark: Mark) -> RuleBuilder {
        let term = Term::Term(mark, CharMatcher::Range(bot, top));
        self.terms.push(term);
        self
    }

    /// nonterminal
    pub fn nt(mut self, name: &str) -> RuleBuilder {
        self.nt_mark(name, Mark::Default)
    }

    /// nonterminal, with specified mark
    pub fn nt_mark(mut self, name: &str, mark: Mark) -> RuleBuilder {
        let term = Term::Nonterm(mark, SmolStr::new(name));
        self.terms.push(term);
        self
    }

    /// record an entirely new (internal, synthesized) named rule
    fn syn_rule(mut self, name: &str, mut rb: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut rb);
        let vec = self.syn_rules.entry(SmolStr::new(name)).or_insert(Vec::new());
        vec.push(rb);
        self
    }

    /// take primary rule from another RuleBuilder
    pub fn expr(mut self, mut sub: RuleBuilder) -> RuleBuilder {
        for t in sub.terms.drain(..) {
            self.terms.push(t)
        }
        self
    }
    
    /// call this on any sub-rules to make sure any generated syn_rules get passed along.
    fn siphon(mut self, sub: &mut RuleBuilder) -> RuleBuilder {
        for (name, rule) in sub.syn_rules.drain() {
            self.syn_rules.insert(name, rule);
        }
        self
    }

    /// f? ⇒ f-option
    /// -f-option: f | ().
    pub fn opt(mut self, mut sub: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut sub);
        // 1 create new rule 'f-option'
        let f_option: &str = &self.mint_internal_id("f-option");
        self = self.syn_rule(f_option, Rule::build()); // empty
        self = self.syn_rule(f_option, sub);
        // 2 insert newly created nt into sequence under construction
        self.nt_mark(f_option, Mark::Skip)
    }

    /// f* ⇒ f-star
    /// -f-star: (f, f-star)?.
    pub fn repeat0(mut self, mut sub: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut sub);
        // 1 create new rule 'f-star'
        let f_star: &str = &self.mint_internal_id("f-star");
        self = self.syn_rule(f_star, Rule::build().opt(Rule::build().expr(sub).nt(f_star)));
        // 2 insert newly-created nt into sequence under construction
        self.nt_mark(f_star, Mark::Skip)
    }

    /// f+ ⇒ f-plus
    /// -f-plus: f, f*.
    pub fn repeat1(mut self, mut sub: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut sub);
        // create new rule 'f-plus'
        let f_plus: &str = &self.mint_internal_id("f-plus");
        self = self.syn_rule(f_plus, Rule::build().expr(sub.clone()).repeat0(Rule::build().expr(sub)));
        // 2 insert newly-created nt into sequence under construction
        self.nt_mark(f_plus, Mark::Skip)
    }

    /// f++sep ⇒ f-plus-sep
    /// -f-plus-sep: f, (sep, f)*.
    pub fn repeat1_sep(mut self, mut sub1: RuleBuilder, mut sub2: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut sub1);
        self = self.siphon(&mut sub2);
        // create new rule 'f-plus-sep'
        let f_plus_sep: &str = &self.mint_internal_id("f-plus-sep");
        self = self.syn_rule(f_plus_sep, Rule::build().expr(sub1.clone()).repeat0(Rule::build().expr(sub2).expr(sub1)));
        // 2 insert newly-created nt into sequence under construction
        self.nt_mark(f_plus_sep, Mark::Skip)
    }    

    /// f**sep ⇒ f-star-sep
    /// -f-star-sep: (f++sep)?.
    pub fn repeat0_sep(mut self, mut sub1: RuleBuilder, mut sub2: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut sub1);
        self = self.siphon(&mut sub2);
        // create new rule 'f-star-sep'
        let f_star_sep: &str = &self.mint_internal_id("f-star-sep");
        self = self.syn_rule(f_star_sep, Rule::build().opt( Rule::build().repeat1_sep(sub1, sub2)));
        // 2 insert newly-created nt into sequence under construction
        self.nt_mark(f_star_sep, Mark::Skip)
    }

    /// internal identifier for synthesized rules
    /// all internal ids start with double hyphens
    fn mint_internal_id(&mut self, hint: &str) -> String {
        unsafe {
            let s = format!("--{}{}", hint, NEXT_ID);
            NEXT_ID += 1;
            s
        }
    }

}
