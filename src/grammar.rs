//! An ixml grammar definition based on <https://invisiblexml.org/>
//!
//! ixml grammars are defined down to the character-level -- there is no "lexing" phase.
//!
//! A simple example ixml grammar might be:
//! doc = "A", "B" | "C", "D".
//!
//! A grammar is encoded as a map of definitions: `EarleyStr` -> `BranchingRule`
//! (`EarleyStr` is a O(1)-to-clone immutable string type)
//! The first definition is taken to be the "root" rule of the grammar
//!
//! In the example gramamr there are two possible branches for the "doc" rule - either ("A","B") or ("C","D")
//! A `BranchingRule` captures all possible alternatives (called 'alts' in the ixml spec)
//! A `BranchingRule` contains a Mark, a `Vec<Rule>`, and an `is_internal` flag.
//! (In ixml, a Mark can be a @ prefix indicating an attribute, or a - prefix indicating to skip over this term)
//!
//! A (non-branching) Rule is always a sequence of zero or more Factors, a `Vec<Factor>`
//! A Factor is an enum of either
//! `Terminal`(TMark, Lit)  (a `TMark` is like a Mark, except there is no @ prefix)
//! or
//! `Nonterm`(Mark, `EarleyStr`, alias) which is a reference to a different definition (which must exist elsewhere in the grammar)
//!
//! More complicated structures like x? or x+ or x* or x++y or x**y
//! are built from the existing primitives and recursive definitions
//!
//! This module includes an ergonomic interface for building grammars by hand,
//! or from the output of upstream processes (including ixml parsing!)

use crate::EarleyStr;
use crate::{debug::DebugLevel, parser::Parser, unicode_ranges::UnicodeRange};
use crate::{debug_grammar, ixml_bootstrap::bootstrap_ixml_grammar};
use indextree::{Arena, NodeId};
use std::{
    cell::{Cell, OnceCell},
    collections::HashMap,
    fmt,
    rc::Rc,
};

/// Key for caching nullability information for both BranchingRules and individual alternatives
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum NullabilityKey {
    /// Entire BranchingRule (true if any alternative is nullable)
    BranchingRule(EarleyStr),
    /// Specific alternative within a BranchingRule
    Alternative(EarleyStr, usize),
}

/// Detailed error types for the three-phase grammar parsing process
#[derive(Debug)]
pub enum GrammarConstructionError {
    /// Phase 1: Validation and preprocessing failed
    ValidationError(String),
    /// Phase 2: Bootstrap grammar couldn't parse the iXML
    BootstrapParseError(crate::parser::ParseError),
    /// Phase 3: Parse tree to Grammar conversion failed. Carries the structured
    /// `ParseError` (with its spec code, e.g. S03) so the code survives to the
    /// `from_ixml_str` boundary and the CLI exit code.
    ConversionError(crate::parser::ParseError),
}

impl fmt::Display for GrammarConstructionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            Self::BootstrapParseError(err) => write!(f, "Bootstrap parse error: {}", err),
            Self::ConversionError(msg) => write!(f, "Conversion error: {}", msg),
        }
    }
}
// TODO: Optimization: add CharMatchers at the Grammar level

struct NamingAttributes {
    mark: Option<String>,
    name: Option<String>,
    alias: Option<String>,
}

/// the primary owner of all grammar data structures
#[derive(Debug, Clone)]
pub struct Grammar {
    definitions: HashMap<EarleyStr, BranchingRule>,
    /// remember insertion order of rules (used for tests & comparing grammars)
    pub defn_order: Vec<EarleyStr>,
    /// unified cache for all nullability information - computed on demand
    nullability_cache: OnceCell<HashMap<NullabilityKey, bool>>,
    /// version declared in grammar prolog (if any). Our implementation version is always "1.0"
    /// NOTE: Future versions may support renaming syntax (rule>newname) and require version-specific parsing
    declared_version: Option<EarleyStr>,
}

/// Two grammars are equal when their *content* matches: the rule definitions,
/// their declaration order (the first definition is the root), and any declared
/// version. This is a manual impl, NOT derived, so it deliberately IGNORES
/// `nullability_cache` — a lazily-populated memo of facts derivable from the
/// rules. Otherwise two grammars with identical rules would compare unequal
/// merely because one had been used for parsing (cache populated) and the other
/// had not. `definitions` is a `HashMap` so it compares order-independently;
/// `defn_order` is compared as an ordered `Vec` (rule order, including the root,
/// is significant).
impl PartialEq for Grammar {
    fn eq(&self, other: &Self) -> bool {
        self.definitions == other.definitions
            && self.defn_order == other.defn_order
            && self.declared_version == other.declared_version
    }
}

impl Eq for Grammar {}

impl Default for Grammar {
    fn default() -> Self {
        Self::new()
    }
}

impl Grammar {
    pub fn new() -> Self {
        Self {
            definitions: HashMap::new(),
            defn_order: Vec::new(),
            nullability_cache: OnceCell::new(),
            declared_version: None,
        }
    }

    pub fn get_rule_count(&self) -> usize {
        assert_eq!(self.definitions.len(), self.defn_order.len());
        self.definitions.len()
    }

    /// Check if the declared version (if any) names a version we do not support.
    pub fn has_version_mismatch(&self) -> bool {
        self.declared_version
            .as_ref()
            .map(|v| !matches!(v.as_str(), "1.0" | "1.1"))
            .unwrap_or(false)
    }

    /// Get the declared version string (if any)
    pub fn declared_version(&self) -> Option<&str> {
        self.declared_version.as_ref().map(|s| s.as_str())
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
        self.mark_define_alias(mark, name, None, sb);
    }

    pub fn mark_define_alias(
        &mut self,
        mark: Mark,
        name: &str,
        alias: Option<&str>,
        sb: SeqBuilder,
    ) {
        // Note: OnceCell nullable cache will be computed on first access

        // 1) the main rule
        let name_smol = EarleyStr::new(name);
        let main_rule = Rule::new(sb.factors);
        let branching_rule = self
            .definitions
            .entry(name_smol.clone())
            .or_insert_with(|| {
                self.defn_order.push(name_smol.clone());
                BranchingRule::new(mark)
            });
        if let Some(alias) = alias {
            branching_rule.alias = Some(EarleyStr::new(alias));
        }
        branching_rule.add_alt_branch(main_rule);

        // 2) synthesized rules — drain by insertion order to avoid cloning
        let mut syn_rules = sb.syn_rules;
        for syn_name in sb.defn_order {
            for builder in syn_rules.remove(&syn_name).unwrap_or_default() {
                let syn_branching_rule =
                    self.definitions.entry(syn_name.clone()).or_insert_with(|| {
                        self.defn_order.push(syn_name.clone());
                        BranchingRule::new(Mark::Mute)
                    });
                syn_branching_rule.add_alt_branch(Rule::new(builder.factors));
            }
        }
    }

    pub fn get_root_definition_name(&self) -> Option<String> {
        self.defn_order.first().map(EarleyStr::to_string)
    }

    pub fn get_root_definition(&self) -> Result<Option<&BranchingRule>, crate::parser::ParseError> {
        match self.defn_order.first() {
            Some(s) => Ok(Some(self.get_definition(s)?)),
            None => Ok(None),
        }
    }

    pub fn get_definition_mark(&self, name: &str) -> Result<Mark, crate::parser::ParseError> {
        if !self.definitions.contains_key(name) {
            return Err(crate::parser::ParseError::static_err(&format!(
                "missing rule named {name}"
            )));
        }
        Ok(self.definitions[name].mark)
    }

    pub fn get_definition_alias(
        &self,
        name: &str,
    ) -> Result<Option<EarleyStr>, crate::parser::ParseError> {
        if !self.definitions.contains_key(name) {
            return Err(crate::parser::ParseError::static_err(&format!(
                "missing rule named {name}"
            )));
        }
        Ok(self.definitions[name].alias.clone())
    }

    pub fn get_definition(&self, name: &str) -> Result<&BranchingRule, crate::parser::ParseError> {
        if !self.definitions.contains_key(name) {
            return Err(crate::parser::ParseError::static_err(&format!(
                "missing rule definition for {name}"
            )));
        }
        Ok(&self.definitions[name])
    }

    pub fn is_nullable(&self, name: &str) -> Result<bool, crate::parser::ParseError> {
        // Ensure nullability cache is computed
        self.ensure_nullability_cache()?;

        let cache = self
            .nullability_cache
            .get()
            .expect("Cache should be initialized");
        let key = NullabilityKey::BranchingRule(EarleyStr::new(name));

        Ok(cache.get(&key).copied().unwrap_or(false))
    }

    /// Check if a specific rule (alternative) is nullable
    /// Uses cached nullability computation for optimal performance
    pub fn is_alternative_nullable(&self, rule: &Rule) -> Result<bool, crate::parser::ParseError> {
        // Ensure nullability cache is computed
        self.ensure_nullability_cache()?;

        let cache = self
            .nullability_cache
            .get()
            .expect("Cache should be initialized");

        // For backwards compatibility, we compute on the fly from the rule structure
        // This is still more efficient than before since the nullable set is cached
        self.is_rule_nullable_from_cache(rule, cache)
    }

