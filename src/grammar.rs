//! An ixml grammar definition based on https://invisiblexml.org/
//! 
//! ixml grammars are defined down to the character-level -- there is no "lexing" phase.
//! 
//! A simple example ixml grammar might be:
//! doc = "A", "B" | "C", "D".
//! 
//! A grammar is encoded as a map of definitions: SmolStr -> BranchingRule
//! (SmolStr is a O(1)-to-clone immutable string type)
//! 
//! In the example gramamr there are two possible branches for the "doc" rule - either ("A","B") or ("C","D")
//! A BranchingRule captures all possible alternatives (called 'alts' in the ixml spec)
//! A BranchingRule contains a Mark, a Vec<Rule>, and an is_internal flag.
//! (In ixml, a Mark can be a @ prefix indicating an attribute, or a - prefix indicating to skip over this term)
//! 
//! A (non-branching) Rule is always a sequence of zero or more Factors, a Vec<Factor>
//! A Factor is an enum of either
//! Terminal(TMark, Lit)  (a TMark is like a Mark, except there is no @ prefix)
//! or
//! Nonterm(Mark, SmolStr) which is a reference to a different definition (which must exist elsewhere in the grammar)
//!
//! More complicated structures like x? or x+ or x* or x++y or x**y
//! are built from the existing primitives and recursive definitions
//! 
//! This module includes an ergonomic interface for building grammars by hand,
//! or from the output of upstream processes (including ixml parsing!)

use std::{fmt, collections::HashMap};
use smol_str::SmolStr;
use crate::{parser::DotNotation, unicode_ranges::UnicodeRange};

// TODO: Optimization: add CharMatchers at the Grammar level

/// the primary owner of all grammar data structures
#[derive(Debug, Clone)]
pub struct Grammar {
    definitions: HashMap<SmolStr, BranchingRule>,
    root_definition_name: SmolStr,
}

impl Grammar {
    pub fn new(root_definition_name: &str) -> Self {
        Self {
            definitions: HashMap::new(),
            root_definition_name: SmolStr::new(root_definition_name)
            }
    }

    /// merge contents of RuleBuilder (which might include entire synthesized named rules) into Grammar
    /// Including the given Mark
    /// Consumes the RuleBuilder
    pub fn define(&mut self, name: &str, rb: RuleBuilder) {
        self.mark_define(Mark::Default, name, rb)
    }

    /// merge contents of RuleBuilder (which might include entire synthesized named rules) into Grammar
    /// Consumes the RuleBuilder
    pub fn mark_define(&mut self, mark: Mark, name: &str, rb: RuleBuilder) {
        // 1) the main rule 
        let main_rule = Rule::new(rb.factors);
        let branching_rule = self.definitions.entry(SmolStr::new(name))
            .or_insert_with(|| BranchingRule::new(mark));
        branching_rule.add_alt_branch(main_rule);
        
        // 2) synthesized rules
        for (syn_name, builders) in rb.syn_rules {
            for builder in builders {
                let syn_branching_rule = self.definitions.entry(syn_name.clone())
                    .or_insert_with(|| BranchingRule::new(Mark::Mute));
                syn_branching_rule.add_alt_branch(Rule::new(builder.factors));
            }
        }
    }

    pub fn get_root_definition_name(&self) -> &str {
        &self.root_definition_name
    }

    pub fn get_root_definition(&self) -> &BranchingRule {
        self.get_definition(&self.root_definition_name)
    }
    
    pub fn get_definition_mark(&self, name: &str) -> Mark {
        self.definitions[name].mark.clone()
    }

    pub fn get_definition(&self, name: &str) -> &BranchingRule {
        &self.definitions[name]
    }
}

impl fmt::Display for Grammar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = string_builder::Builder::default();
        for (name, branching_rule) in &self.definitions {
            builder.append(branching_rule.mark.to_string());
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
pub struct TermIter<'a>(&'a Vec<Factor>, usize);

impl<'a> Iterator for TermIter<'a> {
    type Item = &'a Factor;
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

    pub fn mark(&self) -> Mark {
        self.mark.clone()
    }
}

