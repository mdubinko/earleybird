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

# ✅ RESOLVED: CRITICAL EARLEY PARSER BUGS

## ~~Root Cause 1: Premature Completion in Earley Parser~~ **FIXED**

**Issue**: ~~Character range parsing failed~~ **Character range parsing now works correctly**

**Problem RESOLVED**: The parser was continuing parent tasks immediately when a child completed, rather than exploring all alternatives at the current position first. This prevented patterns like `["0"-"9"]` from working because `member → string` would complete with `"0"` and trigger parent continuation before `member → range` could process the full `"0"-"9"` pattern.

## ~~Root Cause 2: Overly Aggressive Task Deduplication~~ **FIXED**

**Issue**: ~~Single string members like `["A"]` failed bootstrap grammar parsing~~ **String character sets now work correctly**

**Problem RESOLVED**: The parser was deduplicating tasks too aggressively across different parent contexts. When `member → string` and `member → range` were both viable at the same position, only one path was explored because tasks were deduplicated based solely on rule name and position, ignoring the derivation context.

**Solution Implemented**: **Blockchain-style task hashing** where each task's hash includes its parent's hash, creating a unique chain back to the root that ensures different derivation contexts get unique identifiers.

**Key Changes** (src/parser.rs):
- Modified `Task` struct to include `parent_hash: u64` and `full_hash: u64`
- Updated task creation to compute blockchain hashes: `hash(rule + position + parent_hash)`
- Tasks now display as `[parent.child]` using base58 encoding for human readability
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

**Root Cause Identified**: Blockchain hashing created infinitely growing chains for predictions. Each prediction of `expr→sum→expr` generated unique hashes `[A→B→C→D...]` instead of stable cycles `[A→B→A→B]`, preventing proper Earley deduplication.

**Fundamental Architecture Problem**: Confusion between individual alternatives (Rules) vs grouped alternatives under one name (BranchingRules). The Earley paper expects individual alternatives as separate rules, but our implementation grouped them.

**Solution Implemented**: **Alternative Indexing with Hybrid Deduplication**

### Technical Implementation:
```rust
// Added to Task struct (src/parser.rs:95-105)
pub alt_index: usize,  // which alt of this BranchingRule (0-based)

// Hybrid deduplication strategy (src/parser.rs:have_we_seen)
fn have_we_seen(&mut self, task: &Task) -> bool {
    let hash = if task.dot.is_at_start() {
        // PREDICTION: Use traditional Earley deduplication + alt_index
        let prediction_content = format!("{}[{}] at {}:{}",
            task.name, task.alt_index, task.origin, task.pos);
        utils::hash_to_u64(&prediction_content)
    } else {
        // COMPLETION/SCANNING: Use blockchain hash for derivation contexts
        task.full_hash
    };
    // ... deduplication logic
}
```

### Position Semantics Upgrade:
- Changed from `pos=N` to `S(N)` notation (position N = before character N)
- Updated InputIter.get_at() for graphics-style coordinate system
- Position 0 = before first character, matches HTML trace format

### Key Changes:
- **src/parser.rs**: Added `alt_index` field, updated all task constructors, implemented hybrid deduplication
- **src/debug.rs**: Changed trace format from `pos=N` to `S(N)` notation
- **Removed**: Unused `at_end()` method that wasn't following Rust iterator conventions

### Verification:
```bash
# Left recursion now works perfectly
cargo run -- test -g 'S: S, "a"; "b".' -i 'ba'
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

### Technical Debt - Blockchain Hash Implementation (Updated)

The hybrid approach elegantly solves both problems:
1. **Predictions**: Traditional Earley deduplication prevents infinite left-recursion
2. **Completions**: Blockchain hashing preserves different derivation contexts for ambiguous grammars

**Previous concerns resolved**:
- Hash computation performance: Only used for completions/scanning now
- Equality semantics: Not an issue with hybrid approach
- Storage efficiency: `alt_index` is much more compact than full blockchain chains

**Priority**: Technical debt is now minimal - the hybrid approach is both correct and efficient.
