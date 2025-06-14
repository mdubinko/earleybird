# EarleyBird
Experimental implementation of ixml in Rust. Currently targeting the 20220620 spec.

# Status as of Oct 1, 2022

The very early version of running against the test suite is now working. You specify a path the the ixml test suite from the command line. Most of ixml grammar is supported, other than insertions, comments, and some details around string quoting and Unicode support.

Currently re-thinking error handling, in a more Rust-idiomatic way. Also looking at error-stack


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
```

### Debug Output Structure

The logging system provides structured, component-specific output:

- **`[GRAMMAR]`**: Grammar construction and validation
- **`[PARSER]`**: High-level parsing operations  
- **`[EARLEY]`**: Detailed Earley algorithm steps (trace mode)
- **`[EARLEY@n]`**: Earley operations at specific input position n
- **`[EARLEY-FAIL@n]`**: Parse failures at position n showing expected vs actual

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

### Implementation Notes for Developers

The debug infrastructure is centralized in `src/debug.rs`:

- **`DebugLevel`**: Enum controlling output verbosity
- **`DebugConfig`**: Configuration with position filtering and failure-only modes
- **Debug macros**: `debug_basic!()`, `debug_detailed!()`, `debug_trace!()`
- **Component-specific macros**: `debug_grammar!()`, `debug_parser!()`, `debug_earley!()`
- **Position-aware macros**: `debug_earley_pos!()`, `debug_earley_fail!()`
- **Failure analysis**: `debug_parse_failure()` for detailed error context

Key benefits:
- No scattered `println!` or `eprintln!` statements throughout codebase
- Debug levels controlled centrally without conditional bloat in main code
- Position filtering prevents trace output overload
- Easy to add new debugging without changing existing code structure
- Perfect for Claude Code workflows - no temporary files needed for quick testing

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