/// Representation of marks on rules or nonterminal references.
/// These get used often, so the varient names are kept short
/// @ for attribute
/// - for hidden
/// ^ for visible (default)
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Mark {
    Default,
    Unmute,
    Mute,
    Attr,
}

impl fmt::Display for Mark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mark::Default => write!(f, ""),
            Mark::Unmute => write!(f, "^"),
            Mark::Mute => write!(f, "-"),
            Mark::Attr => write!(f, "@"),
        }
    }
}

/// Representation of tmarks on terminals
/// (Much like Mark, except no Attr variant)
/// These get used often, so the varient names are kept short
/// - for hidden
/// ^ for visible (default)
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TMark {
    Default,
    Unmute,
    Mute,
}

impl fmt::Display for TMark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TMark::Default => write!(f, ""),
            TMark::Unmute => write!(f, "^"),
            TMark::Mute => write!(f, "-"),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Rule {
    pub factors: Vec<Factor>,
}

/// A single sequence of terms ( = various terminals or a nonterminal )
/// In many cases, individual terms need to get simplified.
/// For example foo = a, (b | c), d
/// would need to get broken into two Rules, like:
/// foo = a, --synthesized_nt, d
/// --synthesized_nt = b | c
impl Rule {
    pub fn new(terms: Vec<Factor>) -> Rule {
        Rule { factors: terms }
    }

    pub fn len(&self) -> usize {
        self.factors.len()
    }

    pub fn dot_notator(&self) -> crate::parser::DotNotation {
        DotNotation::new(self)
    }

    pub fn add_term(&mut self, term: Factor) {
        self.factors.push(term);
    }

    pub fn iter(&self) -> TermIter<'_> {
        TermIter(&self.factors, 0)
    }

    /// builder interface
    pub fn seq() -> RuleBuilder {
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


/// At thit low level, an individual Factor is either a terminal or a nonterminal
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Factor {
    Terminal(TMark, Lit),
    Nonterm(Mark, SmolStr),
}

impl Factor {
    /// drain off the matchers from a LitBuilder, producing a new Factor::Terminal
    fn new_lit(builder: LitBuilder, tmark: TMark) -> Factor {
        let is_exclude = builder.lit.is_exclude;
        let mut lit = Lit::new();
        lit.matchers = builder.lit.matchers;
        lit.is_exclude = is_exclude;
        Factor::Terminal(tmark, lit)
    }
}

impl fmt::Display for Factor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Factor::Terminal(tmark, lit) => write!(f, "{tmark}{lit}"),
            Factor::Nonterm(mark, str) => write!(f, "{mark}{str}"),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct CharMatchId(usize);

/// A character matcher can be an arbitrarily long set of matchspecs (which are considered logically OR'd)
/// e.g. ["0"-"9"; "?": Nd]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Lit {
    matchers: Vec<CharMatcher>,
    /// negative matchers invert the overall match logic
    /// e.g. ~["0"-"9"]
    is_exclude: bool,
}

impl Lit {
    fn new() -> Self {
        Self { matchers: Vec::new(), is_exclude: false}
    }

    fn make_excluding(&mut self) {
        self.is_exclude = true;
    }

    /// actually match the input char
    pub fn accept(&self, test: char) -> bool {
        if self.is_exclude {
            //self.matchers.iter().all(|m| !m.accept(test))
            !self.matchers.iter().any(|m| m.accept(test))
        } else {
            self.matchers.iter().any(|m| m.accept(test))
        }
    }

    pub fn union() -> LitBuilder {
        LitBuilder::new()
    }
}

impl fmt::Display for Lit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s: String = self.matchers.iter().map(|m| m.to_string()).collect::<Vec<_>>().join(" | ");
        let prefix = if self.is_exclude { "~" } else { "" };
        write!(f, "{prefix}[{s}]")
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum CharMatcher {
    Exact(char),
    OneOf(SmolStr),
    Range(char, char),
    UnicodeRange(SmolStr),
}

