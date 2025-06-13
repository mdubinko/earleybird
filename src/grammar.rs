//! An ixml grammar definition based on https://invisiblexml.org/
//! 
//! ixml grammars are defined down to the character-level -- there is no "lexing" phase.
//! 
//! A simple example ixml grammar might be:
//! doc = "A", "B" | "C", "D".
//! 
//! A grammar is encoded as a map of definitions: `SmolStr` -> `BranchingRule`
//! (`SmolStr` is a O(1)-to-clone immutable string type)
//! The first definition is taken to be the "root" rule of the grammar
//! 
//! In the example gramamr there are two possible branches for the "doc" rule - either ("A","B") or ("C","D")
//! A `BranchingRule` captures all possible alternatives (called 'alts' in the ixml spec)
//! A `BranchingRule` contains a Mark, a Vec<Rule>, and an `is_internal` flag.
//! (In ixml, a Mark can be a @ prefix indicating an attribute, or a - prefix indicating to skip over this term)
//! 
//! A (non-branching) Rule is always a sequence of zero or more Factors, a Vec<Factor>
//! A Factor is an enum of either
//! `Terminal`(TMark, Lit)  (a `TMark` is like a Mark, except there is no @ prefix)
//! or
//! `Nonterm`(Mark, `SmolStr`) which is a reference to a different definition (which must exist elsewhere in the grammar)
//!
//! More complicated structures like x? or x+ or x* or x++y or x**y
//! are built from the existing primitives and recursive definitions
//! 
//! This module includes an ergonomic interface for building grammars by hand,
//! or from the output of upstream processes (including ixml parsing!)

use std::{fmt, collections::HashMap, cell::Cell};
use smol_str::SmolStr;
use indextree::{Arena, NodeId};
use crate::{parser::DotNotation, unicode_ranges::UnicodeRange};

// TODO: Optimization: add CharMatchers at the Grammar level

/// the primary owner of all grammar data structures
#[derive(Debug, Clone)]
pub struct Grammar {
    definitions: HashMap<SmolStr, BranchingRule>,
    /// remember insertion order of rules (used for tests & comparing grammars)
    pub defn_order: Vec<SmolStr>,
}

