//! Integration tests for the public treebird output tree.
//!
//! These see ONLY the public API (external-crate view), so they double as a
//! 1.0 contract check on the consumer-facing surface: the `treebird` types and
//! constructors, `Document::to_xml`, and `Parser::parse_to_document`.

use earleybird::grammar::Grammar;
use earleybird::parser::Parser;
use earleybird::treebird::{attr, Document, Node};

/// The owned tree is constructible and serializes to canonical XML purely from
/// the public constructors — no parser involved.
#[test]
fn public_constructors_serialize_to_xml() {
    let doc = Document::of([Node::el(
        "a",
        [attr("x", "1")],
        [Node::text("hi"), Node::el("b", [], [])],
    )]);
    assert_eq!(doc.to_xml(), r#"<a x="1">hi<b/></a>"#);
}

/// `parse` yields a single top-level element for a single-root grammar — the
/// structured, indextree-free entry point works end to end.
#[test]
fn parse_yields_single_root_element() {
    let grammar = Grammar::from_ixml_str(r#"doc: "a"."#).expect("grammar parses");
    let mut parser = Parser::new(grammar);
    let doc = parser.parse("a").expect("input parses");

    assert_eq!(doc.children.len(), 1, "expected a single top-level node");
    match &doc.children[0] {
        Node::Element { name, .. } => assert_eq!(name, "doc"),
        other => panic!("expected a top-level element, got {other:?}"),
    }
}

/// The public path is end-to-end: parse to an owned `Document`, validate it, and
/// serialize to canonical XML — all without touching any indextree/arena type.
#[test]
fn parse_validate_and_serialize_via_public_api() {
    let grammar = Grammar::from_ixml_str(r#"doc: "a"."#).expect("grammar parses");
    let mut parser = Parser::new(grammar);
    let doc = parser.parse("a").expect("input parses");

    doc.validate().expect("output is well-formed XML");
    assert_eq!(doc.to_xml(), "<doc>a</doc>");
}
