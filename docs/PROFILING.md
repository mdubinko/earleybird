# Performance Profiling Guide for Earleybird

## Quick Performance Checks

### Baseline Timing
```bash
# Time specific test
time cargo run --release -- suite ambiguous/lf2 --console NONE --file NONE

# Time full suite
time cargo run --release -- suite correct --console NONE --file NONE

# Time with hyperfine (install: brew install hyperfine)
hyperfine 'cargo run --release -- suite ambiguous --console NONE --file NONE'
```

## macOS Profiling Tools

### 1. cargo-instruments (Recommended for macOS)

**Install:**
```bash
cargo install cargo-instruments
```

**CPU Time Profiling:**
```bash
# Profile a specific test
cargo instruments -t time --release -- suite ambiguous/lf2 --console NONE --file NONE

# Opens in Instruments.app with visual timeline
# Shows hot functions, call stacks, CPU usage over time
```

**Memory Allocation Profiling:**
```bash
cargo instruments -t alloc --release -- suite ambiguous --console NONE --file NONE

# Shows allocations, deallocations, memory growth
# Identifies memory leaks and allocation hotspots
```

**Available Templates:**
```bash
instruments -s devices  # List available profiling templates
```

Common templates:
- `time` - CPU time profiler
- `alloc` - Allocations and memory
- `sys` - System calls
- `io` - File I/O activity

### 2. cargo-flamegraph (Cross-Platform)

**Install:**
```bash
cargo install flamegraph
```

**Generate Flamegraph:**
```bash
# Creates flamegraph.svg in current directory
cargo flamegraph --release -- suite ambiguous --console NONE --file NONE

# Open in browser
open flamegraph.svg
```

**Read Flamegraph:**
- Width = time spent in function (wider = more CPU time)
- Height = call stack depth
- Click to zoom into specific functions
- Color = random (for visual distinction)

### 3. Xcode Instruments GUI

**Profile Release Binary:**
```bash
# Build release binary
cargo build --release

# Launch Instruments directly
instruments -t "Time Profiler" target/release/eb suite lf2 --console NONE --file NONE

# Or open in GUI
open -a Instruments target/release/eb
```

## Optimization Workflow

### Step 1: Identify Hotspots
```bash
# Profile with time profiler
cargo instruments -t time --release -- suite ambiguous --console NONE --file NONE

# Look for:
# - Functions taking >5% of total time
# - Unexpected allocations in hot paths
# - Recursive calls without memoization
```

### Step 2: Analyze Specific Function
```bash
# Add manual timing to specific function
use std::time::Instant;

let start = Instant::now();
// ... code to profile ...
eprintln!("Function took: {:?}", start.elapsed());
```

### Step 3: Common Performance Issues in Earleybird

**Potential Hotspots:**
1. **Task Deduplication** (src/parser.rs)
   - Hash computation in `have_we_seen()`
   - HashSet lookups in hot loop

2. **Queue Operations** (src/parser.rs)
   - PositionBucketedQueue insertions
   - VecDeque operations

3. **Grammar Lookups** (src/grammar.rs)
   - HashMap lookups for rule definitions
   - Nullability cache misses

4. **String Allocations**
   - SmolStr conversions
   - String formatting in debug output

5. **Tree Walking** (src/parser.rs)
   - unpack_parse_tree_internal recursion
   - Arena traversals

### Step 4: Benchmarking Framework

**Add to Cargo.toml:**
```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "parser_bench"
harness = false
```

**Create benches/parser_bench.rs:**
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use earleybird::{Grammar, Parser};

fn bench_simple_parse(c: &mut Criterion) {
    let grammar = Grammar::from_ixml_str(r#"S = "a", "b", "c"."#).unwrap();
    let mut parser = Parser::new(grammar);

    c.bench_function("simple_parse", |b| {
        b.iter(|| {
            parser.parse(black_box("abc"))
        });
    });
}

fn bench_left_recursion(c: &mut Criterion) {
    let grammar = Grammar::from_ixml_str(r#"S = S, "a" | "b"."#).unwrap();
    let mut parser = Parser::new(grammar);

    c.bench_function("left_recursion", |b| {
        b.iter(|| {
            parser.parse(black_box("ba"))
        });
    });
}

criterion_group!(benches, bench_simple_parse, bench_left_recursion);
criterion_main!(benches);
```

**Run Benchmarks:**
```bash
cargo bench

# Opens HTML report in target/criterion/report/index.html
```

## Quick Wins Checklist

- [ ] Profile with `cargo instruments -t time` to identify hotspots
- [ ] Check for unnecessary clones (use `#[derive(Clone)]` sparingly)
- [ ] Verify SmolStr is used for frequently-cloned strings
- [ ] Ensure debug output is behind `if debug_enabled()` checks
- [ ] Look for allocations in hot loops
- [ ] Consider caching frequently computed values
- [ ] Check for redundant hash computations

## Debug vs Release Performance

**Always profile in --release mode!**

```bash
# Debug build (5-100x slower, not representative)
cargo run -- suite ambiguous --console NONE --file NONE

# Release build (actual performance)
cargo run --release -- suite ambiguous --console NONE --file NONE
```

**Debug builds have:**
- No optimizations
- Bounds checking on every array access
- No function inlining
- Full debug symbols

## Current Performance Baseline (2025-10-02)

```
Single ambiguous test (ambig/ambig): ~0.05s
Simple lf2 test: ~0.05s
Full 'correct' suite (93 tests): TBD
Full 'ambiguous' suite (15 tests): TBD
```

## Memory Profiling

```bash
# Check for memory leaks
cargo instruments -t leaks --release -- suite ambiguous --console NONE --file NONE

# Track allocations
cargo instruments -t alloc --release -- suite ambiguous --console NONE --file NONE
```

## Advanced: Custom Profiling Points

```rust
// Add to critical sections
#[cfg(feature = "profiling")]
use puffin::profile_scope;

pub fn parse(&mut self, input: &str) -> Result<Arena<Content>, ParseError> {
    #[cfg(feature = "profiling")]
    profile_scope!("parse");

    // ... parsing code ...
}
```

Then enable with:
```toml
[features]
profiling = ["puffin"]

[dependencies]
puffin = { version = "0.16", optional = true }
```

## Clone Detection

### cargo-llvm-lines (Best for Clone Analysis)

Shows how much code is generated for each function, revealing clone hotspots:

```bash
# Install
cargo install cargo-llvm-lines

# Analyze library
cargo llvm-lines --lib --release | grep -i clone

# Analyze binary
cargo llvm-lines --bin eb --release | head -50
```

**Output shows:**
- Lines of LLVM IR generated per function
- Number of copies (generic instantiations)
- Identifies expensive clones

See `CLONE_ANALYSIS.md` for detailed clone analysis of earleybird.

**Key Findings:**
- Grammar::clone - 102 lines (used in parse loop)
- MatchRec::clone - 97 lines (dot advancement)
- Factor::clone - 94 lines (grammar construction)

### Runtime Allocation Profiling

```bash
# macOS - Instruments Allocations
cargo instruments -t alloc --release -- parse -g xpath.ixml -i input.txt

# Shows:
# - Total allocations
# - Clone-related allocations
# - Memory growth over time
```

## Resources

- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [cargo-instruments GitHub](https://github.com/cmyr/cargo-instruments)
- [flamegraph.rs](https://github.com/flamegraph-rs/flamegraph)
- [Criterion Benchmarking](https://bheisler.github.io/criterion.rs/book/)
- [cargo-llvm-lines](https://github.com/dtolnay/cargo-llvm-lines)