impl Grammar {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            defn_order: Vec::new(),
        }
    }

    pub fn get_rule_count(&self) -> usize {
        assert_eq!(self.definitions.len(), self.defn_order.len());
        self.definitions.len()
    }

    /// merge contents of `RuleBuilder` (which might include entire synthesized named rules) into Grammar
    /// Including the given Mark
    /// Consumes the `RuleBuilder`
    /// The first definition on a grammar is taken as the root rule
    pub fn define(&mut self, name: &str, sb: SeqBuilder) {
        self.mark_define(Mark::Default, name, sb);
    }

    /// merge contents of `RuleBuilder` (which might include entire synthesized named rules) into Grammar
    /// Consumes the `RuleBuilder`
    pub fn mark_define(&mut self, mark: Mark, name: &str, sb: SeqBuilder) {
        // 1) the main rule 
        let name_smol = SmolStr::new(name);
        let main_rule = Rule::new(sb.factors);
        let branching_rule = self.definitions.entry(name_smol.clone())
            .or_insert_with(|| {
                self.defn_order.push(name_smol.clone());
                BranchingRule::new(mark)
            });
        branching_rule.add_alt_branch(main_rule);
        
        // 2) synthesized rules
        //for (syn_name, builders) in sb.syn_rules {
        for syn_name in sb.defn_order {
            let builders = sb.syn_rules[&syn_name].to_vec(); // TODO: NOT copy
            for builder in builders {
                let syn_branching_rule = self.definitions.entry(syn_name.clone())
                    .or_insert_with(|| {
                        self.defn_order.push(syn_name.clone());
                        BranchingRule::new(Mark::Mute)
                    });
                syn_branching_rule.add_alt_branch(Rule::new(builder.factors));
            }
        }
    }

    pub fn get_root_definition_name(&self) -> Option<String> {
        self.defn_order.get(0).map(smol_str::SmolStr::to_string)
    }

    pub fn get_root_definition(&self) -> Result<Option<&BranchingRule>, crate::parser::ParseError> {
        match self.defn_order.get(0) {
            Some(s) => Ok(Some(self.get_definition(s)?)),
            None => Ok(None),
        }
    }
    
    pub fn get_definition_mark(&self, name: &str) -> Result<Mark, crate::parser::ParseError> {
        if !self.definitions.contains_key(name) {
            return Err(crate::parser::ParseError::static_err(&format!("missing rule named {name}")));
        }
        Ok(self.definitions[name].mark)
    }

    pub fn get_definition(&self, name: &str) -> Result<&BranchingRule, crate::parser::ParseError> {
        if !self.definitions.contains_key(name) {
            return Err(crate::parser::ParseError::static_err(&format!("missing rule definition for {name}")));
        }
        Ok(&self.definitions[name])
    }

    /// Parse an iXML grammar string and construct a Grammar
    pub fn from_ixml_str(ixml: &str) -> Result<Grammar, crate::parser::ParseError> {
        use crate::{ixml_bootstrap::ixml_grammar, parser::Parser};
        
        let mut ixml_parser = Parser::new(ixml_grammar());
        let ixml_arena = ixml_parser.parse(ixml.trim())?;
        let grammar = Grammar::from_parse_tree(&ixml_arena)?;
        Ok(grammar)
    }

    /// Convert a parse tree (Arena<Content>) from iXML parsing into a Grammar
    pub fn from_parse_tree(arena: &Arena<crate::parser::Content>) -> Result<Grammar, crate::parser::ParseError> {
        use crate::parser::{Parser, Content};
        
        let mut g = Grammar::new();

        let root_node = arena.iter().next().unwrap(); // first item == root
        let root_id = arena.get_node_id(root_node).unwrap();

        // first a pass over everything, making some indexes as we go
        let mut all_rules: Vec<NodeId> = Vec::new();

        // more validation checks go here...

        for nid in root_id.descendants(arena) {
            let content = arena.get(nid).unwrap().get();
            match content {
                Content::Element(name) if name=="rule" => all_rules.push(nid),
                _ => {}
            }
        }
        if all_rules.is_empty() {
            return Err(crate::parser::ParseError::static_err("can't convert ixml tree to grammar: no rules present"));
        }
        for rule in all_rules {
            let rule_attrs = Parser::get_attributes(arena, rule);
            let rule_name = &rule_attrs["name"];
            let rule_mark = rule_attrs.get("mark");
            let mark = match rule_mark.map(|s| s.as_str()) {
                Some("@") => Mark::Attr,
                Some("-") => Mark::Mute,
                Some("^") => Mark::Unmute,
                _ => Mark::Default,
            };
            Grammar::construct_rule_from_tree(rule, mark, arena, rule_name, &mut g);
        }
        Ok(g)
    }

    /// Helper function: Fully construct one rule from parse tree
    fn construct_rule_from_tree(rule: NodeId, mark: Mark, arena: &Arena<crate::parser::Content>, rule_name: &str, g: &mut Grammar) {
        use crate::parser::Parser;
        
        let ctx = RuleContext::new(rule_name);
        for (name, eid) in Parser::get_child_elements(arena, rule) {
            if name=="alt" {
                let rb = Grammar::build_sequence_from_tree(eid, arena, &ctx);
                g.mark_define(mark, rule_name, rb);
            }
        }
    }

    /// Helper function: Construct a sequence from parse tree node
    fn build_sequence_from_tree<'a>(node: NodeId, arena: &'a Arena<crate::parser::Content>, ctx: &'a RuleContext) -> SeqBuilder<'a> {
        use crate::parser::Parser;
        
        let mut seq = ctx.seq();
        for (name, nid) in Parser::get_child_elements(arena, node) {
            seq = Grammar::append_factor_from_tree(seq, &name, nid, arena, ctx);
        }
        seq
    }

    /// Helper function: Add factors to sequence from parse tree
    fn append_factor_from_tree<'a>(mut seq: SeqBuilder<'a>, name: &str, nid: NodeId, arena: &'a Arena<crate::parser::Content>, ctx: &'a RuleContext) -> SeqBuilder<'a> {
        use crate::parser::Parser;
        
        let attrs = Parser::get_attributes(arena, nid);
        match name {
            "alts" => {
                // an <alts> with only one <alt> child can be inlined, otherwise we give it the full treatment
                let alt_elements = Parser::get_child_elements_named(arena, nid, "alt");
                if alt_elements.len()==1 {
                    seq = Grammar::append_factor_from_tree(seq, "alt", alt_elements[0], arena, ctx);
                } else {
                    let altrules: Vec<SeqBuilder> = alt_elements.iter()
                        .map(|n| Grammar::build_sequence_from_tree(*n, arena, &ctx))
                        .collect();
                    seq = seq.alts(altrules);
                }
            }
            "literal" => {
                seq = seq.ch(attrs["string"].chars().next().expect("no empty string literals"));
            }
            "inclusion" => {
                // character classes - basic implementation for simple character sets
                // TODO: handle full Unicode ranges and complex sets
                if let Some(string_attr) = attrs.get("string") {
                    seq = seq.ch_in(string_attr);
                }
            }
            "exclusion" => {
                // character classes - basic implementation for simple character sets  
                // TODO: handle full Unicode ranges and complex sets
                if let Some(string_attr) = attrs.get("string") {
                    seq = seq.lit(Lit::union().exclude().ch_in(string_attr));
                }
            }
            "nonterminal" => {
                seq = seq.nt(&attrs["name"]);
            }
            "option" => {
                let subexpr = Grammar::build_sequence_from_tree(nid, arena, &ctx);
                seq = seq.opt(subexpr);
            }
            "repeat0" => {
                let children = Parser::get_child_elements(arena, nid);
                // assume first child is what-to-repeat (from `factor`)
                let expr = children.get(0).expect("Should always be at least one child here");
                let repeat_this_node = expr.1;
                let mut repeat_this = ctx.seq();
                repeat_this = Grammar::append_factor_from_tree(repeat_this, &expr.0, repeat_this_node, arena, ctx);

                // if a <sep> child exists, this is a ** rule, otherwise just *
                if let Some(sep) = children.get(1) {
                    assert_eq!(sep.0, "sep");
                    let separated_by = Grammar::build_sequence_from_tree(sep.1, arena, &ctx);
                    seq = seq.repeat0_sep(repeat_this, separated_by)
                } else {
                    seq = seq.repeat0(repeat_this);
                }
            }
            "repeat1" => {
                let children = Parser::get_child_elements(arena, nid);
                // assume first child is what-to-repeat (from `factor`)
                let expr = children.get(0).expect("Should always be at least one child here");
                let repeat_this_node = expr.1;
                let mut repeat_this = ctx.seq();
                repeat_this = Grammar::append_factor_from_tree(repeat_this, &expr.0, repeat_this_node, arena, ctx);

                // if a <sep> child exists, this is a ++ rule, otherwise just +
                if let Some(sep) = children.get(1) {
                    assert_eq!(sep.0, "sep");
                    let separated_by = Grammar::build_sequence_from_tree(sep.1, arena, &ctx);
                    seq = seq.repeat1_sep(repeat_this, separated_by)
                } else {
                    seq = seq.repeat1(repeat_this);
                }
            }
            _ => unimplemented!("unknown element {name} child of <alt>"),
        }
        seq
    }
}

