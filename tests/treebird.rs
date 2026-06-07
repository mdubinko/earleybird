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

/// `parse_to_document` yields a single top-level element for a single-root
/// grammar — the structured, indextree-free entry point works end to end.
#[test]
fn parse_to_document_yields_single_root_element() {
    let grammar = Grammar::from_ixml_str(r#"doc: "a"."#).expect("grammar parses");
    let mut parser = Parser::new(grammar);
    let doc = parser.parse_to_document("a").expect("input parses");

    assert_eq!(doc.children.len(), 1, "expected a single top-level node");
    match &doc.children[0] {
        Node::Element { name, .. } => assert_eq!(name, "doc"),
        other => panic!("expected a top-level element, got {other:?}"),
    }
}

/// The structured path agrees with the established string serialization: the
/// owned tree's `to_xml` matches the legacy `tree_to_test_format` for the same
/// parse. This pins the port as behavior-preserving via the public API.
#[test]
fn parse_to_document_to_xml_matches_legacy_serialization() {
    let grammar_src = r#"doc: "a"."#;
    let input = "a";

    let mut p_legacy = Parser::new(Grammar::from_ixml_str(grammar_src).unwrap());
    let arena = p_legacy.parse(input).expect("input parses");
    let legacy_xml = Parser::tree_to_test_format(&arena);

    let mut p_doc = Parser::new(Grammar::from_ixml_str(grammar_src).unwrap());
    let doc_xml = p_doc.parse_to_document(input).unwrap().to_xml();

    assert_eq!(doc_xml, legacy_xml);
}
