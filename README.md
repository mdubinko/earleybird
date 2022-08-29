# EarleyBird
Experimental implementation of ixml in Rust. Currently targeting the 20220620 spec.

# Status as of August 25, 2022

The test case in ixml_grammar is now passing, demonstrating full bootstrapping.
(Start with an ixml grammar text file. Parse it. Construct a new grammar from the parse tree.
Use it to parse a target document.)

My overall goal is to get onboard with the ixml test suite. From there, incremental progress should be measurable and swift.

# Usage

for now `cargo test` is your best bet.

# Future work

* move to a better logging system, with a 'verbose' switch

* more generally, performance profiling and optimization


# References

Invisible XML: https://invisiblexml.org/

Test Suite: https://github.com/invisibleXML/ixml/tree/master/tests

Vulturine Guinea Fowl: https://en.wikipedia.org/wiki/Vulturine_guineafowl