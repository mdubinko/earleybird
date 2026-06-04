This is a project to implement Invisible XML (ixml) parser and CLI tool with 100% conformance.

# Human comprehensibilty

- For all new and updated code, carefully consider how to make it extremely human comprehensible, an example for other projects to follow. Follow Rust idioms wherever possible.

- Inline comments should indicate intent, and NOT just rephrase what the code is doing. Inline comments must avoid referring to past version of the code that no longer exist.

Examples:

Good:  // Compute weighted average
Bad:   // Check if foo is null
Bad:   // No longer calling old_fn_that_no_longer_exists()
Bad:   // NEW algorithm...

# iXML 1.0 Specification Reference

- The following self-documenting ixml grammar (similar to EBNF) is from the specifiation, and should be followed closely:
- Comments within ixml_bootstrap.rs reproduce these on a rule-by-rule basis.
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

         rule: naming, -["=:"], s, -alts, -".".
        @mark: ["@^-"].
       naming: (mark, s)?, name, s, (-">", s, alias, s)?.
        @alias: name.
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
  nonterminal: naming.

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

# CLI Commands

All commands support granular debug control with `--console` and `--file` options.

## parse - Parse input using ixml grammar
```bash
# Basic parsing
cargo run -- parse -g grammar.ixml -i input.txt

# Parse strings (great for quick testing)
cargo run -- parse --grammar-str 'A: "a".' --input-str 'a'

# Debug bootstrap grammar issues
cargo run -- parse -g grammar.ixml -i input.txt \
  --console DEBUG --console-filter BOOTSTRAP,GRAMMAR --file NONE

# Focus on scanner issues with file logging
cargo run -- parse --grammar-str 'test: ["A"-"Z"]+.' --input-str 'Hello123' \
  --console DEBUG --console-filter SCANNER --file DEBUG -o log/scanner.log
```

## validate - Validate ixml grammar (bootstrap parsing)
```bash
# Basic validation
cargo run -- validate -g grammar.ixml

# Debug bootstrap parsing issues
cargo run -- validate --grammar-str 'A: B. B: "b".' \
  --console DEBUG --console-filter BOOTSTRAP,GRAMMAR

# Silent validation with detailed file logging
cargo run -- validate -g complex.ixml \
  --console NONE --file DEBUG -o log/validation.log
```

## suite - Run conformance test suite
```bash
# Standard usage
cargo run -- suite --console SUMMARY --file FAILURES

# Filter tests by pattern
cargo run -- suite expr1 --console SUMMARY --file NONE
cargo run -- suite correct --console SUMMARY --file FAILURES -o log/results.txt

# Debug specific test categories
cargo run -- suite syntax --console DEBUG --console-filter BOOTSTRAP,GRAMMAR --file NONE
```

# Debugging Workflow

Whenever generating log files or capturing trace output, put the files in the log/ directory, to avoid cluttering up the project root dir.

## New Debug System (2025)

The debug system now supports granular control with console and file output:

**Console/File Levels**: DEBUG, INFO, SUMMARY, WARNING, ERROR, ALL, NONE, OFF, FAILURES
**Categories**: BOOTSTRAP, QUEUE, SCANNER, OUTPUT, PREDICT, COMPLETE, DEDUP, GRAMMAR

### Common Debug Patterns

```bash
# Bootstrap grammar debugging (highest priority issue)
cargo run -- validate --grammar-str 'test: rule.' \
  --console DEBUG --console-filter BOOTSTRAP,GRAMMAR --file NONE

# Character set debugging
cargo run -- parse --grammar-str 'test: ["A"-"Z"]+.' --input-str 'Hello' \
  --console DEBUG --console-filter SCANNER --file NONE

# Queue management debugging
cargo run -- parse -g complex.ixml -i input.txt \
  --console INFO --console-filter QUEUE,DEDUP --file DEBUG -o log/queue.log

# Silent operation with comprehensive logging
cargo run -- suite syntax --console NONE --file DEBUG -o log/full-debug.log
```

### Category Descriptions
- **BOOTSTRAP**: Grammar parsing (ixml → internal representation) - 🔥 critical for 20 failing tests
- **QUEUE**: Task queue operations (position-bucketed Earley queue)
- **SCANNER**: Character matching and advancement
- **OUTPUT**: XML formatting and tree conversion - 📝 quick wins for 15 failing tests
- **PREDICT**: Earley prediction operations
- **COMPLETE**: Earley completion operations
- **DEDUP**: Task deduplication
- **GRAMMAR**: Grammar processing and validation

