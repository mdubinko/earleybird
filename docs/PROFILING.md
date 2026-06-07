# Performance Profiling Guide for Earleybird

## Methodology: Profile Before Taking Action

**Measure first. Do not optimize from a story about where the time goes.** The tooling
below is only useful in service of this one rule: every performance change must be
preceded by a measurement that *names the dominant cost*, and followed by a measurement
that *confirms the change moved it*. A plausible complexity argument in a code comment,
a TODO, or your own head is a hypothesis, not evidence — and on this codebase those
hypotheses have been wrong.

### Worked example (why this rule exists)

The architecture review confidently diagnosed the parser's #1 cost as an O(n^2) global
completer scan (`continuations` keyed by name with no position). The fix — position-index
the completer, drop the per-alt fan-out — was correct, landed cleanly, and kept
conformance steady (no regressions). But the profile told a different story than the prose:

| metric (bench `unicode_version`) | before | after |
| -------------------------------- | ------ | ----- |
| tasks created                    | 577,883 | 577,883 (unchanged) |
| operations                       | 543,186 | 543,186 (unchanged) |
| deduplicated (rejected attempts) | 296,836 (33%) | 117,946 (16%) |
| build time                       | ~56.2 s | ~52.4 s (~7%) |

The scan was *not* the time-dominant cost. A follow-up `--stats` run across input sizes
found the real driver: distinct Earley items grow ~O(n^2) on right-recursive grammars
(2,274 → 8,642 → 33,666 tasks at n = 64/128/256, **0% dedup** — genuinely distinct
items), with time tracking the count. Two different workloads turned out to have two
different bottlenecks (algorithmic item count vs. per-operation constant cost). Guessing
would have optimized the wrong one.

### The loop

1. **Reproduce** the cost with a repeatable command (`bench`, or a focused `suite`
   filter). Pick the *smallest* input that still shows the pathology.
2. **Attribute** it before writing any fix:
   - Is it count or per-unit cost? Compare `--stats` (tasks/operations) across two input
     sizes. If counts grow super-linearly, it's algorithmic; if counts are ~linear but
     time isn't, it's per-operation constant cost (clones, hashing, scanning).
   - Where in the call graph? `cargo instruments -t time` or `cargo flamegraph` (below).
   - Confirm the split between candidate causes with numbers, not ratios of intuition.
3. **Change** the single thing the measurement implicated.
4. **Re-measure** the same command. State the delta honestly — including "no change" or
   "smaller than expected." A fix that doesn't move the metric it targeted is a signal
   the diagnosis was wrong, not a rounding error to wave away.
5. **Record** the before/after numbers in the commit message and, for architectural
   findings, in `TODO.txt` / the project notes — so the next person inherits evidence,
   not a story.

### Tells that you're guessing, not profiling

- The justification is a complexity claim (`O(n^2)`) with no measured input-size curve.
- You can't say which is bigger: item count or per-item cost.
- "This clone is obviously expensive" — without an allocation profile or `llvm-lines`.
- The plan optimizes a structure you haven't seen dominate a flamegraph.

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

## Current Performance Baseline (2026-06-06, after the tree-extractor span-index fix)

```
Conformance: 890/890 passing (906 total; 16 skipped, Unicode version ≠ 14.0).
  misc/sample.grammar.12/g12.c05 is hyper-ambiguous; scored conformant via a local
  suite override (accepted + flagged ambiguous), not exact tree match. A specific
  enumerated tree would still need SPPF/forest sharing. See tests/suite-overrides.xml.
Full release suite: 29.6s wall across 890 tests  (was ~790s — ~27x, extractor fix).
Slowest suite tests (were 122-204s; the grammar.41 trio was extraction-bound too):
  misc/sample.grammar.41ter/grammar-test: 2.13s   (was ~204s)
  misc/sample.grammar.41bis/grammar-test: 1.84s   (was ~156s)
  misc/sample.grammar.41/grammar-test:    1.45s   (was ~122s)
  unicode-version-14-diagnostic:          1.45s   (was ~52s)
  correct/ixml tests/xpath/xpath:         1.02s   (was ~33s)

Heavy benchmark sample (release `bench --heavy --stats`):
  unicode_version: build 1450ms / parse 3.1ms   (was 51794ms; unpack 50.6s -> 114ms)
  ixml_self:       build 284ms  / parse 252ms    (was 1295/1264ms; unpack 1046 -> 17ms)
  Phase split now: parse loop ~91% of unicode build, ~93% of ixml_self — i.e. the
  remaining cost has moved INTO the Earley loop (where Leo/clone fixes apply).

Synthetic scaling (med_ms, --reps 3): super-linear on completer-stress families
(unchanged by the extractor fix — these are loop-bound, not extraction-bound).
  right_recursion: ~O(n^2.6)   repeat_plus: ~O(n^2.5)
  left_recursion / nested: ~linear   ambiguous: ~O(n^3) (inherent)
```

