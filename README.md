# EarleyBird
Experimental implementation of ixml in Rust. Currently targeting the 1.0 spec.

# Usage

## Running

For development, use `cargo run --` (recommended):

### Parse files

```bash
cargo run -- parse -g grammar.ixml -i input.txt
```

### Quick inline testing (useful for development)

```bash
cargo run -- parse --grammar-str 'rule: "a" | "b".' --input-str 'a'
```

### Run test suite

```bash
# Standard usage - summary to console, failures to file
cargo run -- suite --console SUMMARY --file FAILURES

# Focus on specific test categories; can have separate outputs to stdout vs file
cargo run -- suite syntax --console SUMMARY --file NONE
cargo run -- suite correct --console SUMMARY --file FAILURES -o results.txt

# Debug specific issues with category filtering
cargo run -- suite syntax --console DEBUG --console-filter BOOTSTRAP,GRAMMAR --file NONE

# Silent operation with full logging
cargo run -- suite --console NONE --file ALL -o full-results.txt

# Full release conformance baseline with shell timing and per-test CSV timings
/usr/bin/time -p cargo run --release -- suite --console SUMMARY --file FAILURES \
  -o log/codeberg-release.txt

# Focused release run while investigating one suite area
cargo run --release -- suite ambiguous --console SUMMARY --file FAILURES \
  -o log/ambiguous-release.txt

# Or with environment variables for debugging:
RUST_LOG=info RUST_BACKTRACE=1 cargo run -- suite
```

When `-o` is supplied, suite timings are written beside the chosen output stem. For
example, `-o log/codeberg-release.txt` also writes
`log/codeberg-release.timings.csv`.

### Run performance benchmarks

```bash
# Quick synthetic scaling check
cargo run --release -- bench --sizes 8,16,32,64,128 --reps 3 \
  --csv log/bench-synthetic.csv

# Focus one synthetic family
cargo run --release -- bench --filter left --sizes 16,32,64 --reps 5 \
  --csv log/bench-left.csv

# Run all heavy corpus benchmarks
cargo run --release -- bench --heavy --csv log/bench-heavy.csv

# Run one heavy corpus benchmark; useful for Unicode grammar-build work
cargo run --release -- bench --heavy --filter unicode_version \
  --csv log/bench-unicode-version.csv

# Add parser task statistics when inspecting a single benchmark
cargo run --release -- bench --heavy --filter ixml_self --stats \
  --csv log/bench-ixml-self-stats.csv
```

Benchmark CSV columns are:
`kind,name,n,input_len,min_parse_ms,median_parse_ms,build_ms,parse_ms,status`.
Synthetic rows fill the parse timing columns; heavy rows fill `build_ms` and
`parse_ms` separately.

Alternatively, build the `eb` binary first:

```bash
cargo build --release
./target/release/eb parse -g grammar.ixml -i input.txt
./target/release/eb parse --grammar-str 'rule: "a" | "b".' --input-str 'a'
./target/release/eb suite --console SUMMARY --file NONE
./target/release/eb bench --heavy --filter unicode_version --csv log/bench.csv
```

The test suite expects the official ixml repo to be available as a symlink at `ixml/`.

## Debugging and Transparency

This implementation includes comprehensive debugging tools to aid in conformance work and troubleshooting.

### New Debug System (2025)

All commands now support granular debug control with console and file output:

**Console Levels**: DEBUG, INFO, SUMMARY, WARNING, ERROR, ALL, NONE, OFF
**File Levels**: DEBUG, INFO, SUMMARY, WARNING, ERROR, ALL, NONE, OFF, FAILURES
**Categories**: BOOTSTRAP, QUEUE, SCANNER, OUTPUT, PREDICT, COMPLETE, DEDUP, GRAMMAR

