use indextree::{Arena, NodeId};

use crate::{grammar::{Grammar, Rule, Mark, TMark, RuleBuilder}, parser::{Content, Parser, ParseError}};

// TODO: -rules and @rules

/// Bootstrap ixml grammar; hand-coded definition
pub fn grammar() -> Grammar {
    let mut g = Grammar::new();

    // ixml: s, prolog?, rule++RS, s.
    // TODO: prolog
    g.define("ixml", Rule::seq().nt("s").repeat1_sep(Rule::seq().nt("rule"), Rule::seq().nt("RS")));

    // -s: (whitespace; comment)*. {Optional spacing}
    // TODO: comment
    g.mark_define(Mark::Mute, "s", Rule::seq().repeat0( Rule::seq().nt("whitespace")));

    // -RS: (whitespace; comment)+. {Required spacing}
    // TODO: comment
    g.mark_define(Mark::Mute, "RS", Rule::seq().repeat1( Rule::seq().nt("whitespace")));

    // -whitespace: -[Zs]; tab; lf; cr.
    // TODO: Unicode
    g.mark_define(Mark::Mute, "whitespace", Rule::seq().mark_ch_in(" \u{0009}\u{000a}\u{000d}", TMark::Mute));

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

    // @name: namestart, namefollower*.
    // -namestart: ["_"; L].
    // -namefollower: namestart; ["-.·‿⁀"; Nd; Mn].
    // TODO: fixme
    g.mark_define(Mark::Attr, "name", Rule::seq().repeat1( Rule::seq().ch_in("_abcdefghijklmnopqrstuvwxyzRS")));

    // alts: alt++(-[";|"], s).
    g.define("alts", Rule::seq().repeat1_sep(
        Rule::seq().nt("alt"),
        Rule::seq().mark_ch_in(";|", TMark::Mute).nt("s") ));

    // alt: term**(-",", s)
    g.define("alt", Rule::seq().repeat0_sep(
        Rule::seq().nt("term"),
        Rule::seq().mark_ch(',', TMark::Mute).nt("s") ));

    // -term: factor; option; repeat0; repeat1.
    // TODO: option; repeat0; repeat1
    g.mark_define(Mark::Mute, "term", Rule::seq().nt("factor"));

    // option: factor, -"?", s.
    //g.define("option", Rule::seq().nt("factor").mark_ch('?', Mark::Skip).nt("s"));

    // repeat0: factor, (-"*", s; -"**", s, sep).

    // repeat1: factor, (-"+", s; -"++", s, sep).

    // sep: factor.

    // -factor: terminal; nonterminal; insertion; -"(", s, alts, -")", s.
    // TODO: insertion
    g.mark_define(Mark::Mute, "factor", Rule::seq().nt("terminal"));
    g.mark_define(Mark::Mute, "factor", Rule::seq().nt("nonterminal"));
    g.mark_define(Mark::Mute, "factor", Rule::seq()
        .mark_ch('(', TMark::Mute).nt("s").nt("alts").mark_ch(')', TMark::Mute).nt("s"));

    // -terminal: literal; charset.
    // TODO charset
    g.mark_define(Mark::Mute, "terminal", Rule::seq().nt("literal"));

    // nonterminal: (mark, s)?, name, s.
    g.define("nonterminal", Rule::seq()
        .opt( Rule::seq().nt("mark").nt("s") )
        .nt("name").nt("s") );

    // literal: quoted; encoded.
    // TODO encoded
    g.define("literal", Rule::seq().nt("quoted"));

    // -quoted: (tmark, s)?, string, s.
    // TODO tmark
    g.mark_define(Mark::Mute, "quoted", Rule::seq().nt("string").nt("s"));

    // @string: -'"', dchar+, -'"'; -"'", schar+, -"'".
    // TODO schar variant
    g.mark_define(Mark::Attr, "string", Rule::seq()
        .mark_ch('"', TMark::Mute)
        .repeat1( Rule::seq().nt("dchar"))
        .mark_ch('"', TMark::Mute) );

    // dchar: ~['"'; #a; #d]; '"', -'"'. {all characters except line breaks; quotes must be doubled}
    // TODO: fixme
    g.define("dchar", Rule::seq().ch_in("abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ"));
    /*


    // TODO g.add_literalcharexcept
    // TODO doubled dquote escape
    // TODO g.add_literalcharrange
    let any_char_except_dquote = g.add_litcharoneof("0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZ.,:;?!@#$%^&*'abcdefghijklmnopqrstuvwxyz()<>[]{}-_+=");
    g.add_rule("dchar", any_char_except_dquote);
    */
    g
}

#[test]
fn parse_ixml() -> Result<(), ParseError> {
    let g = grammar();
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

/// Accepts the Arena<Content> resulting from the parse of a valid ixml grammar
/// Produces a new Grammar as output
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
    for rule in all_rules {
        let rule_name = &Parser::get_attributes(arena, rule)["name"];
        // TODO: rule_mark
        ixml_build_rule(rule, arena, rule_name, &mut g);
    }
    g
}

//<rule name="doc"><alt><literal string="A"></literal><literal string="B"></literal></alt></rule>

/// Construct one rule.
/// If it has multiple alts, we end up calling grammar.define() multiple times
pub fn ixml_build_rule(rule: NodeId, arena: &Arena<Content>, rule_name: &str, g: &mut Grammar) {
    // TODO Parser::get_attributes(arena, rule)["mark"] ...
    for (name, eid) in Parser::get_elements(arena, rule) {
        if name=="alt" {
            let rb = ixml_build_alts(eid, arena);
            g.define(rule_name, rb);
        }
    }
}

/// Construct one alt, which is a sequence built from a single `RuleBuilder`
pub fn ixml_build_alts(alt: NodeId, arena: &Arena<Content>) -> RuleBuilder {
    let mut rb = Rule::seq();
    for (ename, enid) in Parser::get_elements(arena, alt) {
        let attrs = Parser::get_attributes(arena, enid);
        match ename.as_str() {
            "literal" => {
                rb = rb.ch(attrs["string"].chars().next().expect("no empty string literals"));
            }
            "nonterminal" => {
                rb = rb.nt(&attrs["name"]);
            }
            _ => unimplemented!("unknown element {ename} child of <alt>"),
        }
    }
    rb
}