impl CharMatcher {
    pub fn accept(&self, test: char) -> bool {
        match self {
            CharMatcher::Exact(ch) => *ch==test,
            CharMatcher::OneOf(lst) => lst.contains(test),
            CharMatcher::Range(bot, top) => test <= *top && test >= *bot,
            CharMatcher::UnicodeRange(name) => UnicodeRange::new(name).accept(test),
        }
    }
}

impl fmt::Display for CharMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CharMatcher::Exact(ch) => write!(f, "'{ch}'"),
            CharMatcher::OneOf(str) => write!(f, "[\"{str}\"]"),
            CharMatcher::Range(bot, top) => write!(f, "[\"{bot}\"-\"{top}\"]"),
            CharMatcher::UnicodeRange(name) => write!(f, "Unicode range {name}"),
        }
    }
}

#[derive(Debug)]
pub struct LitBuilder {
    lit: Lit
}

impl LitBuilder {
    fn new() -> Self {
        Self { lit: Lit::new() }
    }

    /// accept a single char
    pub fn ch(mut self, ch: char) -> LitBuilder {
        let matcher = CharMatcher::Exact(ch);
        self.lit.matchers.push(matcher);
        self
    }

    /// accept a single char out of a list
    pub fn ch_in(mut self, chrs: &str) -> LitBuilder {
        let matcher = CharMatcher::OneOf(SmolStr::new(chrs));
        self.lit.matchers.push(matcher);
        self
    }

    /// accept a single character within a range
    pub fn ch_range(mut self, bot: char, top: char) -> LitBuilder {
        let matcher = CharMatcher::Range(bot, top);
        self.lit.matchers.push(matcher);
        self
    }

    pub fn ch_unicode(mut self, range: &str) -> LitBuilder {
        let matcher = CharMatcher::UnicodeRange(SmolStr::new(range));
        self.lit.matchers.push(matcher);
        self
    }

    pub fn exclude(mut self) -> LitBuilder {
        self.lit.is_exclude = true;
        self
    }
}

static mut NEXT_ID: i32 = 0;

/// build rules in an ergonomic and efficient fashion
/// this format is explicitly accepted (merged) by grammar.add_rule
#[derive(Clone)]
pub struct RuleBuilder {
    /// the "main" TermList being built here
    factors: Vec<Factor>,

    /// in the course of building a rule, we may end up synthesizing additional rules.
    /// These need to eventually get added into the resulting grammar
    syn_rules: HashMap<SmolStr, Vec<RuleBuilder>>,
}

impl RuleBuilder {
    pub fn new() -> RuleBuilder {
        RuleBuilder { factors: Vec::new(), syn_rules: HashMap::new() }
    }

    /// Convenience function: accept a single char
    pub fn ch(mut self, ch: char) -> RuleBuilder {
        self.mark_ch(ch, TMark::Default)
    }

    /// Convenience function: accept a single char, with specified TMark
    pub fn mark_ch(mut self, ch: char, tmark: TMark) -> RuleBuilder {
        let factor = Factor::new_lit(Lit::union().ch(ch) , tmark);
        self.factors.push(factor);
        self
    }

    /// Convenience function: accept a single char out of a list
    pub fn ch_in(mut self, chrs: &str) -> RuleBuilder {
        self.mark_ch_in(chrs, TMark::Default)
    }

    /// Convenience function: accept a single char out of a list, with specified TMark
    pub fn mark_ch_in(mut self, chrs: &str, tmark: TMark) -> RuleBuilder {
        let factor = Factor::new_lit( Lit::union().ch_in(chrs), tmark);
        self.factors.push(factor);
        self
    }

    /// Convenience function: accept a single character within a range
    pub fn ch_range(mut self, bot: char, top: char) -> RuleBuilder {
        self.mark_ch_range(bot, top, TMark::Default)
    }

    /// Convenience function: accept a single character within a range, with specified TMark
    pub fn mark_ch_range(mut self, bot: char, top: char, tmark: TMark) -> RuleBuilder {
        let factor = Factor::new_lit(Lit::union().ch_range(bot, top), tmark);
        self.factors.push(factor);
        self
    }

