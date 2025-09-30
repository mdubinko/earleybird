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

# CLI Commands

## parse - Parse input using ixml grammar
```bash
# Parse files
cargo run -- parse -g grammar.ixml -i input.txt

# Parse strings (great for quick testing)
cargo run -- parse --grammar-str 'A: "a".' --input-str 'a'

# Mix file and string
cargo run -- parse -g grammar.ixml --input-str 'test input'
```

## validate - Validate ixml grammar (bootstrap parsing)
```bash
# Validate grammar file
cargo run -- validate -g grammar.ixml

# Validate grammar string (useful for debugging bootstrap parsing issues)
cargo run -- validate --grammar-str 'A: B. B: "b".'
```

## suite - Run conformance test suite
```bash
# Run all tests
cargo run -- suite

# Filter tests by pattern
cargo run -- suite expr1              # Tests containing "expr1"
cargo run -- suite correct            # Tests in correct/ directory
```

# Debugging Workflow

Whenever generating log files or capturing trace output, put the files in the log/ directory, to avoid cluttering up the project root dir.

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
grep "✅ PASS" log/results.txt        # Find passing tests
grep "🔥 GRAMMAR ERROR" log/results.txt  # Find bootstrap grammar issues
grep "🚧 TODO" log/results.txt       # Find unimplemented test cases
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
cargo run -- parse --grammar-str 'test: [#41].' --input-str 'A' -v trace --trace-file log/debug.log

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

## Current Status: Phase 2 Position-Bucketed Queue (Updated 2025-09-30)

### 🎉 MAJOR BREAKTHROUGH: Position-Bucketed Queue Implementation

**✅ PHASE 2 COMPLETE: Dramatic Test Suite Improvement**
- **Pass Rate**: 62.4% (58/93 tests) vs Phase 1's 30.1% (28/93 tests)
- **+30 additional passing tests** - More than doubled the success rate!
- **Architecture**: Position-bucketed queue ensures proper Earley left-to-right processing
- **Key Innovation**: Tasks at position N completed before advancing to N+1

**✅ Technical Implementation**
- **PositionBucketedQueue**: `BTreeMap<usize, VecDeque<TraceId>>` for position-based task ordering
- **Debug Format**: S(N) notation shows position buckets with task counts (`S(0):3 S(1):7* S(2):1`)
- **Queue Management**: Front/back priority maintained within each position bucket
- **Iterator Support**: Clean `Display` trait implementation for debugging

**✅ Proven Impact**
- **Bootstrap Grammar Parsing**: Complex cases like `a: b. b: "x".` now work correctly
- **Left Recursion**: Continues to work perfectly (`S: S, "a"; "b".` → `<S><S>b</S>a</S>`)
- **Character Sets**: All patterns working (`["A"]`, `[#20]`, `["0"-"9"]`, `[L]`)
- **No Regressions**: All Phase 1 functionality preserved

### Character Sets Status
- ✅ Single hex: `[#20]` works
- ✅ Hex ranges: `[#41-#46]` works
- ✅ String members: `["A"]` works
- ✅ Unicode classes: `[L]`, `[Nd]`, `[Mn]`, `[Zs]` working

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

### Position Semantics Upgrade:
- Changed from `pos=N` to `S(N)` notation (position N = before character N)
- Updated InputIter.get_at() for graphics-style coordinate system
- Position 0 = before first character, matches HTML trace format

### Current Problems with Debugging
- **Temporary code pollution**: Adding `eprintln!` statements directly in source code that must be manually removed
- **Mixed output streams**: Trace output and debug messages interleaved in stderr, making analysis difficult
- **No granular control**: Can't filter debug output without code changes
- **Manual correlation**: Must manually grep and correlate related events across parsing phases

### Proposed Debug Infrastructure

#### 1. Structured Debug Macros
Replace manual `eprintln!` with structured macros:
```rust
debug_dedup!("Skipping duplicate", task);
debug_predict!("Creating prediction", parent_task, child_name);
debug_queue!("Adding to queue", task, queue_size);
```

Auto-include context: arena size, queue size, parsing phase, parent relationships, timestamps

#### 2. Debug Categories with Levels
Enable turning on/off different categories of message in addition to level filtering
```
DEDUP:TRACE - Show all deduplication decisions
PREDICT:DEBUG - Show prediction creation but not internal details
QUEUE:INFO - Show only major queue operations
COMPLETE:TRACE - Show all completion operations
```

#### 3. Contextual Information
Automatically include:
- Task genealogy (parent → child chains)
- Parsing phase indicators
- Queue state snapshots
- Cross-references between related operations

**Priority**: High - This infrastructure would dramatically improve debugging efficiency for complex parsing issues
