use indextree::{Arena, NodeId};

use crate::{grammar::{Grammar, Rule, Mark, TMark, RuleBuilder, Lit, reset_internal_id}, parser::{Content, Parser, ParseError}};

// TODO: -rules and @rules

/// Bootstrap ixml grammar; hand-coded definition
pub fn ixml_grammar() -> Grammar {
    let mut g = Grammar::new();

    // ixml: s, prolog?, rule++RS, s.
    // TODO: prolog
    g.define("ixml", Rule::seq().nt("s").repeat1_sep(Rule::seq().nt("rule"), Rule::seq().nt("RS")).nt("s"));

    // -s: (whitespace; comment)*. {Optional spacing}
    // TODO: comment
    g.mark_define(Mark::Mute, "s", Rule::seq().repeat0( Rule::seq().nt("whitespace")));

    // -RS: (whitespace; comment)+. {Required spacing}
    // TODO: comment
    g.mark_define(Mark::Mute, "RS", Rule::seq().repeat1( Rule::seq().nt("whitespace")));

    // -whitespace: -[Zs]; tab; lf; cr.
    // TODO: Unicode
    g.mark_define(Mark::Mute, "whitespace", Rule::seq().mark_ch_in(" \u{0009}\u{000a}\u{000d}", TMark::Mute));

    // -tab: -#9.
    // DNO = Deliberately Not Implemented

    // -lf: -#a.
    // DNO = Deliberately Not Implemented

    // -cr: -#d.
    // DNO = Deliberately Not Implemented

    // comment: -"{", (cchar; comment)*, -"}".
    // TODO

    // -cchar: ~["{}"].
    // TODO

    // prolog: version, s.
    // TODO

    // version: -"ixml", RS, -"version", RS, string, s, -'.' .
    // TODO

    // rule: (mark, s)?, name, s, -["=:"], s, -alts, -".".
    g.define("rule", Rule::seq()
        .opt(Rule::seq().nt("mark").nt("s"))
        .nt("name")
        .nt("s")
        .mark_ch_in("=:", TMark::Mute)
        .nt("s")
        .mark_nt("alts", Mark::Mute)
        .mark_ch('.', TMark::Mute) );

    // @mark: ["@^-"].
    g.mark_define(Mark::Attr, "mark", Rule::seq().ch_in("@^-"));

    // alts: alt++(-[";|"], s).
    g.define("alts", Rule::seq().repeat1_sep(
        Rule::seq().nt("alt"),
        Rule::seq().mark_ch_in(";|", TMark::Mute).nt("s") ));

    // alt: term**(-",", s)
    g.define("alt", Rule::seq().repeat0_sep(
        Rule::seq().nt("term"),
        Rule::seq().mark_ch(',', TMark::Mute).nt("s") ));

    // -term: factor; option; repeat0; repeat1.
    g.mark_define(Mark::Mute, "term", Rule::seq().nt("factor"));
    g.mark_define(Mark::Mute, "term", Rule::seq().nt("option"));
    g.mark_define(Mark::Mute, "term", Rule::seq().nt("repeat0"));
    g.mark_define(Mark::Mute, "term", Rule::seq().nt("repeat1"));

    // -factor: terminal; nonterminal; insertion; -"(", s, alts, -")", s.
    // TODO: insertion
    g.mark_define(Mark::Mute, "factor", Rule::seq().nt("terminal"));
    g.mark_define(Mark::Mute, "factor", Rule::seq().nt("nonterminal"));
    g.mark_define(Mark::Mute, "factor", Rule::seq()
        .mark_ch('(', TMark::Mute).nt("s").nt("alts").mark_ch(')', TMark::Mute).nt("s"));

    // repeat0: factor, (-"*", s; -"**", s, sep).
    g.define("repeat0", Rule::seq().nt("factor").mark_ch('*', TMark::Mute).nt("s"));
    g.define("repeat0", Rule::seq()
        .nt("factor").mark_ch('*', TMark::Mute).mark_ch('*', TMark::Mute).nt("s").nt("sep"));

    // repeat1: factor, (-"+", s; -"++", s, sep).
    g.define("repeat1", Rule::seq().nt("factor").mark_ch('+', TMark::Mute).nt("s"));
    g.define("repeat1", Rule::seq()
        .nt("factor").mark_ch('+', TMark::Mute).mark_ch('+', TMark::Mute).nt("s").nt("sep"));

    // option: factor, -"?", s.
    g.define("option", Rule::seq().nt("factor").mark_ch('?', TMark::Mute).nt("s"));

    // sep: factor.
    g.define("sep", Rule::seq().nt("factor"));

    // nonterminal: (mark, s)?, name, s.
    g.define("nonterminal", Rule::seq()
        .opt( Rule::seq().nt("mark").nt("s") )
        .nt("name").nt("s") );

    // @name: namestart, namefollower*.
    // TODO: fixme
    g.mark_define(Mark::Attr, "name", Rule::seq().repeat1( Rule::seq().ch_in("_abcdefghijklmnopqrstuvwxyzRS")));
    
    // -namestart: ["_"; L].
    // TODO

    // -namefollower: namestart; ["-.·‿⁀"; Nd; Mn].
    // TODO

    // -terminal: literal; charset.
    g.mark_define(Mark::Mute, "terminal", Rule::seq().nt("literal"));
    g.mark_define(Mark::Mute, "terminal", Rule::seq().nt("charset"));
    
    // literal: quoted; encoded.
    g.define("literal", Rule::seq().nt("quoted"));
    g.define("literal", Rule::seq().nt("encoded"));

    // -quoted: (tmark, s)?, string, s.
    g.mark_define(Mark::Mute, "quoted", Rule::seq()
        .opt( Rule::seq().nt("mark").nt("s") )
        .nt("string").nt("s"));

    // @tmark: ["^-"].
    g.mark_define(Mark::Attr, "tmark", Rule::seq().ch_in("^-"));

    // @string: -'"', dchar+, -'"'; -"'", schar+, -"'".
    // TODO schar variant
    g.mark_define(Mark::Attr, "string", Rule::seq()
        .mark_ch('"', TMark::Mute)
        .repeat1( Rule::seq().nt("dchar"))
        .mark_ch('"', TMark::Mute) );

    // dchar: ~['"'; #a; #d]; '"', -'"'. {all characters except line breaks; quotes must be doubled}
    // TODO: fixme
    g.define("dchar", Rule::seq().ch_in("abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
  
    // schar: ~["'"; #a; #d]; "'", -"'". {all characters except line breaks; quotes must be doubled}
    // TODO

    // -encoded: (tmark, s)?, -"#", hex, s.
    g.mark_define(Mark::Mute, "encoded", Rule::seq()
        .opt(Rule::seq().nt("tmark").nt("s"))
        .mark_ch('#', TMark::Mute).nt("hex").nt("s"));

    // @hex: ["0"-"9"; "a"-"f"; "A"-"F"]+.
    g.mark_define(Mark::Attr, "hex", Rule::seq()
        .repeat1(Rule::seq().lit(Lit::union()
            .ch_range('0', '9').ch_range('a', 'f').ch_range('A', 'F'))));

    // -charset: inclusion; exclusion.
    g.mark_define(Mark::Mute, "charset", Rule::seq().nt("inclusion"));
    g.mark_define(Mark::Mute, "charset", Rule::seq().nt("exclusion"));

    // inclusion: (tmark, s)?,          set.
    g.define("inclusion", Rule::seq()
        .opt( Rule::seq().nt("tmark").nt("s"))
        .nt("set"));

    // exclusion: (tmark, s)?, -"~", s, set.
    g.define("exclusion", Rule::seq()
        .opt( Rule::seq().nt("tmark").nt("s"))
        .mark_ch('~', TMark::Mute).nt("s")
        .nt("set"));

    // -set: -"[", s,  (member, s)**(-[";|"], s), -"]", s.
    g.mark_define(Mark::Mute, "set", Rule::seq()
        .mark_ch('[', TMark::Mute).nt("s")
        .repeat0_sep(
            Rule::seq().nt("member").nt("s"),
            Rule::seq().mark_ch_in(";|", TMark::Mute).nt("s"))
        .mark_ch(']', TMark::Mute).nt("s"));

    // member: string; -"#", hex; range; class.
    g.define("member", Rule::seq().nt("string"));
    g.define("member", Rule::seq().mark_ch('#', TMark::Mute).nt("hex"));
    g.define("member", Rule::seq().nt("range"));
    g.define("member", Rule::seq().nt("class"));

    // -range: from, s, -"-", s, to.
    g.mark_define(Mark::Mute, "range", Rule::seq().nt("from").nt("s").mark_ch('-', TMark::Mute)
        .nt("s").nt("to"));

    // @from: character.
    g.mark_define(Mark::Attr, "from", Rule::seq().nt("character"));

    // @to: character.
    g.mark_define(Mark::Attr, "to", Rule::seq().nt("character"));

    // -character: -'"', dchar, -'"'; -"'", schar, -"'"; "#", hex.
    // TODO: schar variant
    g.mark_define(Mark::Mute, "character", Rule::seq()
        .mark_ch('"', TMark::Mute).nt("dchar").mark_ch('"', TMark::Mute));
    g.mark_define(Mark::Mute, "character", Rule::seq().ch('#').nt("hex"));

    // -class: code.
    g.mark_define(Mark::Mute, "class", Rule::seq().nt("code"));

    // @code: capital, letter?.
    g.mark_define(Mark::Attr, "code", Rule::seq().nt("capital").opt(Rule::seq().nt("letter")));

    // -capital: ["A"-"Z"].
    g.mark_define(Mark::Mute, "capital", Rule::seq().ch_range('A', 'Z'));

    // -letter: ["a"-"z"].
    g.mark_define(Mark::Mute, "letter", Rule::seq().ch_range('a', 'z'));

    // insertion: -"+", s, (string; -"#", hex), s.
    // TODO

    g
}

/*
    ixml: s, prolog?, rule++RS, s.

    -s: (whitespace; comment)*. {Optional spacing}
    -RS: (whitespace; comment)+. {Required spacing}
-whitespace: -[Zs]; tab; lf; cr.
    -tab: -#9.
    -lf: -#a.
    -cr: -#d.
comment: -"{", (cchar; comment)*, -"}".
-cchar: ~["{}"].

prolog: version, s.
version: -"ixml", RS, -"version", RS, string, s, -'.' .

    rule: (mark, s)?, name, s, -["=:"], s, -alts, -".".
    @mark: ["@^-"].
    alts: alt++(-[";|"], s).
    alt: term**(-",", s).
    -term: factor;
        option;
        repeat0;
        repeat1.
-factor: terminal;
        nonterminal;
        insertion;
        -"(", s, alts, -")", s.
repeat0: factor, (-"*", s; -"**", s, sep).
repeat1: factor, (-"+", s; -"++", s, sep).
option: factor, -"?", s.
    sep: factor.
nonterminal: (mark, s)?, name, s.

    @name: namestart, namefollower*.
-namestart: ["_"; L].
-namefollower: namestart; ["-.·‿⁀"; Nd; Mn].

-terminal: literal; 
        charset.
literal: quoted;
        encoded.
-quoted: (tmark, s)?, string, s.

@tmark: ["^-"].
@string: -'"', dchar+, -'"';
        -"'", schar+, -"'".
    dchar: ~['"'; #a; #d];
        '"', -'"'. {all characters except line breaks; quotes must be doubled}
    schar: ~["'"; #a; #d];
        "'", -"'". {all characters except line breaks; quotes must be doubled}
-encoded: (tmark, s)?, -"#", hex, s.
    @hex: ["0"-"9"; "a"-"f"; "A"-"F"]+.

-charset: inclusion; 
        exclusion.
inclusion: (tmark, s)?,          set.
exclusion: (tmark, s)?, -"~", s, set.
    -set: -"[", s,  (member, s)**(-[";|"], s), -"]", s.
member: string;
        -"#", hex;
        range;
        class.
-range: from, s, -"-", s, to.
    @from: character.
    @to: character.
-character: -'"', dchar, -'"';
        -"'", schar, -"'";
        "#", hex.
-class: code.
    @code: capital, letter?.
-capital: ["A"-"Z"].
-letter: ["a"-"z"].
insertion: -"+", s, (string; -"#", hex), s.
*/

/// one stop shopping for ixml String -> Result<Grammar, ParseError>
pub fn ixml_str_to_grammar(ixml: &str) -> Result<Grammar, ParseError> {
    let mut ixml_parser = Parser::new(ixml_grammar());
    let ixml_arena = ixml_parser.parse(ixml.trim())?;
    reset_internal_id();
    let grammar = ixml_tree_to_grammar(&ixml_arena);
    Ok(grammar)
}

/// Accepts the Arena<Content> resulting from the parse of a valid ixml grammar
/// Produces a new Grammar as output
/// TODO: Result<> return type.
pub fn ixml_tree_to_grammar(arena: &Arena<Content>) -> Grammar {
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
    assert!(all_rules.len() > 0, "can't convert ixml tree to grammar: no rules present! {:?}", &arena);
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
        ixml_construct_rule(rule, mark, arena, rule_name, &mut g);
    }
    g
}

/// Fully construct one rule. (which may involve multiple calls to ixml_rulebuilder if there are multiple alts)
pub fn ixml_construct_rule(rule: NodeId, mark: Mark, arena: &Arena<Content>, rule_name: &str, g: &mut Grammar) {
    //println!("Build rule ... {rule_name}");
    for (name, eid) in Parser::get_child_elements(arena, rule) {
        if name=="alt" {
            let rb = ixml_rulebuilder_new(eid, arena);
            g.mark_define(mark, rule_name, rb);
        }
    }
}

/// Construct one alt, which is a sequence built from a single `RuleBuilder`
/// @param `node` is the nodeID of current element, expected to be <alt>, <repeat0>, <repeat1>, <option>, or <sep>
/// as it only looks at child elements downstream from the `NodeId` passed in
pub fn ixml_rulebuilder_new(node: NodeId, arena: &Arena<Content>) -> RuleBuilder {
    let mut rb = Rule::seq();
    for (name, nid) in Parser::get_child_elements(arena, node) {
        rb = ixml_ruleappend(rb, &name, nid, arena);
    }
    rb
}

/// Add additional factors onto the given `RuleBuilder`, possibly recursively
/// @param `node` is the nodeID of current element, which is diectly processed
pub fn ixml_ruleappend(mut rb: RuleBuilder, name: &str, nid: NodeId, arena: &Arena<Content>) -> RuleBuilder {

    let attrs = Parser::get_attributes(arena, nid);
    match name {
        "alts" => {
            // an <alts> with only one <alt> child can be inlined, otherwise we give it the full treatment
            let alt_elements = Parser::get_child_elements_named(arena, nid, "alt");
            if alt_elements.len()==1 {
                rb = ixml_ruleappend(rb, "alt", alt_elements[0], arena);
            } else {
                let altrules: Vec<RuleBuilder> = alt_elements.iter()
                    .map(|n| ixml_rulebuilder_new(*n, arena))
                    .collect();
                rb = rb.alts(altrules);
            }
        }
        "literal" => {
            rb = rb.ch(attrs["string"].chars().next().expect("no empty string literals"));
        }
        "inclusion" => {
            // character classes
            unimplemented!("need to handle character <inclusion>");
        }
        "exclusion" => {
            // character classes
            unimplemented!("need to handle character <exclusion>");
        }
        "nonterminal" => {
            rb = rb.nt(&attrs["name"]);
        }
        "option" => {
            let subexpr = ixml_rulebuilder_new(nid, arena);
            rb = rb.opt(subexpr);
        }
        "repeat0" => {
            let children = Parser::get_child_elements(arena, nid);
            // assume first child is what-to-repeat (from `factor`)
            let expr = children.get(0).expect("Should always be at least one child here");
            let repeat_this_node = expr.1;
            let mut repeat_this = Rule::seq();
            repeat_this = ixml_ruleappend(repeat_this, &expr.0, repeat_this_node, arena);

            // if a <sep> child exists, this is a ** rule, otherwise just *
            if let Some(sep) = children.get(1) {
                assert_eq!(sep.0, "sep");
                let separated_by = ixml_rulebuilder_new(sep.1, arena);
                rb = rb.repeat0_sep(repeat_this, separated_by)
            } else {
                rb = rb.repeat0(repeat_this);
            }
        }
        "repeat1" => {
            let children = Parser::get_child_elements(arena, nid);
            // assume first child is what-to-repeat (from `factor`)
            let expr = children.get(0).expect("Should always be at least one child here");
            let repeat_this_node = expr.1;
            let mut repeat_this = Rule::seq();
            repeat_this = ixml_ruleappend(repeat_this, &expr.0, repeat_this_node, arena);

            // if a <sep> child exists, this is a ++ rule, otherwise just +
            if let Some(sep) = children.get(1) {
                assert_eq!(sep.0, "sep");
                let separated_by = ixml_rulebuilder_new(sep.1, arena);
                rb = rb.repeat1_sep(repeat_this, separated_by)
            } else {
                rb = rb.repeat1(repeat_this);
            }
        }
        _ => unimplemented!("unknown element {name} child of <alt>"),
    }
    rb
}


#[test]
fn parse_ixml() -> Result<(), ParseError> {
    let g = ixml_grammar();
    println!("{}", &g);
    let ixml: &str = r#"doc = "A", "B"."#;
    //                  012345678901234
    let mut parser = Parser::new(g);
    let arena = parser.parse(ixml)?;
    let result = Parser::tree_to_testfmt(&arena);
    let expected = r#"<ixml><rule name="doc"><alt><literal string="A"></literal><literal string="B"></literal></alt></rule></ixml>"#;
    assert_eq!(result, expected);

    println!("=============");
    let gen_grammar = ixml_tree_to_grammar(&arena);
    println!("{gen_grammar}");
    let mut gen_parser = Parser::new(gen_grammar);
    // now do a second pass, with the just-generated grammar
    let input2 = "AB";
    let gen_arena = gen_parser.parse(input2)?;
    let result2 = Parser::tree_to_testfmt(&gen_arena);
    let expected2 = "<doc>AB</doc>";
    assert_eq!(result2, expected2);
    Ok(())
}

#[test]
fn test_ixml_str_to_grammar() -> Result<(), ParseError> {
    let ixml: &str = r#"doc = "A", "B"."#;
    let grammar = &ixml_str_to_grammar(ixml);
    assert!(grammar.is_ok());
    assert_eq!(grammar.as_ref().unwrap().get_rule_count(), 1);
    assert_eq!(grammar.as_ref().unwrap().get_root_definition_name(), Some(String::from("doc")));
    Ok(())
}
