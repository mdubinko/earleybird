use crate::grammar::{Grammar, Mark, TMark, Lit, RuleContext};

/// Bootstrap ixml grammar; hand-coded definition
pub fn bootstrap_ixml_grammar() -> Grammar {
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
    let ctx = RuleContext::new("name");
    g.mark_define(Mark::Attr, "name", ctx.seq().nt("namestart").repeat0(ctx.seq().nt("namefollower")));
    
    // -namestart: ["_"; L].
    // Basic implementation: underscore and letters
    let ctx = RuleContext::new("namestart");
    g.mark_define(Mark::Mute, "namestart", ctx.seq().ch_in("_abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"));

    // -namefollower: namestart; ["-.·‿⁀"; Nd; Mn].
    // Basic implementation: namestart chars plus digits and hyphens
    let ctx = RuleContext::new("namefollower");
    g.mark_define(Mark::Mute, "namefollower", ctx.seq().nt("namestart"));
    g.mark_define(Mark::Mute, "namefollower", ctx.seq().ch_in("-0123456789"));

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
    let ctx = RuleContext::new("string");
    g.mark_define(Mark::Attr, "string", ctx.seq()
        .mark_ch('"', TMark::Mute)
        .repeat1( ctx.seq().nt("dchar"))
        .mark_ch('"', TMark::Mute) );
    g.mark_define(Mark::Attr, "string", ctx.seq()
        .mark_ch('\'', TMark::Mute)
        .repeat1( ctx.seq().nt("schar"))
        .mark_ch('\'', TMark::Mute) );

    // dchar: ~['"'; #a; #d]; '"', -'"'. {all characters except line breaks; quotes must be doubled}
    let ctx = RuleContext::new("dchar");
    g.define("dchar", ctx.seq().lit(Lit::union().exclude().ch('"').ch('\n').ch('\r')));
    g.define("dchar", ctx.seq().ch('"').mark_ch('"', TMark::Mute));
  
    // schar: ~["'"; #a; #d]; "'", -"'". {all characters except line breaks; quotes must be doubled}
    let ctx = RuleContext::new("schar");
    g.define("schar", ctx.seq().lit(Lit::union().exclude().ch('\'').ch('\n').ch('\r')));
    g.define("schar", ctx.seq().ch('\'').mark_ch('\'', TMark::Mute));

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
    let ctx = RuleContext::new("character");
    g.mark_define(Mark::Mute, "character", ctx.seq()
        .mark_ch('"', TMark::Mute).nt("dchar").mark_ch('"', TMark::Mute));
    g.mark_define(Mark::Mute, "character", ctx.seq()
        .mark_ch('\'', TMark::Mute).nt("schar").mark_ch('\'', TMark::Mute));
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
