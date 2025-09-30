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
- ✅ String members: `["A"]` now works (bootstrap parsing improved)
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

# ✅ RESOLVED: CRITICAL EARLEY PARSER BUGS

## ~~Root Cause 1: Premature Completion in Earley Parser~~ **FIXED**

**Issue**: ~~Character range parsing failed~~ **Character range parsing now works correctly**

**Problem RESOLVED**: The parser was continuing parent tasks immediately when a child completed, rather than exploring all alternatives at the current position first. This prevented patterns like `["0"-"9"]` from working because `member → string` would complete with `"0"` and trigger parent continuation before `member → range` could process the full `"0"-"9"` pattern.

## ~~Root Cause 2: Overly Aggressive Task Deduplication~~ **FIXED**

**Issue**: ~~Single string members like `["A"]` failed bootstrap grammar parsing~~ **String character sets now work correctly**

**Problem RESOLVED**: The parser was deduplicating tasks too aggressively across different parent contexts. When `member → string` and `member → range` were both viable at the same position, only one path was explored because tasks were deduplicated based solely on rule name and position, ignoring the derivation context.

**Solution Implemented**: **Parent-hash task hashing** where each task's hash included its parent's hash, creating a unique chain back to the root that ensured different derivation contexts got unique identifiers.

**Key Changes** (src/parser.rs):
- Modified `Task` struct to include `parent_hash: u64` and `full_hash: u64`
- Updated task creation to compute parent-hash chains: `hash(rule + position + parent_hash)`
- Tasks displayed as `[parent.child]` using base58 encoding for human readability
- Binary u64 hashes used for deduplication performance

### Solution Implemented:
**Fixed COMPLETER queue management** - Parent continuations are now queued at the back (`queue_back`) instead of front (`queue_front`), ensuring all alternatives at the current position are explored before parent propagation.

**Key Change** (src/parser.rs:438):
```rust
// OLD: self.queue_front(maybe_id);  // Immediate parent continuation
// NEW: self.queue_back(maybe_id);   // Defer parent continuation
```

## ✅ Root Cause 3: Left Recursion Infinite Loops **COMPLETELY RESOLVED**

**Issue**: Left-recursive grammars like `expr: expr, "+", term; term.` caused infinite loops hitting 100,000 operation limits

**Root Cause Identified**: Parent-hash chaining created infinitely growing chains for predictions. Each prediction of `expr→sum→expr` generated unique hashes `[A→B→C→D...]` instead of stable cycles `[A→B→A→B]`, preventing proper Earley deduplication.

**Fundamental Architecture Problem**: Confusion between individual alternatives (Rules) vs grouped alternatives under one name (BranchingRules). The Earley paper expects individual alternatives as separate rules, but our implementation grouped them.

**Solution Implemented**: **Alternative Indexing with Simple Deduplication (CURRENT)**

### Technical Implementation:
```rust
// Added to Task struct (src/parser.rs:95-105)
pub alt_index: usize,  // which alt of this BranchingRule (0-based)

// Simple deduplication strategy (src/parser.rs:have_we_seen)
fn have_we_seen(&mut self, task: &Task) -> bool {
    // Use task identity hash based on name, alt_index, origin, pos, dot
    if self.hashes.contains(&task.hash) {
        true // duplicate
    } else {
        self.hashes.insert(task.hash);
        false // new task
    }
}
```

### Position Semantics Upgrade:
- Changed from `pos=N` to `S(N)` notation (position N = before character N)
- Updated InputIter.get_at() for graphics-style coordinate system
- Position 0 = before first character, matches HTML trace format

### Key Changes:
- **src/parser.rs**: Added `alt_index` field, updated all task constructors, implemented simple deduplication
- **src/debug.rs**: Changed trace format from `pos=N` to `S(N)` notation
- **Removed**: Unused `at_end()` method that wasn't following Rust iterator conventions
- **SIMPLIFIED (Current)**: Removed parent-hash system entirely, using basic identity hash only

### Verification:
```bash
# Left recursion now works perfectly
cargo run -- parse --grammar-str 'S: S, "a"; "b".' --input-str 'ba'
# Output: <S><S>b</S>a</S>

# Traces show proper alternative indexing
# S[0]: First alternative (S, "a")
# S[1]: Second alternative ("b")
```