    /// Check if a specific alternative (by name and index) is nullable
    /// This is the optimized version that uses pre-computed cache
    pub fn is_alternative_nullable_by_index(
        &self,
        rule_name: &str,
        alt_index: usize,
    ) -> Result<bool, crate::parser::ParseError> {
        // Ensure nullability cache is computed
        self.ensure_nullability_cache()?;

        let cache = self
            .nullability_cache
            .get()
            .expect("Cache should be initialized");
        let key = NullabilityKey::Alternative(EarleyStr::new(rule_name), alt_index);

        Ok(cache.get(&key).copied().unwrap_or(false))
    }

    /// Ensure the nullability cache is computed
    fn ensure_nullability_cache(&self) -> Result<(), crate::parser::ParseError> {
        if self.nullability_cache.get().is_none() {
            let mut temp_grammar = self.clone();
            let cache = temp_grammar.compute_nullability_internal()?;
            // Try to set the cache, but ignore errors if another thread set it first
            let _ = self.nullability_cache.set(cache);
        }
        Ok(())
    }

    /// Compute nullability using fixed point algorithm
    /// Populates both BranchingRule and individual alternative nullability
    fn compute_nullability_internal(
        &mut self,
    ) -> Result<HashMap<NullabilityKey, bool>, crate::parser::ParseError> {
        let mut cache = HashMap::new();
        let mut changed = true;

        // First pass: Initialize all entries to false
        for rule_name in &self.defn_order {
            let branching_key = NullabilityKey::BranchingRule(rule_name.clone());
            cache.insert(branching_key, false);

            let branching_rule = &self.definitions[rule_name];
            for alt_index in 0..branching_rule.alts.len() {
                let alt_key = NullabilityKey::Alternative(rule_name.clone(), alt_index);
                cache.insert(alt_key, false);
            }
        }

        // Fixed point iteration
        while changed {
            changed = false;

            for rule_name in &self.defn_order.clone() {
                let branching_rule = &self.definitions[rule_name];

                // Check each alternative and update its nullability
                let mut any_alternative_nullable = false;
                for (alt_index, rule) in branching_rule.iter().enumerate() {
                    let alt_key = NullabilityKey::Alternative(rule_name.clone(), alt_index);

                    // Compute nullability for this alternative
                    let is_nullable = self.is_rule_nullable_with_cache(rule, &cache)?;

                    // Update the cache if the value changed
                    let old_value = cache[&alt_key];
                    if old_value != is_nullable {
                        cache.insert(alt_key, is_nullable);
                        changed = true;
                    }

                    if is_nullable {
                        any_alternative_nullable = true;
                    }
                }

                // Update BranchingRule nullability
                let branching_key = NullabilityKey::BranchingRule(rule_name.clone());
                let old_branching_value = cache[&branching_key];
                if old_branching_value != any_alternative_nullable {
                    cache.insert(branching_key, any_alternative_nullable);
                    changed = true;
                }
            }
        }

        Ok(cache)
    }

    /// Check if a specific rule (sequence of factors) is nullable using the unified cache
    fn is_rule_nullable_from_cache(
        &self,
        rule: &Rule,
        cache: &HashMap<NullabilityKey, bool>,
    ) -> Result<bool, crate::parser::ParseError> {
        // Empty rule is nullable
        if rule.factors.is_empty() {
            return Ok(true);
        }

        // All factors must be nullable for the rule to be nullable
        for factor in &rule.factors {
            match factor {
                Factor::Terminal(_, _) => {
                    // Terminals are never nullable
                    return Ok(false);
                }
                Factor::Nonterm(_, name, _) => {
                    // Check if this nonterminal is nullable using the cache
                    let key = NullabilityKey::BranchingRule(name.clone());
                    if !cache.get(&key).copied().unwrap_or(false) {
                        return Ok(false);
                    }
                }
                Factor::Insertion(_, _) => {
                    // Insertions are always nullable (don't consume input)
                    // Continue checking other factors
                }
            }
        }

        Ok(true)
    }

    /// Check if a specific rule (sequence of factors) is nullable using current cache during computation
    fn is_rule_nullable_with_cache(
        &self,
        rule: &Rule,
        cache: &HashMap<NullabilityKey, bool>,
    ) -> Result<bool, crate::parser::ParseError> {
        // Empty rule is nullable
        if rule.factors.is_empty() {
            return Ok(true);
        }

        // All factors must be nullable for the rule to be nullable
        for factor in &rule.factors {
            match factor {
                Factor::Terminal(_, _) => {
                    // Terminals are never nullable
                    return Ok(false);
                }
                Factor::Nonterm(_, name, _) => {
                    // Check if this nonterminal is nullable using the cache
                    let key = NullabilityKey::BranchingRule(name.clone());
                    if !cache.get(&key).copied().unwrap_or(false) {
                        return Ok(false);
                    }
                }
                Factor::Insertion(_, _) => {
                    // Insertions are always nullable (don't consume input)
                    // Continue checking other factors
                }
            }
        }

        Ok(true)
    }

    /// Parse an iXML grammar string and construct a Grammar (legacy method)
    pub fn from_ixml_str(ixml: &str) -> Result<Grammar, crate::parser::ParseError> {
        Self::from_ixml_str_with_stats(ixml, true, false)
    }

    /// Parse an iXML grammar string without printing bootstrap parser statistics.
    pub fn from_ixml_str_quiet(ixml: &str) -> Result<Grammar, crate::parser::ParseError> {
        Self::from_ixml_str_with_stats(ixml, false, false)
    }

    /// Parse an iXML grammar string, printing the per-phase wall-time breakdown of the
    /// bootstrap parse (the grammar "build"). Used by the CLI `--stats` flag.
    pub fn from_ixml_str_profiled(ixml: &str) -> Result<Grammar, crate::parser::ParseError> {
        Self::from_ixml_str_with_stats(ixml, true, true)
    }

    fn from_ixml_str_with_stats(
        ixml: &str,
        stats_enabled: bool,
        phase_report: bool,
    ) -> Result<Grammar, crate::parser::ParseError> {
        // Flatten the phased construction error into a ParseError, preserving the
        // structured spec code where one exists (BootstrapParseError and
        // ConversionError already hold a ParseError; only validator messages,
        // which carry no spec code, are wrapped as a code-less static error).
        match Self::from_ixml_str_detailed_with_stats(ixml, stats_enabled, phase_report) {
            Ok(grammar) => Ok(grammar),
            Err(err) => Err(match err {
                GrammarConstructionError::BootstrapParseError(pe) => pe,
                GrammarConstructionError::ConversionError(pe) => pe,
                GrammarConstructionError::ValidationError(msg) => {
                    crate::parser::ParseError::static_err(&msg)
                }
            }),
        }
    }

    /// Parse an iXML grammar string with detailed error categorization
    pub fn from_ixml_str_detailed(ixml: &str) -> Result<Grammar, GrammarConstructionError> {
        Self::from_ixml_str_detailed_with_stats(ixml, true, false)
    }

    fn from_ixml_str_detailed_with_stats(
        ixml: &str,
        stats_enabled: bool,
        phase_report: bool,
    ) -> Result<Grammar, GrammarConstructionError> {
        // Phase 1: Validate and preprocess the iXML text
        // E003: strip UTF-8 BOM (U+FEFF) before any other processing
        let ixml = ixml.trim_start_matches('\u{FEFF}');
        let validation_result = crate::validator::validate_ixml(ixml.trim());

        if !validation_result.is_valid() {
            let error_msgs: Vec<String> = validation_result
                .errors
                .iter()
                .map(|e| e.to_string())
                .collect();
            return Err(GrammarConstructionError::ValidationError(
                error_msgs.join("; "),
            ));
        }

        // Phase 2: Parse the validated and preprocessed text
        let mut ixml_parser = Parser::new(bootstrap_ixml_grammar());
        ixml_parser.set_stats_enabled(stats_enabled);
        ixml_parser.set_phase_report(phase_report);
        let ixml_arena = match ixml_parser.parse_to_arena(&validation_result.processed_text) {
            Ok(arena) => arena,
            Err(parse_error) => {
                return Err(GrammarConstructionError::BootstrapParseError(parse_error))
            }
        };

        // Phase 3: Convert parse tree to Grammar
        let grammar = match Grammar::from_parse_tree(&ixml_arena) {
            Ok(g) => g,
            Err(conversion_error) => {
                return Err(GrammarConstructionError::ConversionError(conversion_error));
            }
        };

        Ok(grammar)
    }

