# EarleyBird
Experimental implementation of ixml in Rust. Currently targeting the 20220620 spec.

# Status as of Oct 1, 2022

The very early version of running against the test suite is now working. You specify a path the the ixml test suite from the command line. Most of ixml grammar is supported, other than insertions, comments, and some details around string quoting and Unicode support.

Currently re-thinking error handling, in a more Rust-idiomatic way. Also looking at error-stack


# Usage

    RUST_LOG=info RUST_BACKTRACE=1 cargo run -- suite ../../ixml/tests/correct/test-catalog.xml

# Future work

* more generally, performance profiling and optimization

* flamegraph profiling?


# References

Invisible XML: https://invisiblexml.org/

Test Suite: https://github.com/invisibleXML/ixml/tree/master/tests

Vulturine Guinea Fowl: https://en.wikipedia.org/wiki/Vulturine_guineafowl