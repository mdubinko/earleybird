use crate::grammar::{Grammar, Rule, Mark, TMark};

// TODO: -rules and @rules

/// Bootstrap ixml grammar; hand-coded definition
pub fn grammar() -> Grammar {
    let mut g = Grammar::new("ixml");

    // ixml: s, prolog?, rule++RS, s.
    // TODO: prolog
    g.define("ixml", Rule::seq().nt("s").repeat1_sep(Rule::seq().nt("rule"), Rule::seq().nt("RS")));

    // -s: (whitespace; comment)*. {Optional spacing}
    // TODO: comment
    g.define("s", Rule::seq().repeat0( Rule::seq().nt("whitespace")));

    // -RS: (whitespace; comment)+. {Required spacing}
    // TODO: comment
    g.define("RS", Rule::seq().repeat1( Rule::seq().nt("whitespace")));

    // -whitespace: -[Zs]; tab; lf; cr.
    // TODO: Unicode
    g.define("whitespace", Rule::seq().ch_in(" \u{0009}\u{000a}\u{000d}"));

    // rule: (mark, s)?, name, s, -["=:"], s, -alts, -".".
    g.define("rule", Rule::seq()
        .opt(Rule::seq().nt("mark").nt("s"))
        .nt("name")
        .nt("s")
        .mark_ch_in("=:", TMark::Mute)
        .nt("s")
        .nt_mark("alts", Mark::Mute)
        .mark_ch('.', TMark::Mute) );

    // @mark: ["@^-"].
    g.define("mark", Rule::seq().ch_in("@^-"));

    // @name: namestart, namefollower*.
    // -namestart: ["_"; L].
    // -namefollower: namestart; ["-.·‿⁀"; Nd; Mn].
    // TODO: fixme
    g.define("name", Rule::seq().repeat1( Rule::seq().ch_in("_abcdefghijklmnopqrstuvwxyzRS")));

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
    g.define("term", Rule::seq().nt("factor"));

    // option: factor, -"?", s.
    //g.define("option", Rule::seq().nt("factor").mark_ch('?', Mark::Skip).nt("s"));

    // repeat0: factor, (-"*", s; -"**", s, sep).

    // repeat1: factor, (-"+", s; -"++", s, sep).

    // sep: factor.

    // -factor: terminal; nonterminal; insertion; -"(", s, alts, -")", s.
    // TODO: insertion
    g.define("factor", Rule::seq().nt("terminal"));
    g.define("factor", Rule::seq().nt("nonterminal"));
    g.define("factor", Rule::seq()
        .mark_ch('(', TMark::Mute).nt("s").nt("alts").mark_ch(')', TMark::Mute).nt("s"));

    // -terminal: literal; charset.
    // TODO charset
    g.define("terminal", Rule::seq().nt("literal"));

    // nonterminal: (mark, s)?, name, s.
    g.define("nonterminal", Rule::seq()
        .opt( Rule::seq().nt("mark").nt("s") )
        .nt("name").nt("s") );

    // literal: quoted; encoded.
    // TODO encoded
    g.define("literal", Rule::seq().nt("quoted"));

    // -quoted: (tmark, s)?, string, s.
    // TODO tmark
    g.define("quoted", Rule::seq().nt("string").nt("s"));

    // @string: -'"', dchar+, -'"'; -"'", schar+, -"'".
    // TODO schar variant
    g.define("string", Rule::seq()
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
fn parse_ixml() {
    let g = grammar();
    println!("{}", &g);
    let ixml: &str = r#"doc = "A", "B"."#;
    //                  012345678901234
    let mut parser = crate::parser::Parser::new(g);
    let _trace = parser.parse(ixml);
    let result = crate::parser::Parser::tree_to_testfmt( &parser.unpack_parse_tree("ixml") );
    let expected = "<ixml><rule></rule></ixml>"; // TODO: fixme
    assert_eq!(result, expected);

    // now do a second pass, with the just-generated grammar
    let _input2 = "AB";
    let _expected2 = "<doc>AB</doc>";
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