    /// Convenience function: accept a single character within a Unicode range
    pub fn ch_unicode(mut self, name: &str) -> RuleBuilder {
        self.mark_ch_unicode(name, TMark::Default)
    }

    /// Convenience function: accept a single character within a Unicode range, with specified TMark
    pub fn mark_ch_unicode(mut self, name: &str, tmark: TMark) -> RuleBuilder {
        let factor = Factor::new_lit(Lit::union().ch_unicode(name), tmark);
        self.factors.push(factor);
        self
    }

    /// if convenience funcutions don't sufice, build your own Lit here
    pub fn lit(mut self, lit: LitBuilder) -> RuleBuilder {
        self.mark_lit(lit, TMark::Default)
    }

    /// if convenience funcutions don't sufice, build your own Lit here, with specified TMark
    pub fn mark_lit(mut self, lit: LitBuilder, tmark: TMark) -> RuleBuilder {
        let factor = Factor::new_lit(lit, tmark);
        self.factors.push(factor);
        self
    }

    /// nonterminal
    pub fn nt(mut self, name: &str) -> RuleBuilder {
        self.mark_nt(name, Mark::Default)
    }

    /// nonterminal, with specified Mark
    pub fn mark_nt(mut self, name: &str, mark: Mark) -> RuleBuilder {
        let term = Factor::Nonterm(mark, SmolStr::new(name));
        self.factors.push(term);
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
        for t in sub.factors.drain(..) {
            self.factors.push(t)
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
        self = self.syn_rule(f_option, Rule::seq()); // empty
        self = self.syn_rule(f_option, sub);
        // 2 insert newly created nt into sequence under construction
        self.mark_nt(f_option, Mark::Mute)
    }

    /// f* ⇒ f-star
    /// -f-star: (f, f-star)?.
    pub fn repeat0(mut self, mut sub: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut sub);
        // 1 create new rule 'f-star'
        let f_star: &str = &self.mint_internal_id("f-star");
        self = self.syn_rule(f_star, Rule::seq().opt(Rule::seq().expr(sub).nt(f_star)));
        // 2 insert newly-created nt into sequence under construction
        self.mark_nt(f_star, Mark::Mute)
    }

    /// f+ ⇒ f-plus
    /// -f-plus: f, f*.
    pub fn repeat1(mut self, mut sub: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut sub);
        // create new rule 'f-plus'
        let f_plus: &str = &self.mint_internal_id("f-plus");
        self = self.syn_rule(f_plus, Rule::seq().expr(sub.clone()).repeat0(Rule::seq().expr(sub)));
        // 2 insert newly-created nt into sequence under construction
        self.mark_nt(f_plus, Mark::Mute)
    }

    /// f++sep ⇒ f-plus-sep
    /// -f-plus-sep: f, (sep, f)*.
    pub fn repeat1_sep(mut self, mut sub1: RuleBuilder, mut sub2: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut sub1);
        self = self.siphon(&mut sub2);
        // create new rule 'f-plus-sep'
        let f_plus_sep: &str = &self.mint_internal_id("f-plus-sep");
        self = self.syn_rule(f_plus_sep, Rule::seq().expr(sub1.clone()).repeat0(Rule::seq().expr(sub2).expr(sub1)));
        // 2 insert newly-created nt into sequence under construction
        self.mark_nt(f_plus_sep, Mark::Mute)
    }    

    /// f**sep ⇒ f-star-sep
    /// -f-star-sep: (f++sep)?.
    pub fn repeat0_sep(mut self, mut sub1: RuleBuilder, mut sub2: RuleBuilder) -> RuleBuilder {
        self = self.siphon(&mut sub1);
        self = self.siphon(&mut sub2);
        // create new rule 'f-star-sep'
        let f_star_sep: &str = &self.mint_internal_id("f-star-sep");
        self = self.syn_rule(f_star_sep, Rule::seq().opt( Rule::seq().repeat1_sep(sub1, sub2)));
        // 2 insert newly-created nt into sequence under construction
        self.mark_nt(f_star_sep, Mark::Mute)
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
