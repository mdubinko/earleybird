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