```bash
# Basic usage - summary to console, no file output
cargo run -- parse -g grammar.ixml -i input.txt -f XML --console SUMMARY --file NONE

# Debug bootstrap grammar issues
cargo run -- validate --grammar-str 'test: "a".' --console DEBUG --console-filter BOOTSTRAP,GRAMMAR

# Focus on parser internals with file logging
cargo run -- parse -g complex.ixml -i input.txt -f XML \
  --console INFO --console-filter SCANNER,QUEUE \
  --file DEBUG -o debug.log

# Multiple categories for complex debugging
cargo run -- parse --grammar-str 'expr: term, ("+", term)*. term: "a".' --input-str 'a+a' \
  --console DEBUG --console-filter BOOTSTRAP,PREDICT,COMPLETE

# Silent operation with detailed file logging
cargo run -- suite correct --console NONE --file DEBUG -o trace.log

# Position-specific debugging (legacy support)
cargo run -- parse -g test.ixml -i input.txt --console DEBUG --debug-pos 0
```

**Category Descriptions:**
- **BOOTSTRAP**: Grammar parsing (ixml → internal representation)
- **QUEUE**: Task queue operations (position-bucketed Earley queue)
- **SCANNER**: Character matching and advancement
- **OUTPUT**: XML formatting and tree conversion
- **PREDICT**: Earley prediction operations
- **COMPLETE**: Earley completion operations
- **DEDUP**: Task deduplication
- **GRAMMAR**: Grammar processing and validation

### Debug Output Structure

The logging system provides structured, component-specific output:

- **`BOOTSTRAP|`**: Grammar parsing and bootstrap operations
- **`QUEUE|`**: Task queue and position management
- **`SCANNER|`**: Character matching and scanning
- **`OUTPUT|`**: XML tree formatting and conversion
- **`PREDICT|`**: Earley prediction operations
- **`COMPLETE|`**: Earley completion operations
- **`DEDUP|`**: Task deduplication decisions
- **`GRAMMAR|`**: Grammar construction and validation
- **`EARLEY|`**: Legacy detailed Earley algorithm steps (trace mode)

#### Structured Format (New)

When using `--trace-file`, output uses a pipe-separated structured format for easier analysis:

- **`EARLEY|pos=n|op=OPERATION|...`**: Earley operations with key-value pairs
- **`EARLEY-FAIL|pos=n|expected=...|actual='x'`**: Parse failures with structured data

**Common Operations:**
- **`op=PREDICTOR`**: Parser predicting what nonterminal should come next
- **`op=SCANNER`**: Parser trying to match terminal characters  
- **`op=COMPLETER`**: Parser completing a rule and looking for continuations
- **`op=SCANNER-MATCH`**: Successful character matches with position advancement

**Example structured output:**
```
EARLEY|pos=0|op=PREDICTOR|task=T1|mark=|name=rule
EARLEY|pos=0|op=SCANNER|task=T2|tmark=|matcher='a'
EARLEY|pos=0|op=SCANNER-MATCH|char='a'|new_pos=1
EARLEY|pos=1|op=COMPLETER|task=T2|completed
EARLEY-FAIL|pos=1|expected='b'|actual='c'
```

**Analysis with grep:**
```bash
# Focus on specific categories
grep "BOOTSTRAP|" debug.log
grep "SCANNER|" debug.log
grep "GRAMMAR|" debug.log

# Legacy Earley operations at position 5
grep "S(5)" debug.log

# All scanner operations (legacy format)
grep "op=SCANNER" debug.log

# Parse failures
grep "FAIL" debug.log

# Scanner matches with specific character
grep "MATCH.*'a'" debug.log
```

### Parse Failure Analysis

When parsing fails, the debug system automatically provides:
- Input position where parsing stopped
- Context around the failure point
- Expected vs actual characters
- Grammar content that was being processed

### Earley Parser Tracing

The `trace` verbosity level provides detailed step-by-step Earley algorithm debugging:

**Key Operations Traced:**
- **PREDICTOR**: When the parser predicts what nonterminal should come next
- **SCANNER**: When the parser tries to match terminal characters  
- **COMPLETER**: When the parser completes a rule and looks for continuations
- **MATCH**: Successful character matches with position advancement
- **FAIL**: Parse failures showing expected vs actual characters

**Position Filtering:**
```bash
# Only show trace at input position 0
cargo run -- test -g 'rule: "a".' -i 'abc' -v trace --debug-pos 0

# Only show trace at input position 2
cargo run -- test -g 'rule: "a", "b", "c".' -i 'abc' -v trace --debug-pos 2
```

