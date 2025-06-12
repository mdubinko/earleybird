use indextree::{Arena, NodeId};

use crate::{grammar::{Grammar, Mark, TMark, SeqBuilder, Lit, RuleContext}, parser::{Content, Parser, ParseError}};

/// Bootstrap ixml grammar; hand-coded definition
pub fn ixml_grammar() -> Grammar {
    let mut g = Grammar::new();

    // ixml: s, prolog?, rule++RS, s.
    // TODO: prolog
    let ctx = RuleContext::new("ixml");
    g.define("ixml", ctx.seq().nt("s").repeat1_sep(ctx.seq().nt("rule"), ctx.seq().nt("RS")).nt("s"));

    // -s: (whitespace; comment)*. {Optional spacing}
    // TODO: comment
    let ctx = RuleContext::new("s");
    g.mark_define(Mark::Mute, "s", ctx.seq().repeat0(ctx.seq().nt("whitespace")));

    // -RS: (whitespace; comment)+. {Required spacing}
    // TODO: comment
    let ctx = RuleContext::new("RS");
    g.mark_define(Mark::Mute, "RS", ctx.seq().repeat1( ctx.seq().nt("whitespace")));

    // -whitespace: -[Zs]; tab; lf; cr.
    // TODO: Unicode
    let ctx = RuleContext::new("whitespace");
    g.mark_define(Mark::Mute, "whitespace", ctx.seq().mark_ch_in(" \u{0009}\u{000a}\u{000d}", TMark::Mute));

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
    let ctx = RuleContext::new("rule");
    g.define("rule", ctx.seq()
        .opt(ctx.seq().nt("mark").nt("s"))
        .nt("name")
        .nt("s")
        .mark_ch_in("=:", TMark::Mute)
        .nt("s")
        .mark_nt("alts", Mark::Mute)
        .mark_ch('.', TMark::Mute) );

    // @mark: ["@^-"].
    let ctx = RuleContext::new("mark");
    g.mark_define(Mark::Attr, "mark", ctx.seq().ch_in("@^-"));

    // alts: alt++(-[";|"], s).
    let ctx = RuleContext::new("alts");
    g.define("alts", ctx.seq().repeat1_sep(
        ctx.seq().nt("alt"),
        ctx.seq().mark_ch_in(";|", TMark::Mute).nt("s") ));

    // alt: term**(-",", s)
    let ctx = RuleContext::new("alt");
    g.define("alt", ctx.seq().repeat0_sep(
        ctx.seq().nt("term"),
        ctx.seq().mark_ch(',', TMark::Mute).nt("s") ));

    // -term: factor; option; repeat0; repeat1.
    let ctx = RuleContext::new("term");
    g.mark_define(Mark::Mute, "term", ctx.seq().nt("factor"));
    g.mark_define(Mark::Mute, "term", ctx.seq().nt("option"));
    g.mark_define(Mark::Mute, "term", ctx.seq().nt("repeat0"));
    g.mark_define(Mark::Mute, "term", ctx.seq().nt("repeat1"));

    // -factor: terminal; nonterminal; insertion; -"(", s, alts, -")", s.
    // TODO: insertion
    let ctx = RuleContext::new("factor");
    g.mark_define(Mark::Mute, "factor", ctx.seq().nt("terminal"));
    g.mark_define(Mark::Mute, "factor", ctx.seq().nt("nonterminal"));
    g.mark_define(Mark::Mute, "factor", ctx.seq()
        .mark_ch('(', TMark::Mute).nt("s").nt("alts").mark_ch(')', TMark::Mute).nt("s"));

    // repeat0: factor, (-"*", s; -"**", s, sep).
    let ctx = RuleContext::new("repeat0");
    g.define("repeat0", ctx.seq().nt("factor").mark_ch('*', TMark::Mute).nt("s"));
    g.define("repeat0", ctx.seq()
        .nt("factor").mark_ch('*', TMark::Mute).mark_ch('*', TMark::Mute).nt("s").nt("sep"));

    // repeat1: factor, (-"+", s; -"++", s, sep).
    let ctx = RuleContext::new("repeat1");
    g.define("repeat1", ctx.seq().nt("factor").mark_ch('+', TMark::Mute).nt("s"));
    g.define("repeat1", ctx.seq()
        .nt("factor").mark_ch('+', TMark::Mute).mark_ch('+', TMark::Mute).nt("s").nt("sep"));

    // option: factor, -"?", s.
    let ctx = RuleContext::new("option");
    g.define("option", ctx.seq().nt("factor").mark_ch('?', TMark::Mute).nt("s"));

    // sep: factor.
    let ctx = RuleContext::new("sep");
    g.define("sep", ctx.seq().nt("factor"));

    // nonterminal: (mark, s)?, name, s.
    let ctx = RuleContext::new("nonterminal");
    g.define("nonterminal", ctx.seq()
        .opt( ctx.seq().nt("mark").nt("s") )
        .nt("name").nt("s") );

    // @name: namestart, namefollower*.
    // TODO: fixme
    let ctx = RuleContext::new("name");
    g.mark_define(Mark::Attr, "name", ctx.seq().repeat1( ctx.seq().ch_in("_abcdefghijklmnopqrstuvwxyzRS")));
    
    // -namestart: ["_"; L].
    // TODO

    // -namefollower: namestart; ["-.·‿⁀"; Nd; Mn].
    // TODO

    // -terminal: literal; charset.
    let ctx = RuleContext::new("terminal");
    g.mark_define(Mark::Mute, "terminal", ctx.seq().nt("literal"));
    g.mark_define(Mark::Mute, "terminal", ctx.seq().nt("charset"));
    
    // literal: quoted; encoded.
    let ctx = RuleContext::new("literal");
    g.define("literal", ctx.seq().nt("quoted"));
    g.define("literal", ctx.seq().nt("encoded"));

    // -quoted: (tmark, s)?, string, s.
    let ctx = RuleContext::new("quoted");
    g.mark_define(Mark::Mute, "quoted", ctx.seq()
        .opt( ctx.seq().nt("mark").nt("s") )
        .nt("string").nt("s"));

    // @tmark: ["^-"].
    let ctx = RuleContext::new("tmark");
    g.mark_define(Mark::Attr, "tmark", ctx.seq().ch_in("^-"));

    // @string: -'"', dchar+, -'"'; -"'", schar+, -"'".
    // TODO schar variant
    let ctx = RuleContext::new("string");
    g.mark_define(Mark::Attr, "string", ctx.seq()
        .mark_ch('"', TMark::Mute)
        .repeat1( ctx.seq().nt("dchar"))
        .mark_ch('"', TMark::Mute) );

    // dchar: ~['"'; #a; #d]; '"', -'"'. {all characters except line breaks; quotes must be doubled}
    // TODO: fixme
    let ctx = RuleContext::new("dchar");
    g.define("dchar", ctx.seq().ch_in("abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
  
    // schar: ~["'"; #a; #d]; "'", -"'". {all characters except line breaks; quotes must be doubled}
    // TODO

    // -encoded: (tmark, s)?, -"#", hex, s.
    let ctx = RuleContext::new("encoded");
    g.mark_define(Mark::Mute, "encoded", ctx.seq()
        .opt(ctx.seq().nt("tmark").nt("s"))
        .mark_ch('#', TMark::Mute).nt("hex").nt("s"));

    // @hex: ["0"-"9"; "a"-"f"; "A"-"F"]+.
    let ctx = RuleContext::new("hex");
    g.mark_define(Mark::Attr, "hex", ctx.seq()
        .repeat1(ctx.seq().lit(Lit::union()
            .ch_range('0', '9').ch_range('a', 'f').ch_range('A', 'F'))));

    // -charset: inclusion; exclusion.
    let ctx = RuleContext::new("charset");
    g.mark_define(Mark::Mute, "charset", ctx.seq().nt("inclusion"));
    g.mark_define(Mark::Mute, "charset", ctx.seq().nt("exclusion"));

    // inclusion: (tmark, s)?,          set.
    let ctx = RuleContext::new("inclusion");
    g.define("inclusion", ctx.seq()
        .opt( ctx.seq().nt("tmark").nt("s"))
        .nt("set"));

    // exclusion: (tmark, s)?, -"~", s, set.
    let ctx = RuleContext::new("exclusion");
    g.define("exclusion", ctx.seq()
        .opt( ctx.seq().nt("tmark").nt("s"))
        .mark_ch('~', TMark::Mute).nt("s")
        .nt("set"));

    // -set: -"[", s,  (member, s)**(-[";|"], s), -"]", s.
    let ctx = RuleContext::new("set");
    g.mark_define(Mark::Mute, "set", ctx.seq()
        .mark_ch('[', TMark::Mute).nt("s")
        .repeat0_sep(
            ctx.seq().nt("member").nt("s"),
            ctx.seq().mark_ch_in(";|", TMark::Mute).nt("s"))
        .mark_ch(']', TMark::Mute).nt("s"));

    // member: string; -"#", hex; range; class.
    let ctx = RuleContext::new("member");
    g.define("member", ctx.seq().nt("string"));
    g.define("member", ctx.seq().mark_ch('#', TMark::Mute).nt("hex"));
    g.define("member", ctx.seq().nt("range"));
    g.define("member", ctx.seq().nt("class"));

    // -range: from, s, -"-", s, to.
    let ctx = RuleContext::new("range");
    g.mark_define(Mark::Mute, "range", ctx.seq().nt("from").nt("s").mark_ch('-', TMark::Mute)
        .nt("s").nt("to"));

    // @from: character.
    let ctx = RuleContext::new("from");
    g.mark_define(Mark::Attr, "from", ctx.seq().nt("character"));

    // @to: character.
    let ctx = RuleContext::new("to");
    g.mark_define(Mark::Attr, "to", ctx.seq().nt("character"));

    // -character: -'"', dchar, -'"'; -"'", schar, -"'"; "#", hex.
    // TODO: schar variant
    let ctx = RuleContext::new("character");
    g.mark_define(Mark::Mute, "character", ctx.seq()
        .mark_ch('"', TMark::Mute).nt("dchar").mark_ch('"', TMark::Mute));
    g.mark_define(Mark::Mute, "character", ctx.seq().ch('#').nt("hex"));

    // -class: code.
    let ctx = RuleContext::new("class");
    g.mark_define(Mark::Mute, "class", ctx.seq().nt("code"));

    // @code: capital, letter?.
    let ctx = RuleContext::new("code");
    g.mark_define(Mark::Attr, "code", ctx.seq().nt("capital").opt(ctx.seq().nt("letter")));

    // -capital: ["A"-"Z"].
    let ctx = RuleContext::new("capital");
    g.mark_define(Mark::Mute, "capital", ctx.seq().ch_range('A', 'Z'));

    // -letter: ["a"-"z"].
    let ctx = RuleContext::new("letter");
    g.mark_define(Mark::Mute, "letter", ctx.seq().ch_range('a', 'z'));

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
    let grammar = ixml_tree_to_grammar(&ixml_arena)?;
    Ok(grammar)
}

/// Accepts the Arena<Content> resulting from the parse of a valid ixml grammar
/// Produces a new Grammar as output
pub fn ixml_tree_to_grammar(arena: &Arena<Content>) -> Result<Grammar, ParseError> {
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
        return Err(ParseError::static_err("can't convert ixml tree to grammar: no rules present"));
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
        ixml_construct_rule(rule, mark, arena, rule_name, &mut g);
    }
    Ok(g)
}

/// Fully construct one rule. (which may involve multiple calls to ixml_rulebuilder if there are multiple alts)
pub fn ixml_construct_rule(rule: NodeId, mark: Mark, arena: &Arena<Content>, rule_name: &str, g: &mut Grammar) {
    //println!("Build rule ... {rule_name}");
    let ctx = RuleContext::new(rule_name);
    for (name, eid) in Parser::get_child_elements(arena, rule) {
        if name=="alt" {
            let rb = ixml_rulebuilder_new(eid, arena, &ctx);
            g.mark_define(mark, rule_name, rb);
        }
    }
}

/// Construct one of what ixml grammar calls an "alt", which is a sequence built from a single `SeqBuilder`
/// @param `node` is the nodeID of current element, expected to be <alt>, <repeat0>, <repeat1>, <option>, or <sep>
/// as it only looks at child elements downstream from the `NodeId` passed in
pub fn ixml_rulebuilder_new<'a>(node: NodeId, arena: &'a Arena<Content>, ctx: &'a RuleContext) -> SeqBuilder<'a> {
    let mut seq = ctx.seq();
    for (name, nid) in Parser::get_child_elements(arena, node) {
        seq = ixml_ruleappend(seq, &name, nid, arena, ctx);
    }
    seq
}

/// Add additional factors onto the given `SeqBuilder`, possibly recursively
/// @param `node` is the nodeID of current element, which is diectly processed
pub fn ixml_ruleappend<'a>(mut seq: SeqBuilder<'a>, name: &str, nid: NodeId, arena: &'a Arena<Content>, ctx: &'a RuleContext) -> SeqBuilder<'a> {

    let attrs = Parser::get_attributes(arena, nid);
    match name {
        "alts" => {
            // an <alts> with only one <alt> child can be inlined, otherwise we give it the full treatment
            let alt_elements = Parser::get_child_elements_named(arena, nid, "alt");
            if alt_elements.len()==1 {
                seq = ixml_ruleappend(seq, "alt", alt_elements[0], arena, ctx);
            } else {
                let altrules: Vec<SeqBuilder> = alt_elements.iter()
                    .map(|n| ixml_rulebuilder_new(*n, arena, &ctx))
                    .collect();
                seq = seq.alts(altrules);
            }
        }
        "literal" => {
            seq = seq.ch(attrs["string"].chars().next().expect("no empty string literals"));
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
            seq = seq.nt(&attrs["name"]);
        }
        "option" => {
            let subexpr = ixml_rulebuilder_new(nid, arena, &ctx);
            seq = seq.opt(subexpr);
        }
        "repeat0" => {
            let children = Parser::get_child_elements(arena, nid);
            // assume first child is what-to-repeat (from `factor`)
            let expr = children.get(0).expect("Should always be at least one child here");
            let repeat_this_node = expr.1;
            let mut repeat_this = ctx.seq();
            repeat_this = ixml_ruleappend(repeat_this, &expr.0, repeat_this_node, arena, ctx);

            // if a <sep> child exists, this is a ** rule, otherwise just *
            if let Some(sep) = children.get(1) {
                assert_eq!(sep.0, "sep");
                let separated_by = ixml_rulebuilder_new(sep.1, arena, &ctx);
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
            repeat_this = ixml_ruleappend(repeat_this, &expr.0, repeat_this_node, arena, ctx);

            // if a <sep> child exists, this is a ++ rule, otherwise just +
            if let Some(sep) = children.get(1) {
                assert_eq!(sep.0, "sep");
                let separated_by = ixml_rulebuilder_new(sep.1, arena, &ctx);
                seq = seq.repeat1_sep(repeat_this, separated_by)
            } else {
                seq = seq.repeat1(repeat_this);
            }
        }
        _ => unimplemented!("unknown element {name} child of <alt>"),
    }
    seq
}


#[test]
fn parse_ixml() -> Result<(), ParseError> {
    let g = ixml_grammar();
    println!("{}", &g);
    let ixml: &str = r#"doc = "A", "B"."#;
    //                  012345678901234
    let mut parser = Parser::new(g);
    let arena = parser.parse(ixml)?;
    let result = Parser::tree_to_test_format(&arena);
    let expected = r#"<ixml><rule name="doc"><alt><literal string="A"></literal><literal string="B"></literal></alt></rule></ixml>"#;
    assert_eq!(result, expected);

    println!("=============");
    let gen_grammar = ixml_tree_to_grammar(&arena)?;
    println!("{gen_grammar}");
    let mut gen_parser = Parser::new(gen_grammar);
    // now do a second pass, with the just-generated grammar
    let input2 = "AB";
    let gen_arena = gen_parser.parse(input2)?;
    let result2 = Parser::tree_to_test_format(&gen_arena);
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