### Impact:
- **Left recursion completely resolved**: No more infinite loops or trace size limits
- **Performance**: Hybrid approach maintains deduplication benefits while preventing infinite chains
- **Correctness**: Follows original Earley paper approach with individual alternatives
- **Diagnostics**: `expr[4]` notation makes traces much more readable

### Architecture Simplification (Current Status)

The simple deduplication approach elegantly solves the core problems:
1. **All Tasks**: Basic identity hash based on `name[alt_index] origin:pos dot` prevents infinite left-recursion
2. **Nullable Handling**: Proper nullable nonterminal advancement ensures correct Earley parsing

**Benefits of simplified approach**:
- Hash computation performance: Single hash computation per task
- Code clarity: Much simpler to understand and debug
- Storage efficiency: Single u64 hash per task, no parent-hash chains

**Priority**: Technical debt eliminated - the simple approach is both correct and efficient.

## 🚀 CURRENT STATUS (2025-09-25)

### Recent Major Simplification
- **REMOVED**: All parent-hash/blockchain complexity from Task struct and deduplication logic
- **SIMPLIFIED**: Task identity now based only on core fields: `name[alt_index] origin:pos dot`
- **MAINTAINED**: All functionality preserved - test suite still at 28/108 PASS rate
- **CLEAN**: Much simpler codebase for debugging remaining bootstrap grammar parsing issues

### Current Test Suite Performance
- **Total**: 108 tests
- **PASS**: 28 (25.9%) - Core functionality working correctly
- **FAIL**: 15 (13.9%) - Output format mismatches, mostly minor XML formatting
- **Bootstrap Errors**: 65 (60.2%) - Bootstrap grammar parsing failures, primary remaining issue
- **Parse Errors**: 0 - Input parsing with valid grammars working well

### Active Work Areas
1. **Bootstrap Grammar Parsing**: 65 tests failing due to bootstrap ixml grammar parsing issues
2. **Nullable Nonterminal Handling**: Recently fixed deduplication logic for nullable nonterminals
3. **Code Architecture**: Successfully simplified from complex parent-hash chains to simple identity hashing

## Debug Infrastructure Improvements Needed

### Current Problems with Debugging
- **Temporary code pollution**: Adding `eprintln!` statements directly in source code that must be manually removed
- **Mixed output streams**: Trace output and debug messages interleaved in stderr, making analysis difficult
- **No granular control**: Can't filter debug output without code changes
- **Manual correlation**: Must manually grep and correlate related events across parsing phases

### Proposed Debug Infrastructure

#### 1. Command-Line Debug Control

Audit what `cargo test` does

Is it helpful as currently put together? What would be better?
In particular, focus on unit testing complex & tricky sections of code

```bash
# Control debug levels and categories from CLI
cargo run -- parse --grammar-str 'grammar' --input-str 'input' --debug-level trace --debug-filter "dedup,predict"

# Clean separation of outputs
cargo run -- --trace-file trace.log --debug-file debug.log --quiet-stdout
```

#### 2. Structured Debug Macros
Replace manual `eprintln!` with structured macros:
```rust
debug_dedup!("Skipping duplicate", task);
debug_predict!("Creating prediction", parent_task, child_name);
debug_queue!("Adding to queue", task, queue_size);
```

Auto-include context: arena size, queue size, parsing phase, parent relationships, timestamps

#### 3. Debug Categories with Levels
Enable turning on/off different categories of message in addition to level filtering
```
DEDUP:TRACE - Show all deduplication decisions
PREDICT:DEBUG - Show prediction creation but not internal details
QUEUE:INFO - Show only major queue operations
COMPLETE:TRACE - Show all completion operations
```

#### 4. Better Test Integration
```bash
# Compare debug output between cases
cargo run -- suite --debug-diff failing_case working_case

# Save debug output per test automatically
cargo run -- suite --debug-archive log/suite_debug/

# Regression detection
cargo run -- suite --debug-baseline --detect-debug-changes
```

#### 5. Contextual Information
Automatically include:
- Task genealogy (parent → child chains)
- Parsing phase indicators
- Queue state snapshots
- Cross-references between related operations

**Priority**: High - This infrastructure would dramatically improve debugging efficiency for complex parsing issues
