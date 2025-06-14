# earleybird Examples

This directory contains simple examples to test the `eb` CLI utility.

## Quick Test

The simplest way to test the CLI:

```bash
# Using included test from the ixml test suite
cargo run --bin eb -- parse -g ixml/tests/correct/test.ixml -i ixml/tests/correct/test.inp

# Using the example files in this directory
cargo run --bin eb -- parse -g examples/simple.ixml -i examples/simple.txt
```

## Files

- `simple.ixml` - A basic greeting grammar
- `simple.txt` - Input text that matches the grammar
- `README.md` - This file

## Expected Output

The CLI parses the input file using the ixml grammar and outputs XML:

```xml
<greeting>Hello <name>World</name>!</greeting>
```

## More Examples

The `ixml/tests/correct/` directory contains hundreds of working test cases with more complex grammars.