impl fmt::Display for Grammar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut builder = string_builder::Builder::default();
        // iterate in insertion order...
        for name in &self.defn_order {
            let branching_rule = &self.definitions[name];
            builder.append(branching_rule.mark.to_string());
            builder.append(name.to_string());
            builder.append("= ");
            let rules: Vec<String> = branching_rule.alts.clone().iter()
                .map(std::string::ToString::to_string)
                .collect();
            builder.append(rules.join(" | "));
            builder.append(".\n");
        }
        write!(f, "{}", builder.string().unwrap())
    }
}

/// within a `BranchingRule`, iterate through the available Rules (branches)
pub struct RuleIter<'a>(&'a Vec<Rule>, usize);

impl<'a> Iterator for RuleIter<'a> {
    type Item = &'a Rule;
    fn next(&mut self) -> Option<Self::Item> {
        let rc = self.0.get(self.1);
        self.1 += 1;
        rc
    }
}

/// within a Rule, iterate through individual `Term`s
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
    pub fn new(mark: Mark) -> Self {
        Self { mark, alts: Vec::new(), is_internal: false }
    }

    fn add_alt_branch(&mut self, alt: Rule) {
        self.alts.push(alt);
    }

    pub fn iter(&self) -> RuleIter<'_> {
        RuleIter(&self.alts, 0)
    }

    pub fn mark(&self) -> Mark {
        self.mark
    }
}

/// Representation of marks on rules or nonterminal references.
/// These get used often, so the varient names are kept short
/// @ for attribute
/// - for hidden
/// ^ for visible (default)
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Mark {
    Default,
    Unmute,  // ^
    Mute,    // -
    Attr,    // @
}

impl fmt::Display for Mark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, ""),
            Self::Unmute => write!(f, "^"),
            Self::Mute => write!(f, "-"),
            Self::Attr => write!(f, "@"),
        }
    }
}

