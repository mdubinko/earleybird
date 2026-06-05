# Performance Profiling Guide for Earleybird

## Quick Performance Checks

### Conformance Suite Baseline

```bash
# Full release conformance run, with shell timing and failure log
/usr/bin/time -p cargo run --release -- suite --console SUMMARY --file FAILURES \
  -o log/codeberg-release.txt

# Focused release run while investigating one suite area
/usr/bin/time -p cargo run --release -- suite ambiguous --console SUMMARY --file FAILURES \
  -o log/ambiguous-release.txt

# Quiet timing when console detail would distort a benchmark
/usr/bin/time -p cargo run --release -- suite syntax --console NONE --file NONE
```

When `-o` is supplied, the suite runner writes a timing CSV beside the output
stem. For example, `-o log/codeberg-release.txt` produces:

```text
log/codeberg-release.txt
log/codeberg-release.timings.csv
```

The timing CSV schema is:

```csv
millis,name
```

### Built-In Bench Command

Use `bench` for repeatable parser scaling checks and for the known heavy corpus
cases. It is quiet by default; add `--stats` when parser task counts are useful.

```bash
# Synthetic scaling baseline
cargo run --release -- bench --sizes 8,16,32,64,128 --reps 3 \
  --csv log/bench-synthetic.csv

# Narrow a synthetic family by substring
cargo run --release -- bench --filter left --sizes 16,32,64 --reps 5 \
  --csv log/bench-left.csv

# All heavy corpus cases
cargo run --release -- bench --heavy --csv log/bench-heavy.csv

# One heavy corpus case; this keeps long runs targeted
cargo run --release -- bench --heavy --filter unicode_version \
  --csv log/bench-unicode-version.csv

# Inspect one case with parser task statistics
cargo run --release -- bench --heavy --filter ixml_self --stats \
  --csv log/bench-ixml-self-stats.csv
```

Benchmark CSV schema:

```csv
kind,name,n,input_len,min_parse_ms,median_parse_ms,build_ms,parse_ms,status
```

Synthetic rows fill `n`, `input_len`, `min_parse_ms`, and
`median_parse_ms`. Heavy rows leave the synthetic timing columns empty and fill
`build_ms` and `parse_ms` separately, because grammar construction and input
parsing often have very different cost profiles.

### External Timing Tools

```bash
# Time with hyperfine (install: brew install hyperfine)
hyperfine 'cargo run --release -- suite ambiguous --console NONE --file NONE'
hyperfine 'cargo run --release -- bench --filter left --sizes 64 --reps 1'
```

## macOS Profiling Tools

### 1. cargo-instruments (Recommended for macOS)

**Install:**
```bash
cargo install cargo-instruments
```

**CPU Time Profiling:**
```bash
# Profile a specific suite filter
cargo instruments -t time --release -- suite ambiguous/lf2 --console NONE --file NONE

# Profile a focused benchmark
cargo instruments -t time --release -- bench --filter left --sizes 128 --reps 1

# Opens in Instruments.app with visual timeline
# Shows hot functions, call stacks, CPU usage over time
```

**Memory Allocation Profiling:**
```bash
cargo instruments -t alloc --release -- suite ambiguous --console NONE --file NONE

cargo instruments -t alloc --release -- bench --heavy --filter ixml_self

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

# Or focus a benchmark
cargo flamegraph --release -- bench --heavy --filter ixml_self

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

## Current Performance Baseline (2026-06-05)

```
Current local Codeberg catalog: 232/232 passing
Full release suite: 127.31s suite wall time / 133.64s process real time
Slowest suite tests:
  correct/ixml tests/unicode-version-check/unicode-version-14-diagnostic: 42.32s
  correct/ixml tests/xpath/xpath: 25.93s

Heavy benchmark sample:
  unicode_version: build 39830.989ms / parse 2.695ms
  ixml_self: build 1235.203ms / parse 993.118ms
```

Use the exact invocations above when refreshing the baseline, and write outputs
under `log/` so the project root stays clean.

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