The extractor fix (span-indexed `completed_trace`; see TODO.txt) removed the dominant
real-world cost. The next lever is the Earley parse loop itself (now ~91% of the heavy
builds): the O(n^2) right-recursion item count (Leo's optimization) and per-op clones.

The dominant remaining cost is TREE EXTRACTION (`unpack_parse_tree` /
`filter_completed_trace`), *not* the Earley parse loop. A broad re-profile on
2026-06-06 (release `bench --stats`, per-phase wall-time) found:

```
unicode_version build 51.8s:  parse loop ~1.2s (2.3%),  unpack ~50.6s (97.7%),  compile 0.8ms
ixml_self    build/parse:      parse loop ~17-18%,        unpack ~82%
```

`filter_completed_trace` linear-scans the whole `completed_trace` per output-tree node
(461M comparisons on a 2KB grammar) — see TODO.txt "De-quadratic the tree extractor".
The parse-loop algorithmic fixes (O(n^2) item count via Leo; clone removal) target only
the ~2–17% that is the loop. This is the methodology's worked example a SECOND time: the
plausible target (parser core) was not the measured-dominant one (extractor). Use the
exact invocations above when refreshing the baseline, and write outputs under `log/` so
the project root stays clean.

### Per-phase wall-time breakdown (`--stats`)

`--stats` (on `bench` and `parse`) now prints a per-phase wall-time breakdown in
addition to task counts: the parse-loop arms (predict/scan/complete/insert), a loop
subtotal, and the `unpack` (tree-extraction) phase. On heavy `bench` cases the grammar
build is itself a bootstrap parse, so its breakdown prints too (via
`Grammar::from_ixml_str_profiled`). Phase timing is opt-in and zero-cost when off, and
is deliberately NOT enabled by `from_ixml_str_detailed`, so `suite` is not flooded.
Example:

```bash
cargo run --release -- bench --heavy --filter ixml_self --stats
cargo run --release -- parse -g grammar.ixml -i input.txt --stats
```

### Exact allocation counts (`alloc-count` feature)

For clone-removal work, wall-time is noisy but allocation counts are exact and
diffable. Build with the optional `alloc-count` feature to install a counting
global allocator (`src/alloc_count.rs`); it is off by default and zero-cost when
off. With it on, `--stats` adds an `≈ alloc` line (allocs / reallocs / deallocs /
bytes) to the per-parse phase breakdown, and `suite` prints a whole-suite
allocation total in its `=== TIMING ===` block.

```bash
# Per-parse allocation delta (e.g. the unicode_version grammar-build loop)
cargo run --release --features alloc-count -- bench --heavy --filter unicode_version --stats

# Whole-suite allocation total
cargo run --release --features alloc-count -- suite --console SUMMARY --file NONE
```

Report clone-removal changes as an allocation delta (`6,145,836 -> N allocs`)
in the commit message alongside the ns/call numbers — it is the honest signal
when the wall-time delta is within run-to-run noise. For per-call-site
attribution (which clone dominates), reach for `dhat` or `cargo instruments -t
alloc` below.

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

See `archive/CLONE_ANALYSIS.md` for detailed clone analysis of earleybird.

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
