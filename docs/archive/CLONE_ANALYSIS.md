# Clone Analysis Report

Generated with `cargo llvm-lines --lib --release | grep -i clone`

## Top Clone Hotspots (by LLVM IR lines)

### High Impact (100+ lines)
1. **Grammar::clone** - 102 lines (0.1% of total)
   - Location: src/grammar.rs
   - Used in: Parser::parse_with_session (line 583: `let g = self.grammar.clone()`)
   - Recommendation: Consider borrowing instead of cloning in hot path

2. **Vec<T>::clone** - 104 lines across 8 copies (0.1% of total)
   - Generic vector cloning throughout codebase
   - Check if any can be replaced with borrowing or moving

### Medium Impact (50-100 lines)
3. **MatchRec::clone** - 97 lines
   - Location: src/parser.rs
   - Part of DotNotation.already_matched: Vec<MatchRec>
   - Used when advancing dot cursor

4. **Factor::clone** - 94 lines
   - Location: src/grammar.rs
   - Grammar factor cloning (terminals, nonterminals)

5. **Task::clone** - 76 lines
   - Location: src/parser.rs
   - Used in: test_inspect_trace (line 927: `self.traces.arena.clone()`)
   - Note: Only in test code, not hot path

6. **SeqBuilder::clone** - 73 lines
   - Location: src/grammar.rs
   - Grammar building phase

7. **CharMatcher::clone** - 79 lines
   - Location: src/grammar.rs
   - Character set matching

### Low Impact (< 50 lines)
- DotNotation::clone - 39 lines
- SmolStr::clone - 17 lines (efficient, uses Arc internally)
- BranchingRule::clone - 16 lines
- Rule::clone - 6 lines

## HashMap Cloning
- **HashMap cloning** - 600 lines across 3 copies in hashbrown
- Used heavily for Grammar definitions HashMap

## Key Findings

### 1. Grammar Clone in Hot Path
**Problem:**
```rust
// src/parser.rs:583
let g = self.grammar.clone();
```

This clones the entire grammar (definitions HashMap, nullability cache, etc.) on **every parse**.

**Why it exists:**
Avoids borrow contention on `self` (see comment: "help avoid borrow-contention on *self")

**Impact:**
- For xpath test: Grammar is cloned every time (7KB grammar → full HashMap clone)
- Includes cloning 70+ rules with all factors, character sets, etc.

**Potential Fix:**
- Refactor to use borrows throughout parsing
- Or use Arc<Grammar> to make clones cheap
- Or restructure Parser to separate mutable state from grammar

### 2. Test Code Cloning
**Not a concern:**
```rust
// src/parser.rs:927
self.traces.arena.clone()  // Only in test_inspect_trace
```
This is only called in test code, not production parsing.

### 3. SmolStr is Efficient
SmolStr clones are only 17 lines - it's Arc-based, so clones are cheap (just ref count increment).

## Recommendations (Priority Order)

### 🔥 High Priority
1. **Remove Grammar clone in parse loop**
   - Current: `let g = self.grammar.clone()` on every parse
   - Target: Use `&self.grammar` throughout
   - Expected Impact: ~100 lines saved, faster parsing

### ⚠️ Medium Priority
2. **Review MatchRec/DotNotation cloning**
   - Check if DotNotation.already_matched needs to be cloned when advancing
   - Maybe use reference counting or arena allocation

3. **Factor cloning during grammar construction**
   - Review if all Factor clones are necessary
   - Might be acceptable during bootstrap (one-time cost)

### ℹ️ Low Priority
4. **Vec cloning patterns**
   - Audit Vec<T>::clone calls to see if any can be moves
   - Not urgent - only 104 lines total across 8 instances

## Tools for Further Analysis

### Runtime Clone Detection (DHAT)
```bash
# Install
cargo install dhat

# Add to Cargo.toml:
[dependencies]
dhat = "0.3"

# Add to main.rs:
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    let _profiler = dhat::Profiler::new_heap();
    // ... your code
}

# Run and view results
cargo run --release
```

### Allocation Profiling
```bash
# macOS
cargo instruments -t alloc --release -- parse -g xpath.ixml -i input.txt

# Shows all allocations including clones
```

### Flamegraph
```bash
cargo flamegraph --release -- parse -g xpath.ixml -i input.txt
# Will show clone calls in the flamegraph
```

## Performance Context

From xpath test (7KB grammar, 8 byte input):
- **411,884 tasks created** in 18 seconds
- **~20K tasks/second** throughput
- Grammar clone happens once per parse (2 times for full test: bootstrap + target)

The Grammar clone is happening **2 times total** (not 411K times), so it's not the bottleneck. The real issue is the 411K tasks being created, not cloning overhead.

**Conclusion:** Clone optimization is good hygiene, but **task avoidance** is the bigger win.
