# EarleyBird
Experimental implementation of ixml in Rust. Currently targeting the 1.0 spec.

# Usage

## Running

The `eb` CLI has several subcommands:

### Parse files

```bash
eb parse -g grammar.ixml -i input.txt
```

### Quick inline testing (useful for development)

```bash
eb test -g 'rule: "a" | "b".' -i 'a'
```

### Run test suite

```bash
eb suite
# Or with legacy path syntax:
RUST_LOG=info RUST_BACKTRACE=1 cargo run -- suite ../../ixml/tests/correct/test-catalog.xml
```

The test suite is a git submodule in `ixml/` that contains the reference implementation.

## Debugging and Transparency

This implementation includes comprehensive debugging tools to aid in conformance work and troubleshooting.

### Verbosity Levels

Both `parse` and `test` commands support verbosity levels via `-v` or `--verbose`:

- **`off`** (default): Silent operation, only shows final output or errors
- **`basic`**: Shows input files/strings being processed and basic error context
- **`detailed`**: Adds grammar statistics, parsing success confirmations, and enhanced error details  
- **`trace`**: Detailed Earley parser step-by-step tracing with position filtering support

Examples:
```bash
# Quick debugging with inline strings
eb test -g 'expr: term, ("+", term)*. term: "a".' -i 'a+a' -v detailed

# Detailed file-based parsing
eb parse -g examples/simple.ixml -i examples/simple.txt -v detailed

# Basic debugging for parse failures
eb test -g 'test: "exact".' -i 'wrong' -v basic

# Trace Earley parser operations at specific position
eb test -g 'rule: "a".' -i 'a' -v trace --debug-pos 0

# Full trace (verbose - use with caution)
eb test -g 'rule: "a".' -i 'a' -v trace

# External trace file
eb test -g 'rule: "a".' -i 'a' -v trace --trace-file earley.log
eb parse -g grammar.ixml -i input.txt -v trace --trace-file debug.log
```

### Debug Output Structure

The logging system provides structured, component-specific output:

- **`[GRAMMAR]`**: Grammar construction and validation
- **`[PARSER]`**: High-level parsing operations  
- **`[EARLEY]`**: Detailed Earley algorithm steps (trace mode)
- **`[EARLEY@n]`**: Earley operations at specific input position n (legacy format)
- **`[EARLEY-FAIL@n]`**: Parse failures at position n showing expected vs actual (legacy format)

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
# All operations at position 5
grep "pos=5" earley.log

# All scanner operations
grep "op=SCANNER" earley.log

# All parse failures
grep "EARLEY-FAIL" earley.log

# Scanner matches with specific character
grep "op=SCANNER-MATCH.*char='a'" earley.log
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
eb test -g 'rule: "a".' -i 'abc' -v trace --debug-pos 0

# Only show trace at input position 2  
eb test -g 'rule: "a", "b", "c".' -i 'abc' -v trace --debug-pos 2
```

This is essential for conformance debugging as full traces can be thousands of lines even for simple grammars.

**External Trace Files:**
The `--trace-file` option is especially useful for workflows to avoid overwhelming the terminal:

```bash
eb parse -g complex.ixml -i large-input.txt -v trace --trace-file trace.log
# Then analyze the trace file separately:
grep "EARLEY-FAIL" trace.log | head -10
grep "pos=42" trace.log
```

### Implementation Notes for Developers

The debug infrastructure is centralized in `src/debug.rs`:

- **`DebugLevel`**: Enum controlling output verbosity
- **`DebugConfig`**: Configuration with position filtering, failure-only modes, and trace file output
- **Debug macros**: `debug_basic!()`, `debug_detailed!()`, `debug_trace!()`
- **Component-specific macros**: `debug_grammar!()`, `debug_parser!()`, `debug_earley!()`
- **Position-aware macros**: `debug_earley_pos!()`, `debug_earley_fail!()`
- **Failure analysis**: `debug_parse_failure()` for detailed error context

Key benefits:
- No scattered `println!` or `eprintln!` statements throughout codebase
- Debug levels controlled centrally without conditional bloat in main code
- Position filtering prevents trace output overload
- External trace files prevent clutter in debug output (use grep instead!)
- Structured pipe-separated format for easy grep analysis
- Easy to add new debugging without changing existing code structure

### Bootstrap Implementation Strategy

**Comment Handling**: Comments (`{...}`) are preprocessed and stripped before grammar parsing to avoid performance issues. Since comments are fully nestable and never appear in output XML, removing them during preprocessing eliminates the character-by-character parsing overhead that would generate massive trace output. This approach maintains semantic correctness while dramatically improving parse performance for grammars with extensive documentation.

### Future Debugging Enhancements (Phase 2)

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

Test suite:
The test harness expects to locate resources from the official ixml repo
https://github.com/invisibleXML/ixml
in a symlinked directory called /ixml.

Assuming you have this repo checked out in a sibling directory to earleybird,
the command for this is
    ln -s ../ixml .


# References

Invisible XML: https://invisiblexml.org/

IXML Repo: https://github.com/invisibleXML/ixml

Test Suite: https://github.com/invisibleXML/ixml/tree/master/tests

Vulturine Guinea Fowl: https://en.wikipedia.org/wiki/Vulturine_guineafowl