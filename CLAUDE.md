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
# Built-in token-efficient modes (recommended)
cargo run -- suite syntax --stdout summary --file-mode none           # Just summary (most efficient)
cargo run -- suite syntax --stdout summary --file-mode failures-only  # Summary + failures to file
cargo run -- suite syntax --stdout progress-only --file-mode none     # Only show passes

# Legacy shell-based approaches (still useful)
cargo run -- suite syntax | head -30          # Limit output to first 30 lines
cargo run -- suite 2>/dev/null | grep -c "✅ PASS"     # Count passes
cargo run -- suite 2>/dev/null | grep -c "🔥 GRAMMAR ERROR"  # Count grammar errors

# Advanced: Separate file vs stdout control
cargo run -- suite --stdout summary --file-mode all -o log/full.txt   # Full details to file, summary to chat
cargo run -- suite --stdout quiet --file-mode failures-only -o log/failures.txt  # Silent with failures logged
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

# ✅ RESOLVED: CRITICAL EARLEY PARSER BUG - CHARACTER RANGES NOW WORK

## ~~Root Cause: Premature Completion in Earley Parser~~ **FIXED**

**Issue**: ~~75/78 syntax tests fail~~ **Character range parsing now works correctly**

**Problem RESOLVED**: The parser was continuing parent tasks immediately when a child completed, rather than exploring all alternatives at the current position first. This prevented patterns like `["0"-"9"]` from working because `member → string` would complete with `"0"` and trigger parent continuation before `member → range` could process the full `"0"-"9"` pattern.

### Solution Implemented:
**Fixed COMPLETER queue management** - Parent continuations are now queued at the back (`queue_back`) instead of front (`queue_front`), ensuring all alternatives at the current position are explored before parent propagation.

**Key Change** (src/parser.rs:438):
```rust
// OLD: self.queue_front(maybe_id);  // Immediate parent continuation
// NEW: self.queue_back(maybe_id);   // Defer parent continuation
```

### Verification:
```bash
# Character ranges now work correctly
cargo run -- test -g 'test: ["0"-"9"].' -i '5'     # ✅ WORKS
cargo run -- test -g 'test: [#41-#5A].' -i 'G'     # ✅ WORKS
cargo run -- test -g 'test: ["0"-"9"].' -i 'A'     # ✅ CORRECTLY REJECTS
```

### Current Status:
- ✅ **Character ranges**: `["0"-"9"]`, `[#41-#5A]` work correctly
- ✅ **Hex ranges**: `[#20-#30]` work correctly
- ✅ **Mixed character sets**: Multiple ranges and hex patterns work
- ❌ **Single string members**: `["A"]` still fail due to different bootstrap parsing issues
- ❌ **Complex grammars**: Multi-rule grammars still have bootstrap parsing issues

### Impact:
- Character range parsing is now **architecturally correct**
- Test suite pass rate: Still 2/78 due to remaining bootstrap grammar parsing issues with string members
- **This fix enables all range-based character sets**, resolving the core algorithmic issue

### Next Priority:
Focus on remaining bootstrap grammar parsing issues, particularly single-character string members `["A"]` which fail during grammar parsing phase (not input parsing).
