# EarleyBird
Experimental implementation of ixml in Rust. Currently targeting the 20220620 spec.

# Status as of August 15, 2022

My overall goal is to get onboard with the ixml test suite. From there, incremental progress in the form of passing %.

In ixml_grammar.rs you will find a subset of the core ixml grammar. Once the parser is relatively bug-free it should be straightforward to wire this up in a workflow that

1) parses an input grammar in ixml format,

2) rendering the parse tree into the internal representation of a grammar (with statements like grammar.add_seq and so on), then

3) parse an input stream against this produced grammar.

And you have the happy-path of a useful tool. From there, progress should be measurable and swift.

# Usage

for now `cargo test` is your best bet.

# Future work

* maybe use https://github.com/saschagrunert/indextree

* move to a better logging system, with a 'verbose' switch

* more generally, performance profiling and optimization


# References

Invisible XML: https://invisiblexml.org/

Test Suite: https://github.com/invisiblexml/ixml

Vulturine Guinea Fowl: https://en.wikipedia.org/wiki/Vulturine_guineafowl