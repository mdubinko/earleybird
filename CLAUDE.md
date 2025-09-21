This is a project to implement Invisible XML (ixml) parser and CLI tool with 100% conformance.

# Human comprehensibilty

- For all new and updated code, carefully consider how to make it extremely human comprehensible, an example for other projects to follow. Follow Rust idioms wherever possible.

# iXML 1.0 Specification Reference

- The following self-documenting ixml grammar (similar to EBNF) is from the specifiation, and should be followed closely:
<ixml>
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
</ixml>

- Primary specification: https://invisiblexml.org/1.0/ -- access this if looking at the EBNF above is insufficient
- Use this as authoritative source for grammar syntax and semantics
- Bootstrap implementation should match spec grammar productions exactly
- This project's goal is 100% conformance to the ixml spec

# Debugging Workflow

Whenever generating log files or capturing trace output, put the files in the log/ directory, to avoid cluttering up the project root dir.

## Token-Efficient Test Suite Analysis
```bash
# Use existing filters strategically
cargo run -- suite syntax              # Focus on syntax issues
cargo run -- suite character-sets      # Target specific features
cargo run -- suite hex                 # Test specific patterns

# Focus on high-impact tests
cargo run -- suite --limit=10          # Don't overwhelm output
```

## Trace-Based Debugging (Highly Efficient)
```bash
# Generate focused trace files
cargo run -- test -g 'test: [#41].' -i 'A' -v trace --trace-file log/debug.log

# Post-hoc filtering (very token-efficient)
grep "charset\|inclusion\|set\|member" log/debug.log # Character set parsing
grep -A 3 -B 3 "FAIL" log/debug.log                  # Context around failures
grep "SCANNER.*string" log/debug.log                 # String processing issues
grep "pos=0" log/debug.log                           # Focus on specific position
```

## Strategic Debugging Approach
1. **Test known working patterns first** (like `[#20]` works, `["A"]` fails)
2. **Use temporary debug prints** for pinpointing issues (remove after fixing)
3. **Focus on parse tree structure** - grammar parsing vs input parsing are different issues
4. **Leverage test suite patterns** - find working examples to understand correct behavior

## Current Status: Character Sets
- ✅ Single hex: `[#20]` works
- ✅ Hex ranges: `[#41-#46]` works (fixed hex prefix stripping)
- ❌ String members: `["A"]` fails at bootstrap grammar level
- 🔄 Next: Debug why string members aren't parsed by bootstrap grammar

# TODO File Management

- Always read `TODO.txt` at session start
- Reference TODO items when suggesting or planning work
- Auto-update TODO.txt after completing tasks
- Add new issues discovered while coding

## TODO Format

- `- [ ]` incomplete, `- [x]` complete; Include file paths: `(src/auth.js:45)`
- Suffix tags for priority or anything else: `#HIGH` `#MED` `#LOW`; or `#techdebt`, etc.

## Workflow

@TODO.txt is for multi-session issues; not immediate work at hand

- Mention when adding, modifying, working on, or marking items done
- Break down COMPLEX tasks

# WebAssembly

- Always ensure that we are producing code that can target WebAssembly (this does not include test harnesses or suites)

# ⚠️ CRITICAL EARLEY PARSER BUG - DELETE AFTER RESOLUTION ⚠️

## Root Cause: Premature Completion in Earley Parser

**Issue**: 75/78 syntax tests fail due to fundamental timing issue in Earley parser's COMPLETER operation.

**Specific Problem**: When parsing `["0"-"9"]` at position 10, `member → string` completes successfully with `"0"`, which triggers immediate parent continuation. This causes the synthesized repeat construct `(member, s)**(-[";|"], s)` to complete before `member → range` can consume the `-` character for the full range pattern.

### Trace Evidence:
```
EARLEY|pos=10|COMPLETER: member=( string@10 • ) completed
EARLEY|pos=10|PREDICTOR: --set.f-plus-sep2=( member@10 • s, ---set.f-star3 )
# Repeat construct immediately checks for separator `;|`, finds `-` instead, completes
EARLEY|pos=10|PREDICTOR: range=( from@10 • s, -['-'], s, to )  # Too late!
```

### Architecture Insight:
Repeat constructs (`f++sep`, `f**sep`, `f*`, `f?`) are **pure syntactic sugar** that expand to regular grammar rules:

- `f++sep` → `f, (sep, f)*`
- `f**sep` → `(f++sep)?`
- `f*` → `(f, f-star)?`
- `f?` → `f; ()`

**There should be no special-casing for repeat constructs in the parser**. The bug is in the fundamental Earley algorithm implementation, not repeat-specific logic.

### Real Issue:
The COMPLETER operation continues parent tasks **immediately** when a child completes, rather than waiting for **all alternatives to be exhaustively explored** per ABC reference specification. This violates the Earley algorithm's requirement that all possibilities at a position be fully examined.

### Code Changes Made:
- Added comprehensive trace debugging showing exact timing of completions
- Enhanced parse failure diagnostics with queue snapshots
- Created failing unit test demonstrating the issue: `test_synthesized_repeat_ambiguity_handling`
- Fixed unrelated tree processing bug (attributes vs child elements)

### Required Fix:
Modify COMPLETER logic to ensure **exhaustive alternative exploration** before parent continuation. This is a core algorithmic fix to make the parser "thoroughly, bulletproof-ly correct" without any special-casing for synthesized constructs.
