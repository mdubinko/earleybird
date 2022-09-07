# EarleyBird
Experimental implementation of ixml in Rust. Currently targeting the 20220620 spec.

# Status as of Sep 6, 2022

The very early version of running against the test suite is now working. You specify a path the the ixml test suite from the command line


# Usage

    RUST_LOG=info RUST_BACKTRACE=1 cargo run -- suite ../../ixml/tests/correct/test-catalog.xml

# Future work

* more generally, performance profiling and optimization


# References

Invisible XML: https://invisiblexml.org/

Test Suite: https://github.com/invisibleXML/ixml/tree/master/tests

Vulturine Guinea Fowl: https://en.wikipedia.org/wiki/Vulturine_guineafowl