/// Representation of tmarks on terminals
/// (Much like Mark, except no Attr variant)
/// These get used often, so the varient names are kept short
/// - for hidden
/// ^ for visible (default)
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TMark {
    Default,
    Unmute,
    Mute,
}

impl fmt::Display for TMark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => write!(f, ""),
            Self::Unmute => write!(f, "^"),
            Self::Mute => write!(f, "-"),
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
/// foo = a, --synthesizedNT, d
/// --synthesizedNT = b | c
impl Rule {
    pub fn new(terms: Vec<Factor>) -> Self {
        Self { factors: terms }
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


/// At this low level, an individual `Factor` is either a terminal or a nonterminal
/// TODO: insertions
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Factor {
    Terminal(TMark, Lit),
    Nonterm(Mark, SmolStr),
}

impl Factor {
    /// drain off the matchers from a `LitBuilder`, producing a new `Factor::Terminal`
    fn new_lit(builder: LitBuilder, tmark: TMark) -> Self {
        let is_exclude = builder.lit.is_exclude;
        let mut lit = Lit::new();
        lit.matchers = builder.lit.matchers;
        lit.is_exclude = is_exclude;
        Self::Terminal(tmark, lit)
    }
}

impl fmt::Display for Factor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Terminal(tmark, lit) => write!(f, "{tmark}{lit}"),
            Self::Nonterm(mark, str) => write!(f, "{mark}{str}"),
        }
    }
}


// #[derive(Debug, Clone, Copy, Eq, PartialEq)]
// struct CharMatchId(usize); // Currently unused

/// A character matcher can be an arbitrarily long set of matchspecs (which are considered logically OR'd)
/// e.g. ["0"-"9" | "?" | #64 | Nd]
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
        let s: String = self.matchers.iter().map(std::string::ToString::to_string).collect::<Vec<_>>().join(" | ");
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
            Self::Exact(ch) => *ch==test,
            Self::OneOf(lst) => lst.contains(test),
            Self::Range(bot, top) => test <= *top && test >= *bot,
            Self::UnicodeRange(name) => UnicodeRange::new(name).accept(test),
        }
    }
}

impl fmt::Display for CharMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(ch) => write!(f, "'{ch}'"),
            Self::OneOf(str) => write!(f, "[\"{str}\"]"),
            Self::Range(bot, top) => write!(f, "[\"{bot}\"-\"{top}\"]"),
            Self::UnicodeRange(name) => write!(f, "Unicode range {name}"),
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
    pub fn ch(mut self, ch: char) -> Self {
        let matcher = CharMatcher::Exact(ch);
        self.lit.matchers.push(matcher);
        self
    }

    /// accept a single char out of a list
    pub fn ch_in(mut self, chrs: &str) -> Self {
        let matcher = CharMatcher::OneOf(SmolStr::new(chrs));
        self.lit.matchers.push(matcher);
        self
    }

    /// accept a single character within a range
    pub fn ch_range(mut self, bot: char, top: char) -> Self {
        let matcher = CharMatcher::Range(bot, top);
        self.lit.matchers.push(matcher);
        self
    }

    pub fn ch_unicode(mut self, range: &str) -> Self {
        let matcher = CharMatcher::UnicodeRange(SmolStr::new(range));
        self.lit.matchers.push(matcher);
        self
    }

    pub fn exclude(mut self) -> Self {
        self.lit.is_exclude = true;
        self
    }
}

/// A general way to track specifics needed to name rules
/// You should construct a new `BuilderContext` for each named rule when building a grammar
/// (either hand-assembling, or parsing)
/// Do not re-use a `BuilderContext` iff you're interested in testing and comparability
#[derive(Debug)]
pub struct RuleContext {
    rulename: String,
    next_id: Cell<i32>,
}

impl RuleContext {
    pub fn new(rulename: &str) -> Self {
        RuleContext { rulename: rulename.to_string(), next_id: Cell::new(1) }
    }

    pub fn seq(&self) -> SeqBuilder {
        SeqBuilder::new(self)
    }

    /// NOT threadsafe. Note interior mutability
    /// designed to be called from a `SeqBuilder` holding a backreference to this context
    fn get_and_increment_next_id(&self) -> i32 {
        let nid = self.next_id.get();
        self.next_id.set(nid + 1);
        nid
    }

    fn get_rulename(&self) -> &str {
        &self.rulename
    }
}

