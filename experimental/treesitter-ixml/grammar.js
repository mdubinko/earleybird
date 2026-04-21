// grammar.js
module.exports = grammar({
  name: 'ixml',

  // Hybrid: whitespace and comments are invisible (extras).
  // In option B, you might make them visible nodes instead.
  extras: $ => [
    $.whitespace,
    $.comment,
  ],

  // Hybrid: inline some structural helpers so they don't clutter the CST.
  // In option B, you would REMOVE them from `inline` to keep them as nodes.
  inline: $ => [
    $.factor,
    $.term,
    $.alt,
    $.alts,
    $.sep,
  ],

  rules: {
    // Top-level: ixml: s, prolog?, rule++RS, s.
    // With extras handling spacing, we just need prolog? and 1+ rules.
    ixml_grammar: $ => seq(
      optional($.prolog),
      repeat1($.rule),
    ),

    // prolog: version, s.
    prolog: $ => $.version,

    // version: -"ixml", RS, -"version", RS, string, s, -'.' .
    version: $ => seq(
      field('keyword_ixml', 'ixml'),
      field('keyword_version', 'version'),
      field('version_string', $.string),
      '.',
    ),

    // rule: (mark, s)?, name, s, -["=:"], s, -alts, -".".
    rule: $ => seq(
      optional($.mark),
      field('name', $.name),
      field('assign', choice('=', ':')),
      field('alts', $.alts),
      '.',
    ),

    // @mark: ["@^-"].
    mark: $ => token(choice('@', '^', '-')),

    // alts: alt++(-[";|"], s).
    // Hybrid: alts/alt are inline; in option B, remove them from `inline`.
    alts: $ => seq(
      $.alt,
      repeat(seq(
        field('separator', choice(';', '|')),
        $.alt,
      )),
    ),

    // alt: term**(-",", s).
    alt: $ => seq(
      $.term,
      repeat(seq(
        ',',
        $.term,
      )),
    ),

    // -term: factor; option; repeat0; repeat1.
    term: $ => choice(
      $.factor,
      $.option,
      $.repeat0,
      $.repeat1,
    ),

    // -factor: terminal; nonterminal; insertion; "(" alts ")".
    factor: $ => choice(
      $.terminal,
      $.nonterminal,
      $.insertion,
      seq('(', $.alts, ')'),
    ),

    // repeat0: factor, ("*" ; "**" sep).
    repeat0: $ => seq(
      field('body', $.factor),
      field('op', choice(
        '*',
        seq('**', $.sep),
      )),
    ),

    // repeat1: factor, ("+" ; "++" sep).
    repeat1: $ => seq(
      field('body', $.factor),
      field('op', choice(
        '+',
        seq('++', $.sep),
      )),
    ),

    // option: factor, "?".
    option: $ => seq(
      field('body', $.factor),
      '?',
    ),

    // sep: factor.
    sep: $ => $.factor,

    // nonterminal: (mark, s)?, name, s.
    nonterminal: $ => seq(
      optional($.mark),
      field('name', $.name),
    ),

    // @name: namestart, namefollower*.
    // Hybrid: we collapse namestart/namefollower into a single token.
    // In option B, you could expose namestart/namefollower as separate rules.
    name: $ => token(/[A-Za-z_][A-Za-z0-9_.-]*/),

    // -terminal: literal; charset.
    terminal: $ => choice(
      $.literal,
      $.charset,
    ),

    // literal: quoted; encoded.
    literal: $ => choice(
      $.quoted,
      $.encoded,
    ),

    // -quoted: (tmark, s)?, string, s.
    quoted: $ => seq(
      optional($.tmark),
      $.string,
    ),

    // @tmark: ["^-"].
    tmark: $ => token(choice('^', '-')),

    // @string: double-quoted or single-quoted with doubled quotes.
    // This approximates the ixml rules for dchar/schar.
    string: $ => token(choice(
      /"([^"\r\n]|"")+"/,
      /'([^'\r\n]|'')+'/
    )),

    // -encoded: (tmark, s)?, "#", hex, s.
    encoded: $ => seq(
      optional($.tmark),
      '#',
      $.hex,
    ),

    // @hex: ["0"-"9"; "a"-"f"; "A"-"F"]+.
    hex: $ => token(/[0-9a-fA-F]+/),

    // -charset: inclusion; exclusion.
    charset: $ => choice(
      $.inclusion,
      $.exclusion,
    ),

    // inclusion: (tmark, s)?, set.
    inclusion: $ => seq(
      optional($.tmark),
      $.set,
    ),

    // exclusion: (tmark, s)?, "~", s, set.
    exclusion: $ => seq(
      optional($.tmark),
      '~',
      $.set,
    ),

    // -set: "[", (member)**([";|"]), "]".
    // Hybrid: we don't expose the separators as nodes; in B you might.
    set: $ => seq(
      '[',
      optional(seq(
        $.member,
        repeat(seq(
          optional(choice(';', '|')),
          $.member,
        )),
      )),
      ']',
    ),

    // member: string; "#", hex; range; class.
    member: $ => choice(
      $.string,
      seq('#', $.hex),
      $.range,
      $.class,
    ),

    // -range: from, "-", to.
    range: $ => seq(
      field('from', $.character),
      '-',
      field('to', $.character),
    ),

    // @from/@to: character.
    // -character: '"', dchar, '"' ; "'" schar "'" ; "#" hex.
    // Hybrid: we approximate dchar/schar with a single-char string.
    character: $ => choice(
      token(/"([^"\r\n]|"")"/),
      token(/'([^'\r\n]|'')'/),
      seq('#', $.hex),
    ),

    // -class: code.
    class: $ => $.code,

    // @code: capital, letter?.
    code: $ => seq(
      $.capital,
      optional($.letter),
    ),

    // -capital: ["A"-"Z"].
    capital: $ => token(/[A-Z]/),

    // -letter: ["a"-"z"].
    letter: $ => token(/[a-z]/),

    // insertion: "+", (string; "#", hex).
    insertion: $ => seq(
      '+',
      choice(
        $.string,
        seq('#', $.hex),
      ),
    ),

    // ---------- Lexical helpers (hybrid) ----------

    // whitespace: (Zs; tab; lf; cr)+.
    // We approximate Zs + tab/lf/cr with a simple class.
    // In option B, you might want separate rules for tab/lf/cr.
    whitespace: $ => token(/[ \t\r\n]+/),

    // comment: "{", (cchar; comment)*, "}".
    // Nested comments are allowed via recursion.
    // In option B, you might expose cchar as a separate node.
    comment: $ => seq(
      '{',
      repeat(choice(
        $.comment,
        $.comment_char,
      )),
      '}',
    ),

    // -cchar: ~["{}"].
    comment_char: $ => token(/[^{}]/),
  },
});