## Test Suite Emoji Indicators

When running test suites or grepping log files, these emoji indicators help identify test outcomes:

- 🧪 **Test start**: "Test {name} ..."
- ✅ **PASS**: Test succeeded
- ❌ **FAIL**: Test failed (expected vs actual mismatch)
- 🔥 **GRAMMAR ERROR**: Bootstrap grammar parsing failed
- ⚠️ **PARSE ERROR**: Input parsing failed with grammar
- 💥 **PANIC**: Test crashed with panic
- ⏭️ **SKIP**: Test skipped (missing components)
- 🚧 **TODO**: Test marked as not yet implemented

Example grep commands:
```bash
grep "✅ PASS" log/results.txt           # Find passing tests
grep "🔥 GRAMMAR ERROR" log/results.txt  # Find bootstrap grammar issues
grep "🚧 TODO" log/results.txt           # Find unimplemented test cases

# Special test case indicators (shown during test loading)
grep "🫥 Ambiguous" log/results.txt      # Tests expecting ambiguous parses (multiple trees)
grep "📦 Version mismatch" log/results.txt  # Tests with version-specific behavior
grep "💥 Expected failure" log/results.txt  # Tests that should fail
```

## Test Suite Filtering

You can filter tests by name using the suite command's second argument:

```bash
cargo run -- suite expr1             # Run all tests containing "expr1"
cargo run -- suite correct           # Run all tests in correct/ directory
cargo run -- suite attribute         # Run all attribute-related tests
cargo run -- suite syntax/elem       # Run specific test pattern
```

This is much more efficient than running the full test suite when debugging specific issues.

## Test Suite Expected Results

The iXML test catalog may list more than one acceptable expected result for a
single test case, usually through multiple `<assert-xml>` entries. Treat these
as alternatives: a case passes when the parser output matches any listed
expected result. This is especially important for ambiguous grammars where the
current parser may choose one valid tree while the catalog lists several valid
serializations.

### Debug Examples by Use Case

```bash
# Quick test validation
cargo run -- validate --grammar-str 'test: "a".' --console SUMMARY --file NONE

# Deep bootstrap debugging
cargo run -- validate -g failing-grammar.ixml \
  --console DEBUG --console-filter BOOTSTRAP,GRAMMAR \
  --file DEBUG -o log/bootstrap-debug.log

# Performance analysis
cargo run -- parse -g large.ixml -i big-input.txt \
  --console SUMMARY --file DEBUG --file-filter QUEUE,DEDUP -o log/perf.log

# Test suite debugging with focused output
cargo run -- suite correct \
  --console INFO --console-filter BOOTSTRAP \
  --file FAILURES -o log/bootstrap-failures.txt
```

## Token-Efficient Test Suite Analysis
```bash
# New CLI (recommended)
cargo run -- suite syntax --console SUMMARY --file NONE                    # Just summary (most efficient)
cargo run -- suite syntax --console SUMMARY --file FAILURES                # Summary + failures to file
cargo run -- suite syntax --console INFO --console-filter BOOTSTRAP --file NONE  # Only bootstrap issues

# Legacy shell-based approaches (still useful)
cargo run -- suite syntax --console SUMMARY | head -30          # Limit output to first 30 lines
cargo run -- suite 2>/dev/null | grep -c "✅ PASS"             # Count passes
cargo run -- suite 2>/dev/null | grep -c "🔥 GRAMMAR ERROR"    # Count grammar errors

# stderr handling for debugging workflows
cargo run -- suite broken-tests 2>log/errors.log               # Capture warnings/errors separately
cargo run -- parse -g bad.ixml -i input.txt 2>log/parse-errors.log  # CLI errors to file
cargo run -- suite --console NONE 2>&1 | grep "Warning"        # Merge stderr to stdout, filter warnings
cargo run -- suite syntax 2>/dev/null | head -20               # Pure stdout analysis, no stderr noise

# Advanced: Separate console vs file control
cargo run -- suite --console SUMMARY --file ALL -o log/full.txt       # Full details to file, summary to console
cargo run -- suite --console NONE --file FAILURES -o log/failures.txt  # Silent with failures logged
```