    /// Convert a parse tree (Arena<Content>) from iXML parsing into a Grammar
    pub(crate) fn from_parse_tree(
        arena: &Arena<crate::parser::Content>,
    ) -> Result<Grammar, crate::parser::ParseError> {
        use crate::parser::{Content, Parser};

        let mut g = Grammar::new();

        let root_node = arena.iter().next().unwrap(); // first item == root
        let root_id = arena.get_node_id(root_node).unwrap();

        // Debug: Show root element info
        let root_content = arena.get(root_id).unwrap().get();
        match root_content {
            Content::Element(root_name) => {
                debug_grammar!(
                    DebugLevel::Basic,
                    "GRAMMAR|phase=tree_start|root_element={}|node_count={}",
                    root_name,
                    arena.count()
                );
            }
            other => {
                debug_grammar!(
                    DebugLevel::Basic,
                    "GRAMMAR|phase=tree_start|root_content={:?}|node_count={}",
                    other,
                    arena.count()
                );
            }
        }

        // first a pass over everything, making some indexes as we go
        let mut all_rules: Vec<NodeId> = Vec::new();
        let mut element_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        // Debug: Track all elements we encounter
        for nid in root_id.descendants(arena) {
            let content = arena.get(nid).unwrap().get();
            #[allow(clippy::single_match)] // kept as match; more arms are likely here
            match content {
                Content::Element(name) => {
                    *element_counts.entry(name.clone()).or_insert(0) += 1;
                    if name == "rule" {
                        all_rules.push(nid);
                        let attrs = Parser::get_attributes(arena, nid);
                        let naming = Grammar::naming_from_node(arena, nid);
                        let rule_name = attrs
                            .get("name")
                            .or(naming.name.as_ref())
                            .map(|s| s.as_str())
                            .unwrap_or("UNNAMED");
                        debug_grammar!(
                            DebugLevel::Detailed,
                            "GRAMMAR|phase=rule_found|name={}|node_id={:?}",
                            rule_name,
                            nid
                        );
                    }
                }
                _ => {} // Skip text/other content for now
            }
        }

        // Debug: Show summary of all elements found
        for (element_name, count) in &element_counts {
            debug_grammar!(
                DebugLevel::Basic,
                "GRAMMAR|phase=tree_summary|element={}|count={}",
                element_name,
                count
            );
        }

        // Extract version declaration from prolog if present
        for nid in root_id.descendants(arena) {
            if let Content::Element(name) = arena.get(nid).unwrap().get() {
                if name == "version" {
                    // Version text is stored in the "string" attribute of the version element
                    let attrs = Parser::get_attributes(arena, nid);
                    if let Some(version_text) = attrs.get("string") {
                        g.declared_version = Some(EarleyStr::new(version_text));
                        debug_grammar!(
                            DebugLevel::Basic,
                            "GRAMMAR|phase=version_extraction|declared_version={}",
                            version_text
                        );
                    }
                    break;
                }
            }
        }

        use crate::debug::DebugLevel;
        debug_grammar!(
            DebugLevel::Basic,
            "Converting ixml tree to grammar: found {} rules",
            all_rules.len()
        );

        if all_rules.is_empty() {
            debug_grammar!(
                DebugLevel::Basic,
                "GRAMMAR|phase=error|error=no_rules_found|total_elements={}",
                element_counts.values().sum::<usize>()
            );
            debug_grammar!(DebugLevel::Basic, "ERROR: No rules found in ixml tree");
            return Err(crate::parser::ParseError::static_err(
                "can't convert ixml tree to grammar: no rules present",
            ));
        }

        // S03: track top-level rule names to detect duplicates
        let mut seen_rule_names: std::collections::HashSet<EarleyStr> =
            std::collections::HashSet::new();

        for rule in all_rules {
            let rule_attrs = Parser::get_attributes(arena, rule);
            let naming = Grammar::naming_from_node(arena, rule);
            let rule_name = match rule_attrs.get("name").or(naming.name.as_ref()) {
                Some(name) => name,
                None => {
                    debug_grammar!(DebugLevel::Basic, "ERROR: Rule is missing a name attribute");
                    return Err(crate::parser::ParseError::static_err(
                        "Rule is missing a name attribute",
                    ));
                }
            };

            // S03: duplicate rule definition
            let rule_name_smol = EarleyStr::new(rule_name.as_str());
            if !seen_rule_names.insert(rule_name_smol) {
                return Err(crate::parser::ParseError::coded(
                    crate::parser::ErrorCode::S03,
                    format!("grammar contains more than one rule for nonterminal '{rule_name}'"),
                ));
            }

            let rule_mark = rule_attrs.get("mark").or(naming.mark.as_ref());
            let mark = match rule_mark.map(|s| s.as_str()) {
                Some("@") => Mark::Attr,
                Some("-") => Mark::Mute,
                Some("^") => Mark::Unmute,
                _ => Mark::Default,
            };
            Grammar::construct_rule_from_tree(
                rule,
                mark,
                naming.alias.as_deref(),
                arena,
                rule_name,
                &mut g,
            )?;
        }

        // S02: every nonterminal reference must have a corresponding rule definition
        for br in g.definitions.values() {
            for alt in br.iter() {
                for factor in alt.iter() {
                    if let Factor::Nonterm(_, name, _) = factor {
                        if !g.definitions.contains_key(name.as_str()) {
                            return Err(crate::parser::ParseError::coded(
                                crate::parser::ErrorCode::S02,
                                format!(
                                    "nonterminal '{name}' is used but not defined in the grammar"
                                ),
                            ));
                        }
                    }
                }
            }
        }

        Ok(g)
    }

    fn naming_from_node(arena: &Arena<crate::parser::Content>, node: NodeId) -> NamingAttributes {
        use crate::parser::Parser;

        let mut attrs = Parser::get_attributes(arena, node);
        if let Some(naming_node) = Parser::get_child_elements_named(arena, node, "naming").first() {
            for (key, value) in Parser::get_attributes(arena, *naming_node) {
                attrs.entry(key).or_insert(value);
            }
        }

        NamingAttributes {
            mark: attrs.remove("mark"),
            name: attrs.remove("name"),
            alias: attrs.remove("alias"),
        }
    }

    /// Helper function: Fully construct one rule from parse tree
    fn construct_rule_from_tree(
        rule: NodeId,
        mark: Mark,
        alias: Option<&str>,
        arena: &Arena<crate::parser::Content>,
        rule_name: &str,
        g: &mut Grammar,
    ) -> Result<(), crate::parser::ParseError> {
        use crate::debug::DebugLevel;
        use crate::parser::Parser;

        debug_grammar!(
            DebugLevel::Detailed,
            "Constructing rule '{}' with mark {:?}",
            rule_name,
            mark
        );

        let ctx = RuleContext::new(rule_name);
        let mut alt_count = 0;
        for (name, eid) in Parser::get_child_elements(arena, rule) {
            if name == "alt" {
                alt_count += 1;
                debug_grammar!(
                    DebugLevel::Trace,
                    "  Processing alt {} for rule '{}'",
                    alt_count,
                    rule_name
                );
                let rb = Grammar::build_sequence_from_tree(eid, arena, &ctx)?;
                g.mark_define_alias(mark, rule_name, alias, rb);
            }
        }
        debug_grammar!(
            DebugLevel::Detailed,
            "Completed rule '{}' with {} alternatives",
            rule_name,
            alt_count
        );
        Ok(())
    }

    /// Helper function: Construct a sequence from parse tree node
    fn build_sequence_from_tree(
        node: NodeId,
        arena: &Arena<crate::parser::Content>,
        ctx: &Rc<RuleContext>,
    ) -> Result<SeqBuilder, crate::parser::ParseError> {
        use crate::debug::DebugLevel;
        use crate::parser::Parser;

        debug_grammar!(
            DebugLevel::Trace,
            "Building sequence for rule '{}'",
            ctx.rulename
        );
        let mut seq = ctx.seq();
        let mut factor_count = 0;
        for (name, nid) in Parser::get_child_elements(arena, node) {
            factor_count += 1;
            debug_grammar!(
                DebugLevel::Trace,
                "  Processing factor {} '{}' in rule '{}'",
                factor_count,
                name,
                ctx.rulename
            );
            seq = Grammar::append_factor_from_tree(seq, &name, nid, arena, ctx)?;
        }
        debug_grammar!(
            DebugLevel::Trace,
            "Completed sequence for rule '{}' with {} factors",
            ctx.rulename,
            factor_count
        );
        Ok(seq)
    }