/// build rules in an ergonomic and efficient fashion
/// this format is explicitly accepted (merged) by `grammar.add_rule`
#[derive(Debug, Clone)]
pub struct SeqBuilder<'a> {
    /// the "main" TermList being built here
    factors: Vec<Factor>,

    /// in the course of building a rule, we may end up synthesizing additional rules.
    /// These need to eventually get added into the resulting grammar
    syn_rules: HashMap<SmolStr, Vec<SeqBuilder<'a>>>,
    defn_order: Vec<SmolStr>,

    context: &'a RuleContext,
}

impl<'a> SeqBuilder<'a> {

    fn new(context: &'a RuleContext) -> Self {
        Self { factors: Vec::new(), syn_rules: HashMap::new(), defn_order: Vec::new(), context }
    }

    /// Convenience function: accept a single char
    pub fn ch(self, ch: char) -> Self {
        self.mark_ch(ch, TMark::Default)
    }

    /// Convenience function: accept a single char, with specified `TMark`
    pub fn mark_ch(mut self, ch: char, tmark: TMark) -> Self {
        let factor = Factor::new_lit(Lit::union().ch(ch) , tmark);
        self.factors.push(factor);
        self
    }

    /// Convenience function: accept a single char out of a list
    pub fn ch_in(self, chrs: &str) -> Self {
        self.mark_ch_in(chrs, TMark::Default)
    }

    /// Convenience function: accept a single char out of a list, with specified `TMark`
    pub fn mark_ch_in(mut self, chrs: &str, tmark: TMark) -> Self {
        let factor = Factor::new_lit( Lit::union().ch_in(chrs), tmark);
        self.factors.push(factor);
        self
    }

    /// Convenience function: accept a single character within a range
    pub fn ch_range(self, bot: char, top: char) -> Self {
        self.mark_ch_range(bot, top, TMark::Default)
    }

    /// Convenience function: accept a single character within a range, with specified `TMark`
    pub fn mark_ch_range(mut self, bot: char, top: char, tmark: TMark) -> Self {
        let factor = Factor::new_lit(Lit::union().ch_range(bot, top), tmark);
        self.factors.push(factor);
        self
    }

    /// Convenience function: accept a single character within a Unicode range
    pub fn ch_unicode(self, name: &str) -> Self {
        self.mark_ch_unicode(name, TMark::Default)
    }

    /// Convenience function: accept a single character within a Unicode range, with specified `TMark`
    pub fn mark_ch_unicode(mut self, name: &str, tmark: TMark) -> Self {
        let factor = Factor::new_lit(Lit::union().ch_unicode(name), tmark);
        self.factors.push(factor);
        self
    }

    /// if convenience funcutions don't sufice, build your own Lit here
    pub fn lit(self, lit: LitBuilder) -> Self {
        self.mark_lit(lit, TMark::Default)
    }

    /// if convenience funcutions don't sufice, build your own Lit here, with specified `TMark`
    pub fn mark_lit(mut self, lit: LitBuilder, tmark: TMark) -> Self {
        let factor = Factor::new_lit(lit, tmark);
        self.factors.push(factor);
        self
    }

    /// nonterminal
    pub fn nt(self, name: &str) -> Self {
        self.mark_nt(name, Mark::Default)
    }

    /// nonterminal, with specified Mark
    pub fn mark_nt(mut self, name: &str, mark: Mark) -> Self {
        let term = Factor::Nonterm(mark, SmolStr::new(name));
        self.factors.push(term);
        self
    }

    /// record an entirely new (internal, synthesized) named rule
    fn syn_rule(mut self, name: &str, mut rb: Self) -> Self {
        self = self.siphon(&mut rb);
        let vec = self.syn_rules.entry(SmolStr::new(name)).or_insert(Vec::new());
        vec.push(rb);
        let smol_name = SmolStr::new(name);
        if !self.defn_order.contains(&smol_name) {
            self.defn_order.push(smol_name); // maintain insertion order
        }
        self
    }

    /// take primary rule from another `RuleBuilder`
    pub fn expr(mut self, mut sub: Self) -> Self {
        for t in sub.factors.drain(..) {
            self.factors.push(t)
        }
        self
    }
    
    /// call this on any sub-rules to make sure any generated `syn_rules` get passed along.
    fn siphon(mut self, sub: &mut Self) -> Self {
        for name in sub.defn_order.drain(..) { // maintain insertion order
            let rule = sub.syn_rules.remove(&name); //.expect("intenal syn_rules and defn_order out of sync");
            if rule.is_none() {
                println!("###### {name} ######");
                dbg!(&self.defn_order);
                dbg!(&self.syn_rules);
            }
            let rule = rule.expect("defn_order and syn_rules out of sync");
            self.defn_order.push(name.clone());
            self.syn_rules.insert(name, rule);
        }
        self
    }