This is essential for conformance debugging as full traces can be thousands of lines even for simple grammars.

**External Trace Files:**
The `--trace-file` option is especially useful for workflows to avoid overwhelming the terminal:

```bash
cargo run -- parse -g complex.ixml -i large-input.txt -v trace --trace-file trace.log
# Then analyze the trace file separately:
grep "EARLEY-FAIL" trace.log | head -10
grep "pos=42" trace.log
```

### Implementation Notes for Developers

The debug infrastructure is centralized in `src/debug.rs`:

- **`DebugLevel`**: Enum controlling output verbosity (Off, Basic, Detailed, Trace)
- **`DebugConfig`**: Configuration with position filtering, category filtering, and trace file output
- **Category-specific functions**: `debug_bootstrap()`, `debug_queue()`, `debug_scanner()`, etc.
- **Category-specific macros**: `debug_bootstrap!()`, `debug_queue!()`, `debug_scanner!()`, etc.
- **Legacy macros**: `debug_basic!()`, `debug_detailed!()`, `debug_trace!()`, `debug_earley!()`
- **Failure analysis**: `debug_parse_failure()` for detailed error context

Key benefits:
- **Granular control**: Both level AND category must be enabled for output
- **Case-insensitive CLI**: Accepts "debug", "DEBUG", "Debug", "bootstrap", "BOOTSTRAP", etc.
- **Clean separation**: Console vs file output with independent control
- **Category filtering**: Focus on specific components (BOOTSTRAP, SCANNER, etc.)
- **Structured output**: Pipe-separated format for easy analysis
- **No code pollution**: Centralized debug system eliminates scattered `eprintln!` statements

### Advanced Usage Examples

```bash
# Bootstrap grammar debugging workflow
cargo run -- validate -g complex.ixml --console DEBUG --console-filter BOOTSTRAP,GRAMMAR --file NONE

# Parser performance analysis
cargo run -- parse -g grammar.ixml -i large-input.txt \
  --console SUMMARY --file DEBUG --file-filter QUEUE,DEDUP --output performance.log

# Test suite debugging with focused output
cargo run -- suite syntax --console INFO --console-filter BOOTSTRAP \
  --file DEBUG --file-filter BOOTSTRAP,GRAMMAR --output bootstrap-issues.log

# Character scanning issues
cargo run -- parse --grammar-str 'test: ["A"-"Z"]+.' --input-str 'Hello123' \
  --console DEBUG --console-filter SCANNER --file NONE

# Silent CI/CD runs with failure logging
cargo run -- suite --console NONE --file FAILURES -o ci-failures.txt
```

### Future Debugging Enhancements

Planned advanced debugging features:
- HTML trace viewer for step-by-step parse visualization
- Earley chart state visualization
- Parse tree diff viewer for comparing expected vs actual results
- Interactive parser stepper for debugging complex failures
- Export capabilities (JSON/GraphViz) for external analysis tools

# Future work

* more generally, performance profiling and optimization

* flamegraph profiling?

# Statement on AI Generated Code

As of May 1, 2023, no AI generated code has been used in any part of this project.

The core concepts and architecture were all built 'by hand'.

Since this is a learning project, I intend to experiment with different code generation products in the future, especially for testing and fleshing out the details of the implementation.

# Test Suite Setup

The test harness expects to locate resources from the official ixml repo in a symlinked directory called `ixml/`.

1. Clone the official ixml repository:
   ```bash
   git clone https://github.com/invisibleXML/ixml.git
   ```

2. Create a symlink in your earleybird directory:
   ```bash
   # If ixml repo is in a sibling directory:
   ln -s ../ixml .

   # Or if ixml repo is elsewhere:
   ln -s /path/to/ixml .
   ```


# References

Invisible XML: https://invisiblexml.org/

IXML Repo: https://github.com/invisibleXML/ixml

Test Suite: https://github.com/invisibleXML/ixml/tree/master/tests

Vulturine Guinea Fowl: https://en.wikipedia.org/wiki/Vulturine_guineafowl