    /// Helper function: Add factors to sequence from parse tree
    fn append_factor_from_tree(
        mut seq: SeqBuilder,
        name: &str,
        nid: NodeId,
        arena: &Arena<crate::parser::Content>,
        ctx: &Rc<RuleContext>,
    ) -> Result<SeqBuilder, crate::parser::ParseError> {
        use crate::debug::DebugLevel;
        use crate::parser::Parser;

        debug_grammar!(
            DebugLevel::Trace,
            "    Appending factor '{}' to rule '{}'",
            name,
            ctx.rulename
        );
        let attrs = Parser::get_attributes(arena, nid);
        match name {
            "alts" => {
                let alt_elements = Parser::get_child_elements_named(arena, nid, "alt");
                if alt_elements.len() == 1 {
                    seq =
                        Grammar::append_factor_from_tree(seq, "alt", alt_elements[0], arena, ctx)?;
                } else {
                    let altrules: Vec<SeqBuilder> = alt_elements
                        .iter()
                        .map(|n| Grammar::build_sequence_from_tree(*n, arena, ctx))
                        .collect::<Result<_, _>>()?;
                    seq = seq.alts(altrules);
                }
            }
            "alt" => {
                for (child_name, child_nid) in Parser::get_child_elements(arena, nid) {
                    seq =
                        Grammar::append_factor_from_tree(seq, &child_name, child_nid, arena, ctx)?;
                }
            }
            "literal" => {
                let tmark_attr = attrs
                    .get("tmark")
                    .or_else(|| attrs.get("mark"))
                    .map(|s| s.as_str());
                let tmark = match tmark_attr {
                    Some("^") => TMark::Unmute,
                    Some("-") => TMark::Mute,
                    _ => TMark::Default,
                };
                debug_grammar!(
                    DebugLevel::Trace,
                    "      Processing literal with attrs: {:?}",
                    attrs
                );
                if let Some(string_value) = attrs.get("string") {
                    if !string_value.is_empty() {
                        seq = seq.mark_str(string_value, tmark);
                    }
                } else if let Some(hex_value) = attrs.get("hex") {
                    let code_point = u32::from_str_radix(hex_value, 16).map_err(|_| {
                        crate::parser::ParseError::coded(
                            crate::parser::ErrorCode::S06,
                            format!("invalid hexadecimal value '#{hex_value}'"),
                        )
                    })?;
                    let ch = Self::validate_hex_codepoint(code_point, hex_value)?;
                    seq = seq.mark_ch(ch, tmark);
                }
            }
            "inclusion" => {
                let tmark = match attrs.get("tmark").map(|s| s.as_str()) {
                    Some("^") => TMark::Unmute,
                    Some("-") => TMark::Mute,
                    _ => TMark::Default,
                };
                if let Some(string_attr) = attrs.get("string") {
                    seq = seq.mark_ch_in(string_attr, tmark);
                } else {
                    let mut lit_builder = TerminalDefn::union();
                    for (child_name, child_nid) in Parser::get_child_elements(arena, nid) {
                        if child_name == "member" {
                            let member_attrs = Parser::get_attributes(arena, child_nid);
                            if let (Some(from), Some(to)) =
                                (member_attrs.get("from"), member_attrs.get("to"))
                            {
                                let (from_char, to_char) = Self::parse_range_values(from, to)?;
                                lit_builder = lit_builder.ch_range(from_char, to_char);
                            } else if let Some(string_attr) = member_attrs.get("string") {
                                lit_builder = lit_builder.ch_in(string_attr);
                            } else {
                                lit_builder =
                                    Self::process_member_element(arena, child_nid, lit_builder)?;
                            }
                        }
                    }
                    seq = seq.mark_lit(lit_builder, tmark);
                }
            }
            "exclusion" => {
                let tmark = match attrs.get("tmark").map(|s| s.as_str()) {
                    Some("^") => TMark::Unmute,
                    Some("-") => TMark::Mute,
                    _ => TMark::Default,
                };
                if let Some(string_attr) = attrs.get("string") {
                    seq = seq.mark_lit(TerminalDefn::union().exclude().ch_in(string_attr), tmark);
                } else {
                    let mut lit_builder = TerminalDefn::union().exclude();
                    for (child_name, child_nid) in Parser::get_child_elements(arena, nid) {
                        if child_name == "member" {
                            let member_attrs = Parser::get_attributes(arena, child_nid);
                            if let (Some(from), Some(to)) =
                                (member_attrs.get("from"), member_attrs.get("to"))
                            {
                                let (from_char, to_char) = Self::parse_range_values(from, to)?;
                                lit_builder = lit_builder.ch_range(from_char, to_char);
                            } else if let Some(string_attr) = member_attrs.get("string") {
                                lit_builder = lit_builder.ch_in(string_attr);
                            } else {
                                lit_builder =
                                    Self::process_member_element(arena, child_nid, lit_builder)?;
                            }
                        }
                    }
                    seq = seq.mark_lit(lit_builder, tmark);
                }
            }
            "nonterminal" => {
                let naming = Grammar::naming_from_node(arena, nid);
                let mark = match attrs
                    .get("mark")
                    .or(naming.mark.as_ref())
                    .map(|s| s.as_str())
                {
                    Some("@") => Mark::Attr,
                    Some("-") => Mark::Mute,
                    Some("^") => Mark::Unmute,
                    _ => Mark::Default,
                };
                let nt_name = attrs
                    .get("name")
                    .or(naming.name.as_ref())
                    .expect("nonterminal must have a name");
                seq = seq.mark_nt_alias(nt_name, mark, naming.alias.as_deref());
            }
            "option" => {
                let subexpr = Grammar::build_sequence_from_tree(nid, arena, ctx)?;
                seq = seq.opt(subexpr);
            }
            "repeat0" => {
                let children = Parser::get_child_elements(arena, nid);
                let expr = children
                    .first()
                    .expect("Should always be at least one child here");
                let repeat_this_node = expr.1;
                let mut repeat_this = ctx.seq();
                repeat_this = Grammar::append_factor_from_tree(
                    repeat_this,
                    &expr.0,
                    repeat_this_node,
                    arena,
                    ctx,
                )?;

                if let Some(sep) = children.get(1) {
                    assert_eq!(sep.0, "sep");
                    let separated_by = Grammar::build_sequence_from_tree(sep.1, arena, ctx)?;
                    seq = seq.repeat0_sep(repeat_this, separated_by)
                } else {
                    seq = seq.repeat0(repeat_this);
                }
            }
            "repeat1" => {
                let children = Parser::get_child_elements(arena, nid);
                let expr = children
                    .first()
                    .expect("Should always be at least one child here");
                let repeat_this_node = expr.1;
                let mut repeat_this = ctx.seq();
                repeat_this = Grammar::append_factor_from_tree(
                    repeat_this,
                    &expr.0,
                    repeat_this_node,
                    arena,
                    ctx,
                )?;

                if let Some(sep) = children.get(1) {
                    assert_eq!(sep.0, "sep");
                    let separated_by = Grammar::build_sequence_from_tree(sep.1, arena, ctx)?;
                    seq = seq.repeat1_sep(repeat_this, separated_by)
                } else {
                    seq = seq.repeat1(repeat_this);
                }
            }
            "insertion" => {
                let tmark = match attrs.get("tmark").map(|s| s.as_str()) {
                    Some("^") => TMark::Unmute,
                    Some("-") => TMark::Mute,
                    _ => TMark::Default,
                };
                let text = if let Some(string_val) = attrs.get("string") {
                    string_val.to_string()
                } else if let Some(hex_val) = attrs.get("hex") {
                    let code_point = u32::from_str_radix(hex_val, 16).map_err(|_| {
                        crate::parser::ParseError::coded(
                            crate::parser::ErrorCode::S06,
                            format!("invalid hexadecimal value '#{hex_val}' in insertion"),
                        )
                    })?;
                    Self::validate_hex_codepoint(code_point, hex_val)?.to_string()
                } else {
                    return Err(crate::parser::ParseError::static_err(
                        "insertion must have either string or hex attribute",
                    ));
                };
                seq = seq.insertion(text, tmark);
            }
            _ => unimplemented!("unknown element {name} child of <alt>"),
        }
        Ok(seq)
    }

    /// Process hex members, class members, and child elements for character sets
    fn process_member_element(
        arena: &Arena<crate::parser::Content>,
        child_nid: NodeId,
        mut lit_builder: LitBuilder,
    ) -> Result<LitBuilder, crate::parser::ParseError> {
        use crate::parser::Parser;
        let member_attrs = Parser::get_attributes(arena, child_nid);

        if let Some(hex_attr) = member_attrs.get("hex") {
            return Self::process_hex_member(hex_attr, lit_builder);
        }

        if let Some(class_attr) = member_attrs.get("class") {
            return Self::process_class_member(class_attr, lit_builder);
        }
        if let Some(code_attr) = member_attrs.get("code") {
            return Self::process_class_member(code_attr, lit_builder);
        }

        for (child_elem_name, child_elem_nid) in Parser::get_child_elements(arena, child_nid) {
            match child_elem_name.as_str() {
                "hex" => {
                    if let Some(hex_text) = Self::extract_text_content(arena, child_elem_nid) {
                        lit_builder = Self::process_hex_member(&hex_text, lit_builder)?;
                    }
                }
                "class" => {
                    if let Some(class_text) = Self::extract_text_content(arena, child_elem_nid) {
                        lit_builder = Self::process_class_member(&class_text, lit_builder)?;
                    }
                }
                _ => {}
            }
        }

        Ok(lit_builder)
    }

    /// Process hex member like #41
    /// S07/S08: validate a raw code-point number and convert to char
    fn validate_hex_codepoint(
        code: u32,
        hex_attr: &str,
    ) -> Result<char, crate::parser::ParseError> {
        if code > 0x10FFFF {
            return Err(crate::parser::ParseError::coded(
                crate::parser::ErrorCode::S07,
                format!(
                    "hex value #{hex_attr} is outside the Unicode code-point range (0..10FFFF)"
                ),
            ));
        }
        if (0xD800..=0xDFFF).contains(&code) {
            return Err(crate::parser::ParseError::coded(
                crate::parser::ErrorCode::S08,
                format!("hex value #{hex_attr} denotes a Unicode surrogate code point"),
            ));
        }
        if (0xFDD0..=0xFDEF).contains(&code) || (code & 0xFFFF) >= 0xFFFE {
            return Err(crate::parser::ParseError::coded(
                crate::parser::ErrorCode::S08,
                format!("hex value #{hex_attr} denotes a Unicode noncharacter"),
            ));
        }
        Ok(char::from_u32(code).expect("validated above"))
    }

    fn process_hex_member(
        hex_attr: &str,
        lit_builder: LitBuilder,
    ) -> Result<LitBuilder, crate::parser::ParseError> {
        let hex_value = u32::from_str_radix(hex_attr, 16).map_err(|_| {
            crate::parser::ParseError::coded(
                crate::parser::ErrorCode::S06,
                format!("invalid hexadecimal value '#{hex_attr}'"),
            )
        })?;
        let ch = Self::validate_hex_codepoint(hex_value, hex_attr)?;
        Ok(lit_builder.ch(ch))
    }

    /// Process Unicode class member like L, LC, Nd, etc.
    fn process_class_member(
        class_attr: &str,
        lit_builder: LitBuilder,
    ) -> Result<LitBuilder, crate::parser::ParseError> {
        if !UnicodeRange::is_valid(class_attr) {
            return Err(crate::parser::ParseError::coded(
                crate::parser::ErrorCode::S10,
                format!("'{class_attr}' is not a defined Unicode character category"),
            ));
        }
        Ok(lit_builder.ch_unicode(class_attr))
    }

