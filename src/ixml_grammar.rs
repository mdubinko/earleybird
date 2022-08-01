use crate::parser::Grammar;

pub fn grammar() -> Grammar {
    let mut g = Grammar::new();

    // literals
    let _colon = g.add_litchar(':');
    let _comma = g.add_litchar(',');
    let _dot = g.add_litchar('.');
    let _vbar = g.add_litchar('|');
    let _equals = g.add_litchar('=');
    let _squote = g.add_litchar('\'');
    let _qmark = g.add_litchar('?');
    let _plus = g.add_litchar('+');
    let _star = g.add_litchar('*');
    let _dquote = g.add_litchar('"');
    let _minus = g.add_litchar('-');
    let _lbrack = g.add_litchar('[');
    let _rbrack = g.add_litchar(']');
    let _lparen = g.add_litchar('(');
    let _rparen = g.add_litchar(')');
    let _lbrace = g.add_litchar('{');
    let _rbrace = g.add_litchar('}');
    let _at = g.add_litchar('@');
    let _caret = g.add_litchar('^');
    let _hash = g.add_litchar('#');
    let _tilde = g.add_litchar('~');
    let _colon_or_equals = g.add_litcharoneof(":=");
    let _semicolon_or_vbar = g.add_litcharoneof(";|");
 
    // nonterminals
    let s = g.add_nonterm("s");
    let rule = g.add_nonterm("rule");
    let rs = g.add_nonterm("RS");
    let name = g.add_nonterm("name");
    let alts = g.add_nonterm("alts");
    let alt = g.add_nonterm("alt");
    let term = g.add_nonterm("term");
    let factor = g.add_nonterm("factor");
    let terminal = g.add_nonterm("terminal");
    let nonterminal = g.add_nonterm("nonterminal");
    let option = g.add_nonterm("option");
    let sep = g.add_nonterm("sep");
    let repeat0 = g.add_nonterm("repeat0");
    let repeat1 = g.add_nonterm("repeat1");
    let literal = g.add_nonterm("literal");
    let quoted = g.add_nonterm("quoted");
    let string = g.add_nonterm("string");
    let dchar = g.add_nonterm("dchar");

    // ixml: s, prolog?, rule++RS, s.
    // TODO: prolog
    let rule_plus_plus_rs = g.add_repeat1_sep(rule, rs);
    let ixml_seq = g.add_seq(vec![s, rule_plus_plus_rs, s]);
    g.add_rule("ixml", ixml_seq);

    // rule: (mark, s)?, name, s, -["=:"], s, -alts, -".".
    // TODO mark
    let rule_seq = g.add_seq(vec![name, s, _colon_or_equals, s, alts, _dot]);
    g.add_rule("rule", rule_seq);

    // alts: alt++(-[";|"], s).
    let alt_plus_plus_semi_or_vbar = g.add_repeat1_sep(alt, _semicolon_or_vbar);
    g.add_rule("alts", alt_plus_plus_semi_or_vbar);

    // alt: term**(-",", s)
    let comma_space = g.add_seq(vec![_comma, s]);
    let term_star_star_comma_space = g.add_repeat0_sep(term, comma_space);
    g.add_rule("alt", term_star_star_comma_space);

    // -term: factor; option; repeat0; repeat1.
    let term_opts = g.add_oneof(vec![factor, option, repeat0, repeat1]);
    g.add_rule("term", term_opts);

    // -factor: terminal; nonterminal; insertion; -"(", s, alts, -")", s.
    // TODO insertion
    let paren_alts_paren = g.add_seq(vec![_lparen, s, alts, _rparen, s]);
    let factor_opts = g.add_oneof(vec![terminal, nonterminal, paren_alts_paren]);
    g.add_rule("factor", factor_opts);

    // option: factor, -"?", s.
    let factor_qmark_s = g.add_seq(vec![factor, _qmark, s]);
    g.add_rule("option", factor_qmark_s);

    // repeat0: factor, (-"*", s; -"**", s, sep).
    let star_s = g.add_seq(vec![_star, s]);
    let star_star_sep = g.add_seq(vec![_star, _star, s, sep]);
    let star_or_star_star_sep = g.add_oneof(vec![star_s, star_star_sep]);
    let repeat0_seq = g.add_seq(vec![factor, star_or_star_star_sep]);
    g.add_rule("repeat0", repeat0_seq);

    // repeat1: factor, (-"+", s; -"++", s, sep).
    let plus_s = g.add_seq(vec![_plus, s]);
    let plus_plus_sep = g.add_seq(vec![_plus, _plus, s, sep]);
    let plus_or_plus_plus_sep = g.add_oneof(vec![plus_s, plus_plus_sep]);
    let repeat1_seq = g.add_seq(vec![factor, plus_or_plus_plus_sep]);
    g.add_rule("repeat1", repeat1_seq);

    // sep: factor.
    g.add_rule("sep", factor);

    // -terminal: literal; charset.
    // TODO charset
    g.add_rule("terminal", literal);

    // nonterminal: (mark, s)?, name, s.
    // TODO mark
    let nonterminal_seq = g.add_seq(vec![name, s]);
    g.add_rule("nonterminal", nonterminal_seq);

    // literal: quoted; encoded.
    // TODO encoded
    g.add_rule("literal", quoted);

    // -quoted: (tmark, s)?, string, s.
    // TODO tmark
    let string_s = g.add_seq(vec![string, s]);
    g.add_rule("quoted", string_s);

    // @string: -'"', dchar+, -'"'; -"'", schar+, -"'".
    // TODO schar
    let dchar_plus = g.add_repeat1(dchar);
    let string_seq = g.add_seq(vec![_dquote, dchar_plus, _dquote]);
    g.add_rule("string", string_seq);

    // dchar: ~['"'; #a; #d]; '"', -'"'. {all characters except line breaks; quotes must be doubled}
    // TODO g.add_literalcharexcept
    // TODO doubled dquote escape
    // TODO g.add_literalcharrange
    let any_char_except_dquote = g.add_litcharoneof("0123456789 ABCDEFGHIJKLMNOPQRSTUVWXYZ.,:;?!@#$%^&*'abcdefghijklmnopqrstuvwxyz()<>[]{}-_+=");
    g.add_rule("dchar", any_char_except_dquote);

    g
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
}