    /// f? ⇒ f-option
    /// -f-option: f | ().
    pub fn opt(mut self, mut sub: Self) -> Self {
        self = self.siphon(&mut sub);
        // 1 create new rule 'f-option'
        let f_option: &str = &self.mint_internal_id("f-option");
        let empty = self.context.seq();
        self = self.syn_rule(f_option, empty); // empty
        self = self.syn_rule(f_option, sub);
        // 2 insert newly created nt into sequence under construction
        self.mark_nt(f_option, Mark::Mute)
    }

    /// f* ⇒ f-star
    /// -f-star: (f, f-star)?.
    pub fn repeat0(mut self, mut sub: Self) -> Self {
        self = self.siphon(&mut sub);
        // 1 create new rule 'f-star'
        let f_star: &str = &self.mint_internal_id("f-star");
        let subseq1 = self.context.seq();
        let subseq2 = self.context.seq();
        self = self.syn_rule(f_star, subseq1.opt(subseq2.expr(sub).nt(f_star)));
        // 2 insert newly-created nt into sequence under construction
        self.mark_nt(f_star, Mark::Mute)
    }

    /// f+ ⇒ f-plus
    /// -f-plus: f, f*.
    pub fn repeat1(mut self, mut sub: Self) -> Self {
        self = self.siphon(&mut sub);
        // create new rule 'f-plus'
        let f_plus: &str = &self.mint_internal_id("f-plus");
        let subseq1 = self.context.seq();
        let subseq2 = self.context.seq();
        self = self.syn_rule(f_plus, subseq1.expr(sub.clone()).repeat0(subseq2.expr(sub)));
        // 2 insert newly-created nt into sequence under construction
        self.mark_nt(f_plus, Mark::Mute)
    }

    /// f++sep ⇒ f-plus-sep
    /// -f-plus-sep: f, (sep, f)*.
    pub fn repeat1_sep(mut self, mut sub1: Self, mut sub2: Self) -> Self {
        self = self.siphon(&mut sub1);
        self = self.siphon(&mut sub2);
        // create new rule 'f-plus-sep'
        let f_plus_sep: &str = &self.mint_internal_id("f-plus-sep");
        let subseq1 = self.context.seq();
        let subseq2 = self.context.seq();
        self = self.syn_rule(f_plus_sep, subseq1.expr(sub1.clone()).repeat0(subseq2.expr(sub2).expr(sub1)));
        // 2 insert newly-created nt into sequence under construction
        self.mark_nt(f_plus_sep, Mark::Mute)
    }    

    /// f**sep ⇒ f-star-sep
    /// -f-star-sep: (f++sep)?.
    pub fn repeat0_sep(mut self, mut sub1: Self, mut sub2: Self) -> Self {
        self = self.siphon(&mut sub1);
        self = self.siphon(&mut sub2);
        // create new rule 'f-star-sep'
        let f_star_sep: &str = &self.mint_internal_id("f-star-sep");
        let subseq1 = self.context.seq();
        let subseq2 = self.context.seq();
        self = self.syn_rule(f_star_sep, subseq1.opt( subseq2.repeat1_sep(sub1, sub2)));
        // 2 insert newly-created nt into sequence under construction
        self.mark_nt(f_star_sep, Mark::Mute)
    }

    /// inline set of options, one of which must match
    /// for example for
    /// a: "{", (b, "c" | a), "}".
    /// g.define("a", Rule::seq()
    ///     .ch('{')
    ///     .alts(vec![
    ///         Rule::seq().nt("b").ch('c'),
    ///         Rule::seq().nt("a"),
    ///     ]})
    ///     .ch('}')
    /// );
    pub fn alts(mut self, exprs: Vec<SeqBuilder<'a>>) -> Self {
        // create new rule f_opt
        let f_opt = &self.mint_internal_id("f-opt");
        for expr in exprs {
            self = self.syn_rule(f_opt, expr);
        }

        // 2 insert newly-created nt into sequence under construction
        self.nt(f_opt)
    }

    /// internal identifier for synthesized rules
    /// all internal ids start with double hyphens
    fn mint_internal_id(&mut self, hint: &str) -> String {
        let nid = self.context.get_and_increment_next_id();
        let rulename = self.context.get_rulename();
        let s = format!("--{}.{}{}", rulename, hint, nid);
        s
    }

}