    /// Parse range values that could be characters or hex values
    fn parse_range_values(from: &str, to: &str) -> Result<(char, char), crate::parser::ParseError> {
        let from_char = Self::parse_char_or_hex(from)?;
        let to_char = Self::parse_char_or_hex(to)?;
        if from_char > to_char {
            return Err(crate::parser::ParseError::coded(
                crate::parser::ErrorCode::S09,
                format!("in character range '{from}'-'{to}', first character has greater code point than second"),
            ));
        }
        Ok((from_char, to_char))
    }

    /// Parse a value that could be a character literal or hex value
    fn parse_char_or_hex(value: &str) -> Result<char, crate::parser::ParseError> {
        if let Some(hex_part) = value.strip_prefix('#') {
            if !hex_part.is_empty() {
                let hex_value = u32::from_str_radix(hex_part, 16).map_err(|_| {
                    crate::parser::ParseError::coded(
                        crate::parser::ErrorCode::S06,
                        format!("invalid hexadecimal value '{value}'"),
                    )
                })?;
                return Self::validate_hex_codepoint(hex_value, hex_part);
            }
            // '#' alone is the plain '#' character (codepoint 0x23)
        }

        // Character literal
        value
            .chars()
            .next()
            .ok_or_else(|| crate::parser::ParseError::static_err("empty character value in range"))
    }

    /// Extract text content from an element node
    fn extract_text_content(arena: &Arena<crate::parser::Content>, nid: NodeId) -> Option<String> {
        use crate::parser::Content;
        let mut text = String::new();
        for descendant in nid.descendants(arena) {
            if let Some(node) = arena.get(descendant) {
                if let Content::Text(txt) = node.get() {
                    text.push_str(txt);
                }
            }
        }
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

impl fmt::Display for Grammar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        // iterate in insertion order...
        for name in &self.defn_order {
            let branching_rule = &self.definitions[name];
            out.push_str(&branching_rule.mark.to_string());
            out.push_str(name);
            if let Some(alias) = &branching_rule.alias {
                out.push('>');
                out.push_str(alias);
            }
            out.push_str("= ");
            let rules: Vec<String> = branching_rule
                .alts
                .clone()
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
            out.push_str(&rules.join(" | "));
            out.push_str(".\n");
        }
        write!(f, "{}", out)
    }
}

/// within a `BranchingRule`, iterate through the available Rules (branches)
pub struct RuleIter<'a>(&'a Vec<Rc<Rule>>, usize);

impl<'a> Iterator for RuleIter<'a> {
    type Item = &'a Rc<Rule>;
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
    alias: Option<EarleyStr>,
    /// Each alternative is held behind an `Rc` so the predict path can hand a shared
    /// rule to `DotNotation` (a refcount bump) instead of deep-cloning the whole
    /// `Vec<Factor>` once per predicted alternative — see TODO.txt "Stop cloning
    /// grammar fragments in the hot loop".
    alts: Vec<Rc<Rule>>,
    is_internal: bool,
}

impl BranchingRule {
    pub fn new(mark: Mark) -> Self {
        Self {
            mark,
            alias: None,
            alts: Vec::new(),
            is_internal: false,
        }
    }

    fn add_alt_branch(&mut self, alt: Rule) {
        self.alts.push(Rc::new(alt));
    }

    pub fn iter(&self) -> RuleIter<'_> {
        RuleIter(&self.alts, 0)
    }

    pub fn mark(&self) -> Mark {
        self.mark
    }
}

/// Representation of marks on rules or nonterminal references.
/// These get used often, so the varient names are kept short:
/// `@` for attribute, `-` for hidden, `^` for visible (default).
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum Mark {
    Default,
    Unmute, // ^
    Mute,   // -
    Attr,   // @
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
/// These get used often, so the varient names are kept short:
/// `-` for hidden, `^` for visible (default).
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
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

    pub fn is_empty(&self) -> bool {
        self.factors.is_empty()
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
        let s: String = self
            .factors
            .clone()
            .iter()
            .map(|f| format!("{:?}", f))
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "{}", s)
    }
}

/// At this low level, an individual `Factor` is either a terminal, a nonterminal, or an insertion
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Factor {
    Terminal(TMark, TerminalDefn),
    Nonterm(Mark, EarleyStr, Option<EarleyStr>),
    /// Insertion: text to insert in output without consuming input
    /// Can be marked with TMark for exclusion or hex representation
    Insertion(TMark, EarleyStr),
}

impl Factor {
    /// drain off the matchers from a `LitBuilder`, producing a new `Factor::Terminal`
    fn new_lit(builder: LitBuilder, tmark: TMark) -> Self {
        let is_exclude = builder.lit.is_exclude;
        let mut lit = TerminalDefn::new();
        lit.matchers = builder.lit.matchers;
        lit.is_exclude = is_exclude;
        Self::Terminal(tmark, lit)
    }
}

impl fmt::Display for Factor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Terminal(tmark, lit) => write!(f, "{tmark}{lit}"),
            Self::Nonterm(mark, name, alias) => {
                if let Some(alias) = alias {
                    write!(f, "{mark}{name}>{alias}")
                } else {
                    write!(f, "{mark}{name}")
                }
            }
            Self::Insertion(tmark, text) => write!(f, "{tmark}+\"{text}\""),
        }
    }
}

// #[derive(Debug, Clone, Copy, Eq, PartialEq)]
// struct CharMatchId(usize); // Currently unused

/// A character matcher can be an arbitrarily long set of matchspecs (which are considered logically OR'd)
/// e.g. ["0"-"9" | "?" | #64 | Nd]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TerminalDefn {
    matchers: Vec<CharMatcher>,
    /// negative matchers invert the overall match logic
    /// e.g. ~["0"-"9"]
    is_exclude: bool,
}