## Trace-Based Debugging (Highly Efficient)
```bash
# Generate focused trace files with new CLI
cargo run -- parse --grammar-str 'test: [#41].' --input-str 'A' \
  --console NONE --file DEBUG -o log/debug.log

# Category-specific debugging
cargo run -- parse --grammar-str 'test: ["A"-"C"].' --input-str 'B' \
  --console DEBUG --console-filter SCANNER --file DEBUG --file-filter SCANNER -o log/scanner.log

# Bootstrap grammar debugging
cargo run -- validate --grammar-str 'complex: rule.' \
  --console DEBUG --console-filter BOOTSTRAP,GRAMMAR --file DEBUG -o log/bootstrap.log

# Post-hoc filtering (very token-efficient)
grep "BOOTSTRAP|" log/debug.log    # Bootstrap parsing issues
grep "SCANNER|" log/debug.log      # Character scanning
grep "GRAMMAR|" log/debug.log      # Grammar processing
grep "S(0)" log/debug.log          # Focus on position 0
grep "FAIL" log/debug.log          # All failures
```

## Strategic Debugging Approach
1. **Test known working patterns first** (like `[#20]` works, `["A"]` fails)
2. **Use temporary debug prints** for pinpointing issues (remove after fixing)
3. **Focus on parse tree structure** - grammar parsing vs input parsing are different issues
4. **Leverage test suite patterns** - find working examples to understand correct behavior

## Current Status: Non-XML Clear Cases Fixed (Updated 2026-06-04)

**Full conformance catalog**
- **Pass Rate**: 179/231 passing (77.5%)
- **Remaining**: 13 concrete failures, 39 bootstrap errors, 0 TODO
- **Latest baseline**: `cargo run -- suite --console SUMMARY --file FAILURES -o log/full-suite-naming-after.txt`
- **Focused improvement since latest full baseline**: `error` subset is now 20/20 after fixing the long-comment bootstrap guard

**Recent Fixes**
- **Long Comment Bootstrap Guard**: The per-position infinite-loop guard now scales with input length, allowing finite completion waves from long comments while preserving the small-input floor.
- **Draft Naming/Alias Syntax**: `B>X = ...` and `B>C` parse with the original rule name but serialize using the alias, including attribute references.
- **Dynamic Errors**: `AssertDynamicError` is implemented in the suite driver; XML output validation detects duplicate attributes, invalid XML names, non-XML characters, root attributes, wrong root cardinality, and reserved `xmlns` attributes.
- **Multiple Expected Results**: Multiple `<assert-xml>` entries are treated as acceptable alternatives.
- **Occurrence Marks**: A marked nonterminal reference such as `@b` now overrides the rule definition mark during output tree construction.
- **Encoded Terminal Marks**: Hidden encoded terminals such as `-#01` retain their hidden tmark during grammar conversion.

**Feature Status Summary**
- ✅ **Position-Bucketed Queue**: Working
- ✅ **Left Recursion**: Supported
- ✅ **Character Sets**: Core patterns working (`["A"]`, `[#20]`, `["0"-"9"]`, `[L]`, Unicode classes)
- ✅ **Insertion Syntax**: Core functionality complete; remaining insertion failures are ambiguous cases
- ✅ **Draft Naming/Alias Syntax**: Rule-definition and nonterminal-reference aliases are implemented
- ✅ **Dynamic Errors**: D01-D07 output validation implemented where applicable
- ✅ **Comments**: Nested comment support
- ✅ **Version Declarations**: Version mismatch detection and `ixml:state` attribute support
- ✅ **Dynamic Error Suite**: `error` subset is 20/20 passing
- ⚠️ **Ambiguous Parses**: Not yet supported; remaining `FAIL` cases are ambiguity-related
- ⚠️ **XML-form Grammar Fixtures**: Still bootstrap-erroring

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
- Minimize external dependencies; if using dependencies make sure they can readily compile to WebAssembly

### Position Semantics Upgrade:
- Changed from `pos=N` to `S(N)` notation (position N = before character N)
- Updated InputIter.get_at() for coordinate system indicating positions *between* characters
- Position 0 = before first character