impl TerminalDefn {
    fn new() -> Self {
        Self {
            matchers: Vec::new(),
            is_exclude: false,
        }
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

impl fmt::Display for TerminalDefn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s: String = self
            .matchers
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(" | ");
        let prefix = if self.is_exclude { "~" } else { "" };
        write!(f, "{prefix}[{s}]")
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum CharMatcher {
    Exact(char),
    OneOf(EarleyStr),
    Range(char, char),
    UnicodeRange(EarleyStr),
}

impl CharMatcher {
    pub fn accept(&self, test: char) -> bool {
        match self {
            Self::Exact(ch) => *ch == test,
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
    lit: TerminalDefn,
}

impl LitBuilder {
    fn new() -> Self {
        Self {
            lit: TerminalDefn::new(),
        }
    }

    /// accept a single char
    pub fn ch(mut self, ch: char) -> Self {
        let matcher = CharMatcher::Exact(ch);
        self.lit.matchers.push(matcher);
        self
    }

    /// accept a single char out of a list
    pub fn ch_in(mut self, chrs: &str) -> Self {
        let matcher = CharMatcher::OneOf(EarleyStr::new(chrs));
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
        let matcher = CharMatcher::UnicodeRange(EarleyStr::new(range));
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
    pub fn new(rulename: &str) -> Rc<Self> {
        Rc::new(RuleContext {
            rulename: rulename.to_string(),
            next_id: Cell::new(1),
        })
    }

    pub fn seq(self: &Rc<Self>) -> SeqBuilder {
        SeqBuilder::new(self.clone())
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
pub struct SeqBuilder {
    /// the "main" TermList being built here
    factors: Vec<Factor>,

    /// in the course of building a rule, we may end up synthesizing additional rules.
    /// These need to eventually get added into the resulting grammar
    syn_rules: HashMap<EarleyStr, Vec<SeqBuilder>>,
    defn_order: Vec<EarleyStr>,

    context: Rc<RuleContext>,
}

impl SeqBuilder {
    fn new(context: Rc<RuleContext>) -> Self {
        Self {
            factors: Vec::new(),
            syn_rules: HashMap::new(),
            defn_order: Vec::new(),
            context,
        }
    }

    /// Convenience function: accept a single char
    pub fn ch(self, ch: char) -> Self {
        self.mark_ch(ch, TMark::Default)
    }

    /// Convenience function: accept a single char, with specified `TMark`
    pub fn mark_ch(mut self, ch: char, tmark: TMark) -> Self {
        let factor = Factor::new_lit(TerminalDefn::union().ch(ch), tmark);
        self.factors.push(factor);
        self
    }

    /// Convenience function: accept a single char out of a list
    pub fn ch_in(self, chrs: &str) -> Self {
        self.mark_ch_in(chrs, TMark::Default)
    }

    /// Convenience function: accept a single char out of a list, with specified `TMark`
    pub fn mark_ch_in(mut self, chrs: &str, tmark: TMark) -> Self {
        let factor = Factor::new_lit(TerminalDefn::union().ch_in(chrs), tmark);
        self.factors.push(factor);
        self
    }

    /// Convenience function: accept a single character within a range
    pub fn ch_range(self, bot: char, top: char) -> Self {
        self.mark_ch_range(bot, top, TMark::Default)
    }

    /// Convenience function: accept a single character within a range, with specified `TMark`
    pub fn mark_ch_range(mut self, bot: char, top: char, tmark: TMark) -> Self {
        let factor = Factor::new_lit(TerminalDefn::union().ch_range(bot, top), tmark);
        self.factors.push(factor);
        self
    }

    /// Convenience function: accept a single character within a Unicode range
    pub fn ch_unicode(self, name: &str) -> Self {
        self.mark_ch_unicode(name, TMark::Default)
    }

    /// Convenience function: accept a single character within a Unicode range, with specified `TMark`
    pub fn mark_ch_unicode(mut self, name: &str, tmark: TMark) -> Self {
        let factor = Factor::new_lit(TerminalDefn::union().ch_unicode(name), tmark);
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

    /// Convenience function: accept a string literal (expands to sequence of characters)
    pub fn str(self, s: &str) -> Self {
        self.mark_str(s, TMark::Default)
    }

    /// Convenience function: accept a string literal with specified `TMark`
    pub fn mark_str(mut self, s: &str, tmark: TMark) -> Self {
        for ch in s.chars() {
            self = self.mark_ch(ch, tmark);
        }
        self
    }

    /// nonterminal
    pub fn nt(self, name: &str) -> Self {
        self.mark_nt(name, Mark::Default)
    }

    /// nonterminal, with specified Mark
    pub fn mark_nt(mut self, name: &str, mark: Mark) -> Self {
        let term = Factor::Nonterm(mark, EarleyStr::new(name), None);
        self.factors.push(term);
        self
    }

    /// nonterminal, with specified Mark and output alias
    pub fn mark_nt_alias(mut self, name: &str, mark: Mark, alias: Option<&str>) -> Self {
        let term = Factor::Nonterm(mark, EarleyStr::new(name), alias.map(EarleyStr::new));
        self.factors.push(term);
        self
    }

    /// insertion: text to insert in output without consuming input
    pub fn insertion(self, text: String, tmark: TMark) -> Self {
        self.mark_insertion(text, tmark)
    }

    /// insertion with specified TMark
    pub fn mark_insertion(mut self, text: String, tmark: TMark) -> Self {
        let factor = Factor::Insertion(tmark, EarleyStr::new(&text));
        self.factors.push(factor);
        self
    }

    /// record an entirely new (internal, synthesized) named rule
    fn syn_rule(mut self, name: &str, mut rb: Self) -> Self {
        self = self.siphon(&mut rb);
        let vec = self.syn_rules.entry(EarleyStr::new(name)).or_default();
        vec.push(rb);
        let smol_name = EarleyStr::new(name);
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
        for name in sub.defn_order.drain(..) {
            // maintain insertion order
            let rule = sub.syn_rules.remove(&name); //.expect("intenal syn_rules and defn_order out of sync");
            if rule.is_none() {
                debug_grammar!(DebugLevel::Trace, "Missing rule in siphon: {}", name);
                debug_grammar!(DebugLevel::Trace, "defn_order: {:?}", self.defn_order);
                debug_grammar!(DebugLevel::Trace, "syn_rules: {:?}", self.syn_rules);
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
        self = self.syn_rule(f_option, sub);
        self = self.syn_rule(f_option, empty); // empty
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
        self = self.syn_rule(
            f_plus_sep,
            subseq1
                .expr(sub1.clone())
                .repeat0(subseq2.expr(sub2).expr(sub1)),
        );
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
        self = self.syn_rule(f_star_sep, subseq1.opt(subseq2.repeat1_sep(sub1, sub2)));
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
    pub fn alts(mut self, exprs: Vec<SeqBuilder>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    // ---- Grammar PartialEq ------------------------------------------------

    fn sample_grammar() -> Grammar {
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();
        g.define("doc", ctx.seq().nt("a").ch('b'));
        g.define("a", ctx.seq().ch('x'));
        g
    }

    #[test]
    fn grammar_eq_reflexive_and_clone() {
        let g = sample_grammar();
        assert_eq!(g, g.clone());
    }

    #[test]
    fn grammar_eq_ignores_nullability_cache() {
        // THE reason this is a manual impl: equality must not depend on whether
        // the lazily-populated nullability cache has been computed.
        let g1 = sample_grammar();
        let g2 = g1.clone();
        let _ = g2.is_nullable("a").unwrap(); // populates g2's cache only
        assert!(
            g1.nullability_cache.get().is_none(),
            "g1 cache must be empty"
        );
        assert!(
            g2.nullability_cache.get().is_some(),
            "g2 cache must be populated"
        );
        assert_eq!(g1, g2, "differing cache state must not affect equality");
    }

    #[test]
    fn grammar_eq_detects_rule_difference() {
        let g1 = sample_grammar();
        let ctx = RuleContext::new("test");
        let mut g2 = Grammar::new();
        g2.define("doc", ctx.seq().nt("a").ch('b'));
        g2.define("a", ctx.seq().ch('y')); // 'y' vs 'x'
        assert_ne!(g1, g2);
    }

    #[test]
    fn grammar_eq_is_definition_order_sensitive() {
        // Rule order is significant (the first definition is the root), so two
        // grammars with the same rules in a different order are not equal.
        let ctx = RuleContext::new("test");
        let mut g1 = Grammar::new();
        g1.define("doc", ctx.seq().nt("a"));
        g1.define("a", ctx.seq().ch('x'));
        let mut g2 = Grammar::new();
        g2.define("a", ctx.seq().ch('x'));
        g2.define("doc", ctx.seq().nt("a"));
        assert_ne!(g1, g2);
    }

    #[test]
    fn parse_ixml() -> Result<(), crate::parser::ParseError> {
        let g = bootstrap_ixml_grammar();
        println!("{}", &g);
        let ixml: &str = r#"doc = "A", "B"."#;
        //                    012345678901234
        let mut parser = Parser::new(g);
        let arena = parser.parse_to_arena(ixml)?;
        let result = Parser::tree_to_test_format(&arena);
        let expected = r#"<ixml><rule name="doc"><alt><literal string="A"/><literal string="B"/></alt></rule></ixml>"#;
        assert_eq!(result, expected);

        println!("=============");
        let gen_grammar = Grammar::from_parse_tree(&arena)?;
        println!("{gen_grammar}");
        let mut gen_parser = Parser::new(gen_grammar);
        // now do a second pass, with the just-generated grammar
        let input2 = "AB";
        let gen_arena = gen_parser.parse_to_arena(input2)?;
        let result2 = Parser::tree_to_test_format(&gen_arena);
        let expected2 = "<doc>AB</doc>";
        assert_eq!(result2, expected2);
        Ok(())
    }

    #[test]
    fn parse_ixml_tmark_is_tmark_not_mark() -> Result<(), crate::parser::ParseError> {
        // Regression: bootstrap `quoted` rule must use @tmark not @mark.
        // A literal with a terminal mark must produce tmark="..." not mark="...".
        let g = bootstrap_ixml_grammar();
        let ixml: &str = r#"doc = -"A"."#;
        let mut parser = Parser::new(g);
        let arena = parser.parse_to_arena(ixml)?;
        let result = Parser::tree_to_test_format(&arena);
        assert!(
            result.contains("tmark=\"-\""),
            "expected tmark attribute but got: {result}"
        );
        assert!(
            !result.contains("mark=\"-\"") || result.contains("tmark=\"-\""),
            "got mark instead of tmark: {result}"
        );
        Ok(())
    }

    #[test]
    fn test_ixml_str_to_grammar() -> Result<(), crate::parser::ParseError> {
        let ixml: &str = r#"doc = "A", "B"."#;
        let grammar = &Grammar::from_ixml_str(ixml);
        assert!(grammar.is_ok());
        assert_eq!(grammar.as_ref().unwrap().get_rule_count(), 1);
        assert_eq!(
            grammar.as_ref().unwrap().get_root_definition_name(),
            Some(String::from("doc"))
        );
        Ok(())
    }

    #[test]
    fn test_nullability_algorithm() -> Result<(), crate::parser::ParseError> {
        // Build a grammar with various nullability patterns
        let ctx = RuleContext::new("test");

        let mut g = Grammar::new();

        // empty: .  (directly nullable)
        g.define("empty", ctx.seq());

        // terminal: "a".  (not nullable)
        g.define("terminal", ctx.seq().ch('a'));

        // nullable_chain: empty, empty.  (nullable through transitivity)
        g.define("nullable_chain", ctx.seq().nt("empty").nt("empty"));

        // mixed: empty, "a".  (not nullable - contains terminal)
        g.define("mixed", ctx.seq().nt("empty").ch('a'));

        // alternatives: "a" | empty.  (nullable through alternative)
        g.define(
            "alternatives",
            ctx.seq()
                .alts(vec![ctx.seq().ch('a'), ctx.seq().nt("empty")]),
        );

        // complex: nullable_chain, alternatives?.  (nullable if alternatives is nullable)
        // This involves synthesized rules from the ? operator
        g.define(
            "complex",
            ctx.seq()
                .nt("nullable_chain")
                .opt(ctx.seq().nt("alternatives")),
        );

        // Test the nullability
        assert!(g.is_nullable("empty")?);
        assert!(!g.is_nullable("terminal")?);
        assert!(g.is_nullable("nullable_chain")?);
        assert!(!g.is_nullable("mixed")?);
        assert!(g.is_nullable("alternatives")?);
        assert!(g.is_nullable("complex")?);

        // Test that nullability computation is cached
        assert!(g.nullability_cache.get().is_some());

        Ok(())
    }

    #[test]
    fn test_nullability_recursive_patterns() -> Result<(), crate::parser::ParseError> {
        // Test patterns involving *, +, ++, ** operators which generate synthesized rules
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();

        // base: "a".
        g.define("base", ctx.seq().ch('a'));

        // star: base*.  (always nullable due to * operator)
        g.define("star", ctx.seq().repeat0(ctx.seq().nt("base")));

        // plus: base+.  (not nullable - requires at least one base)
        g.define("plus", ctx.seq().repeat1(ctx.seq().nt("base")));

        // empty: .
        g.define("empty", ctx.seq());

        // empty_star: empty*.  (nullable)
        g.define("empty_star", ctx.seq().repeat0(ctx.seq().nt("empty")));

        // Test results
        assert!(!g.is_nullable("base")?);
        assert!(g.is_nullable("star")?); // * is always nullable
        assert!(!g.is_nullable("plus")?); // + requires at least one non-nullable item
        assert!(g.is_nullable("empty")?);
        assert!(g.is_nullable("empty_star")?);

        Ok(())
    }

    #[test]
    fn test_f_option() -> Result<(), crate::parser::ParseError> {
        // Test SeqBuilder::opt() creates synthetic f-option rules correctly
        // f? ⇒ f-option
        // -f-option: f | ().
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();

        // optional: base?.  (should create --test.f-option1 synthetic rule)
        // Define this first so it becomes the root rule
        g.define("optional", ctx.seq().opt(ctx.seq().nt("base")));

        // base: "a".
        g.define("base", ctx.seq().ch('a'));

        // Print the effective grammar after synthetic rule generation
        println!("=== Grammar after opt() synthetic rule generation ===");
        println!("{}", &g);

        // Test that the optional rule is nullable (due to empty alternative)
        assert!(g.is_nullable("optional")?);
        assert!(!g.is_nullable("base")?);

        // Test actual parsing with both epsilon and non-epsilon branches

        // Test 1: epsilon branch (empty input should match optional)
        let mut parser1 = crate::parser::Parser::new(g.clone());
        let arena1 = parser1.parse_to_arena("")?;
        let result1 = crate::parser::Parser::tree_to_test_format(&arena1);
        println!("Empty input parse result: {}", result1);
        assert!(result1.contains("<optional>") || result1.contains("<optional/>"));

        // Test 2: non-epsilon branch (input "a" should match base inside optional)
        let mut parser2 = crate::parser::Parser::new(g);
        let arena2 = parser2.parse_to_arena("a")?;
        let result2 = crate::parser::Parser::tree_to_test_format(&arena2);
        println!("Input 'a' parse result: {}", result2);
        assert!(result2.contains("<optional>") || result2.contains("<optional/>"));
        assert!(result2.contains("a"));

        Ok(())
    }

    #[test]
    fn test_f_star() -> Result<(), crate::parser::ParseError> {
        // Test SeqBuilder::repeat0() creates synthetic f-star rules correctly
        // f* ⇒ f-star
        // f-star: (f, f-star)?.
        // This creates nested synthetic rules: f-star uses opt() which creates f-option rules
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();

        // star: base*.  (should create --test.f-star1 synthetic rule)
        // Define this first so it becomes the root rule
        g.define("star", ctx.seq().repeat0(ctx.seq().nt("base")));

        // base: "a".
        g.define("base", ctx.seq().ch('a'));

        // Print the effective grammar after synthetic rule generation
        println!("=== Grammar after repeat0() synthetic rule generation ===");
        println!("{}", &g);

        // Test that the star rule is nullable (due to * operator)
        assert!(g.is_nullable("star")?);
        assert!(!g.is_nullable("base")?);

        // Test actual parsing with different repetition counts

        // Test 1: zero repetitions (epsilon branch)
        let mut parser1 = crate::parser::Parser::new(g.clone());
        let arena1 = parser1.parse_to_arena("")?;
        let result1 = crate::parser::Parser::tree_to_test_format(&arena1);
        println!("Empty input parse result: {}", result1);
        assert!(result1.contains("<star>") || result1.contains("<star/>"));

        // Test 2: one repetition
        let mut parser2 = crate::parser::Parser::new(g.clone());
        let arena2 = parser2.parse_to_arena("a")?;
        let result2 = crate::parser::Parser::tree_to_test_format(&arena2);
        println!("Input 'a' parse result: {}", result2);
        assert!(result2.contains("<star>") || result2.contains("<star/>"));
        assert!(result2.contains("a"));

        // Test 3: multiple repetitions
        let mut parser3 = crate::parser::Parser::new(g);
        let arena3 = parser3.parse_to_arena("aaa")?;
        let result3 = crate::parser::Parser::tree_to_test_format(&arena3);
        println!("Input 'aaa' parse result: {}", result3);
        assert!(result3.contains("<star>") || result3.contains("<star/>"));
        assert_eq!(result3.matches("<base>").count(), 3);

        Ok(())
    }

    #[test]
    fn test_f_plus() -> Result<(), crate::parser::ParseError> {
        // Test SeqBuilder::repeat1() creates synthetic f-plus rules correctly
        // f+ ⇒ f-plus
        // -f-plus: f, f*.
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();

        // plus: base+.  (should create --test.f-plus1 synthetic rule)
        // Define this first so it becomes the root rule
        g.define("plus", ctx.seq().repeat1(ctx.seq().nt("base")));

        // base: "a".
        g.define("base", ctx.seq().ch('a'));

        // Print the effective grammar after synthetic rule generation
        println!("=== Grammar after repeat1() synthetic rule generation ===");
        println!("{}", &g);

        // Test that the plus rule is NOT nullable (requires at least one)
        assert!(!g.is_nullable("plus")?);
        assert!(!g.is_nullable("base")?);

        // Test actual parsing with different repetition counts

        // Test 1: one repetition (minimum required)
        let mut parser1 = crate::parser::Parser::new(g.clone());
        let arena1 = parser1.parse_to_arena("a")?;
        let result1 = crate::parser::Parser::tree_to_test_format(&arena1);
        println!("Input 'a' parse result: {}", result1);
        assert!(result1.contains("<plus>"));
        assert_eq!(result1.matches("<base>").count(), 1);

        // Test 2: multiple repetitions
        let mut parser2 = crate::parser::Parser::new(g);
        let arena2 = parser2.parse_to_arena("aaa")?;
        let result2 = crate::parser::Parser::tree_to_test_format(&arena2);
        println!("Input 'aaa' parse result: {}", result2);
        assert!(result2.contains("<plus>"));
        assert_eq!(result2.matches("<base>").count(), 3);

        Ok(())
    }

    #[test]
    fn test_f_plus_sep() -> Result<(), crate::parser::ParseError> {
        // Test SeqBuilder::repeat1_sep() creates synthetic f-plus-sep rules correctly
        // f++sep ⇒ f-plus-sep
        // -f-plus-sep: f, (sep, f)*.
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();

        // plus_sep: base++",".  (should create --test.f-plus-sep1 synthetic rule)
        // Define this first so it becomes the root rule
        g.define(
            "plus_sep",
            ctx.seq()
                .repeat1_sep(ctx.seq().nt("base"), ctx.seq().ch(',')),
        );

        // base: "a".
        g.define("base", ctx.seq().ch('a'));

        // Print the effective grammar after synthetic rule generation
        println!("=== Grammar after repeat1_sep() synthetic rule generation ===");
        println!("{}", &g);

        // Test that the plus_sep rule is NOT nullable (requires at least one)
        assert!(!g.is_nullable("plus_sep")?);
        assert!(!g.is_nullable("base")?);

        // Test actual parsing with different repetition counts

        // Test 1: one item (no separator needed)
        let mut parser1 = crate::parser::Parser::new(g.clone());
        let arena1 = parser1.parse_to_arena("a")?;
        let result1 = crate::parser::Parser::tree_to_test_format(&arena1);
        println!("Input 'a' parse result: {}", result1);
        assert!(result1.contains("<plus_sep>"));
        assert_eq!(result1.matches("<base>").count(), 1);

        // Test 2: multiple items with separators
        let mut parser2 = crate::parser::Parser::new(g);
        let arena2 = parser2.parse_to_arena("a,a,a")?;
        let result2 = crate::parser::Parser::tree_to_test_format(&arena2);
        println!("Input 'a,a,a' parse result: {}", result2);
        assert!(result2.contains("<plus_sep>"));
        assert_eq!(result2.matches("<base>").count(), 3);

        Ok(())
    }

    #[test]
    fn test_f_star_sep() -> Result<(), crate::parser::ParseError> {
        // Test SeqBuilder::repeat0_sep() creates synthetic f-star-sep rules correctly
        // f**sep ⇒ f-star-sep
        // -f-star-sep: (f++sep)?.
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();

        // star_sep: base**",".  (should create --test.f-star-sep1 synthetic rule)
        // Define this first so it becomes the root rule
        g.define(
            "star_sep",
            ctx.seq()
                .repeat0_sep(ctx.seq().nt("base"), ctx.seq().ch(',')),
        );

        // base: "a".
        g.define("base", ctx.seq().ch('a'));

        // Print the effective grammar after synthetic rule generation
        println!("=== Grammar after repeat0_sep() synthetic rule generation ===");
        println!("{}", &g);

        // Test that the star_sep rule IS nullable (allows zero)
        assert!(g.is_nullable("star_sep")?);
        assert!(!g.is_nullable("base")?);

        // Test actual parsing with different repetition counts

        // Test 1: zero items (epsilon)
        let mut parser1 = crate::parser::Parser::new(g.clone());
        let arena1 = parser1.parse_to_arena("")?;
        let result1 = crate::parser::Parser::tree_to_test_format(&arena1);
        println!("Empty input parse result: {}", result1);
        assert!(result1.contains("<star_sep>") || result1.contains("<star_sep/>"));

        // Test 2: one item (no separator needed)
        let mut parser2 = crate::parser::Parser::new(g.clone());
        let arena2 = parser2.parse_to_arena("a")?;
        let result2 = crate::parser::Parser::tree_to_test_format(&arena2);
        println!("Input 'a' parse result: {}", result2);
        assert!(result2.contains("<star_sep>") || result2.contains("<star_sep/>"));
        assert_eq!(result2.matches("<base>").count(), 1);

        // Test 3: multiple items with separators
        let mut parser3 = crate::parser::Parser::new(g);
        let arena3 = parser3.parse_to_arena("a,a,a")?;
        let result3 = crate::parser::Parser::tree_to_test_format(&arena3);
        println!("Input 'a,a,a' parse result: {}", result3);
        assert!(result3.contains("<star_sep>") || result3.contains("<star_sep/>"));
        assert_eq!(result3.matches("<base>").count(), 3);

        Ok(())
    }

    #[test]
    fn test_synthetic_rules_nullability() -> Result<(), crate::parser::ParseError> {
        // Comprehensive test to verify nullability computation for all synthetic rule types
        // This is critical for bootstrap grammar parsing where synthetic rules must
        // have correct nullability to trigger proper epsilon completions
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();

        // Create all types of synthetic rules with various nullability scenarios

        // 1. f-option (from opt()) - ALWAYS nullable
        g.define("opt_rule", ctx.seq().opt(ctx.seq().nt("base")));

        // 2. f-star (from repeat0()) - ALWAYS nullable
        g.define("star_rule", ctx.seq().repeat0(ctx.seq().nt("base")));

        // 3. f-plus (from repeat1()) - NOT nullable (requires at least one)
        g.define("plus_rule", ctx.seq().repeat1(ctx.seq().nt("base")));

        // 4. f-star-sep (from repeat0_sep()) - ALWAYS nullable
        g.define(
            "star_sep_rule",
            ctx.seq()
                .repeat0_sep(ctx.seq().nt("base"), ctx.seq().ch(',')),
        );

        // 5. f-plus-sep (from repeat1_sep()) - NOT nullable (requires at least one)
        g.define(
            "plus_sep_rule",
            ctx.seq()
                .repeat1_sep(ctx.seq().nt("base"), ctx.seq().ch(',')),
        );

        // 6. Mix nullable and non-nullable base elements
        g.define("base", ctx.seq().ch('a')); // NOT nullable
        g.define("empty", ctx.seq()); // nullable (epsilon)
        g.define("opt_empty", ctx.seq().opt(ctx.seq())); // nullable (optional epsilon)

        // More complex scenarios with nullable bases
        g.define("star_nullable", ctx.seq().repeat0(ctx.seq().nt("empty")));
        g.define("plus_nullable", ctx.seq().repeat1(ctx.seq().nt("empty")));

        println!("=== Grammar with all synthetic rule types ===");
        println!("{}", &g);

        // Test nullability for all user-defined rules
        assert!(
            !g.is_nullable("base")?,
            "base should NOT be nullable (contains 'a')"
        );
        assert!(
            g.is_nullable("empty")?,
            "empty should be nullable (epsilon rule)"
        );
        assert!(
            g.is_nullable("opt_empty")?,
            "opt_empty should be nullable (optional epsilon)"
        );

        // Test nullability for synthetic rules with non-nullable base
        assert!(
            g.is_nullable("opt_rule")?,
            "opt_rule should be nullable (optional)"
        );
        assert!(
            g.is_nullable("star_rule")?,
            "star_rule should be nullable (zero or more)"
        );
        assert!(
            !g.is_nullable("plus_rule")?,
            "plus_rule should NOT be nullable (one or more)"
        );
        assert!(
            g.is_nullable("star_sep_rule")?,
            "star_sep_rule should be nullable (zero or more)"
        );
        assert!(
            !g.is_nullable("plus_sep_rule")?,
            "plus_sep_rule should NOT be nullable (one or more)"
        );

        // Test nullability for synthetic rules with nullable base
        assert!(
            g.is_nullable("star_nullable")?,
            "star_nullable should be nullable (zero or more of nullable)"
        );
        assert!(
            g.is_nullable("plus_nullable")?,
            "plus_nullable should be nullable (one or more of nullable)"
        );

        // Now examine the actual synthetic rules that were generated
        println!("\n=== Examining synthetic rules directly ===");
        for name in g.definitions.keys() {
            if name.contains("--test.f-") {
                println!(
                    "Synthetic rule: {} -> nullable: {}",
                    name,
                    g.is_nullable(name)?
                );

                // Verify synthetic rules follow expected nullability patterns
                if name.contains("-option") {
                    assert!(
                        g.is_nullable(name)?,
                        "All f-option synthetic rules should be nullable"
                    );
                } else if name.contains("-star") && !name.contains("-plus") {
                    assert!(
                        g.is_nullable(name)?,
                        "All f-star synthetic rules should be nullable"
                    );
                } else if name.contains("-plus") {
                    // f-plus nullability depends on what it's repeating
                    let is_nullable = g.is_nullable(name)?;
                    println!("f-plus rule {} nullability: {}", name, is_nullable);
                    // Don't assert here since f-plus of nullable elements can be nullable
                }
            }
        }

        // Test that epsilon completion would work for the nullable synthetic rules
        // This is the core issue: synthetic rules that are nullable should trigger
        // immediate completion when predicted, matching the "Bpredict/complete" pattern

        // Parse empty input with nullable synthetic rules
        let mut parser = crate::parser::Parser::new(g.clone());
        let arena = parser.parse_to_arena("")?;
        let result = crate::parser::Parser::tree_to_test_format(&arena);
        println!("\nEmpty input parse result: {}", result);

        // Should succeed because opt_rule is nullable
        assert!(
            result.contains("<opt_rule>") || result.contains("<opt_rule/>"),
            "Empty input should parse successfully with nullable opt_rule"
        );

        Ok(())
    }

    #[test]
    fn test_nullability_caching_simple() -> Result<(), crate::parser::ParseError> {
        // Test that the new caching mechanism is more efficient than repeated computation
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();

        // Create a complex grammar with many synthetic rules
        g.define(
            "root",
            ctx.seq()
                .opt(ctx.seq().nt("complex"))
                .repeat0(ctx.seq().nt("complex"))
                .repeat1_sep(ctx.seq().nt("base"), ctx.seq().ch(',')),
        );

        g.define(
            "complex",
            ctx.seq().alts(vec![
                ctx.seq().nt("base").opt(ctx.seq().ch('?')),
                ctx.seq()
                    .repeat0_sep(ctx.seq().nt("base"), ctx.seq().ch(';')),
                ctx.seq(), // epsilon alternative
            ]),
        );

        g.define("base", ctx.seq().ch('a'));

        // Test the new efficient method multiple times - should hit cache
        for _ in 0..10 {
            assert!(!g.is_alternative_nullable_by_index("base", 0)?);

            // Test that we have at least one nullable alternative in complex (epsilon)
            let complex_def = g.get_definition("complex")?;
            let mut found_epsilon = false;
            for idx in 0..complex_def.alts.len() {
                if g.is_alternative_nullable_by_index("complex", idx)? {
                    found_epsilon = true;
                    break;
                }
            }
            assert!(
                found_epsilon,
                "Should have at least one nullable alternative in complex"
            );
        }

        // Verify cache consistency between methods
        for (rule_name, branching_rule) in &g.definitions {
            for (alt_index, rule) in branching_rule.iter().enumerate() {
                let cached_result = g.is_alternative_nullable_by_index(rule_name, alt_index)?;
                let direct_result = g.is_alternative_nullable(rule)?;
                assert_eq!(
                    cached_result, direct_result,
                    "Mismatch for {}[{}]: cached={}, direct={}",
                    rule_name, alt_index, cached_result, direct_result
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_nullability_cache_structure() -> Result<(), crate::parser::ParseError> {
        // Test that the cache contains all expected entries
        let ctx = RuleContext::new("test");
        let mut g = Grammar::new();

        g.define("empty", ctx.seq());
        g.define("terminal", ctx.seq().ch('a'));
        g.define(
            "choice",
            ctx.seq()
                .alts(vec![ctx.seq().nt("empty"), ctx.seq().nt("terminal")]),
        );

        // Trigger cache computation
        g.is_nullable("empty")?;

        // Verify cache structure
        let cache = g
            .nullability_cache
            .get()
            .expect("Cache should be populated");

        // Check that both BranchingRule and Alternative entries exist
        assert!(cache.contains_key(&NullabilityKey::BranchingRule(EarleyStr::new("empty"))));
        assert!(cache.contains_key(&NullabilityKey::Alternative(EarleyStr::new("empty"), 0)));

        assert!(cache.contains_key(&NullabilityKey::BranchingRule(EarleyStr::new("terminal"))));
        assert!(cache.contains_key(&NullabilityKey::Alternative(EarleyStr::new("terminal"), 0)));

        // Check synthetic rule from alts()
        let synthetic_name = cache
            .keys()
            .filter_map(|k| match k {
                NullabilityKey::BranchingRule(name) if name.contains("--test.f-opt") => {
                    Some(name.clone())
                }
                _ => None,
            })
            .next()
            .expect("Should have synthetic rule");

        assert!(cache.contains_key(&NullabilityKey::BranchingRule(synthetic_name.clone())));
        assert!(cache.contains_key(&NullabilityKey::Alternative(synthetic_name.clone(), 0)));
        assert!(cache.contains_key(&NullabilityKey::Alternative(synthetic_name, 1)));

        Ok(())
    }
} // end tests module
