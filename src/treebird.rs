//! treebird: an owned, public output tree for parse results.
//!
//! The parser builds results in a private `indextree` arena over
//! [`crate::parser::Content`]; that arena is the right shape for the build phase
//! but the wrong type to EXPOSE (an indextree major bump would be a breaking
//! change for earleybird, and consumers would need their own indextree dep).
//!
//! treebird is the owned infoset tree we hand to consumers instead: an
//! [`Element`](Node::Element)/[`Text`](Node::Text) vocabulary with attributes
//! kept in an ordered `Vec` (document order is significant and must round-trip).
//! Serializers (XML today, others later) live as methods on these types, and
//! `to_xml` defines the single canonical string form.

use indextree::{Arena, NodeId};

use crate::parser::Content;

/// The root of an owned output tree. Holds the top-level nodes (normally a
/// single element; the type permits more so the version-mismatch / ambiguous
/// states and malformed-but-representable trees stay expressible).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    pub children: Vec<Node>,
}

/// A node in the owned output tree. Mirrors the XML infoset that ixml emits:
/// elements (with ordered attributes and children) and text. There is no
/// attribute *node* — attributes belong to their element, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element {
        name: String,
        attributes: Vec<(String, String)>,
        children: Vec<Node>,
    },
    Text(String),
}

/// Convenience constructor for an attribute pair, e.g. `attr("x", "1")`.
pub fn attr(key: impl Into<String>, value: impl Into<String>) -> (String, String) {
    (key.into(), value.into())
}

impl Document {
    /// Build a document from a sequence of top-level nodes.
    pub fn of(children: impl IntoIterator<Item = Node>) -> Document {
        Document {
            children: children.into_iter().collect(),
        }
    }

    /// Build an owned [`Document`] from the parser's private indextree arena.
    ///
    /// This is the boundary between the internal build representation and the
    /// public output type. Attributes interleaved among an element's arena
    /// children are pulled out into the element's ordered `attributes`, in the
    /// order they appear.
    pub(crate) fn from_content_arena(arena: &Arena<Content>) -> Document {
        // The first node in the arena is the synthetic Root (see unpack_parse_tree).
        let Some(root) = arena.iter().next() else {
            return Document::default();
        };
        let root_id = arena
            .get_node_id(root)
            .expect("arena root must have a node id");
        let children = root_id
            .children(arena)
            .filter_map(|nid| node_from_arena(arena, nid))
            .collect();
        Document { children }
    }
}

impl Node {
    /// Build an element node, e.g.
    /// `Node::el("a", [attr("x", "1")], [Node::text("hi")])`.
    pub fn el(
        name: impl Into<String>,
        attributes: impl IntoIterator<Item = (String, String)>,
        children: impl IntoIterator<Item = Node>,
    ) -> Node {
        Node::Element {
            name: name.into(),
            attributes: attributes.into_iter().collect(),
            children: children.into_iter().collect(),
        }
    }

    /// Build a text node.
    pub fn text(value: impl Into<String>) -> Node {
        Node::Text(value.into())
    }

    /// Serialize this node into `out`. `extra_attrs` (if any) are appended,
    /// unescaped, after the element's own attributes — used only by the caller
    /// to stamp ixml:state annotations onto top-level elements; descendants
    /// always receive `None`.
    fn write_xml(&self, out: &mut String, extra_attrs: Option<&[(&str, &str)]>) {
        match self {
            Node::Element {
                name,
                attributes,
                children,
            } => {
                out.push('<');
                out.push_str(name);
                for (key, value) in attributes {
                    out.push(' ');
                    out.push_str(key);
                    out.push_str("=\"");
                    out.push_str(&escape_attr_value(value));
                    out.push('"');
                }
                if let Some(extra) = extra_attrs {
                    for (key, value) in extra {
                        out.push(' ');
                        out.push_str(key);
                        out.push_str("=\"");
                        out.push_str(value);
                        out.push('"');
                    }
                }
                // Attributes are not children here, so emptiness alone decides
                // self-closing (vs. the old arena code's is_attr filtering).
                if children.is_empty() {
                    out.push_str("/>");
                } else {
                    out.push('>');
                    for child in children {
                        child.write_xml(out, None);
                    }
                    out.push_str("</");
                    out.push_str(name);
                    out.push('>');
                }
            }
            Node::Text(value) => out.push_str(&escape_text(value)),
        }
    }
}

/// Escape an attribute value: `&`, `<`, `"` (order matters — `&` first).
fn escape_attr_value(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

/// Escape text content: only `&` and `<` (`>` and `"` are left literal).
fn escape_text(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;")
}

impl Document {
    /// Serialize to the canonical XML string form.
    pub fn to_xml(&self) -> String {
        self.to_xml_with_state(false, false)
    }

    /// Serialize, optionally stamping an `ixml:state` annotation onto the
    /// top-level element(s). `version_mismatch` takes precedence over
    /// `ambiguous` (matching the historical conformance-harness behavior).
    pub(crate) fn to_xml_with_state(&self, version_mismatch: bool, ambiguous: bool) -> String {
        let extra_attrs: Option<Vec<(&str, &str)>> = if version_mismatch {
            Some(vec![
                ("xmlns", ""),
                ("xmlns:ixml", "http://invisiblexml.org/NS"),
                ("ixml:state", "version-mismatch"),
            ])
        } else if ambiguous {
            Some(vec![
                ("xmlns:ixml", "http://invisiblexml.org/NS"),
                ("ixml:state", "ambiguous"),
            ])
        } else {
            None
        };
        let mut out = String::new();
        for child in &self.children {
            child.write_xml(&mut out, extra_attrs.as_deref());
        }
        out
    }
}

/// Recursively convert one arena node into an owned [`Node`].
///
/// Returns `None` for nodes that do not become tree nodes on their own: the
/// synthetic `Root`, and `Attribute` nodes (those are folded into their parent
/// element's `attributes` by the element arm below).
fn node_from_arena(arena: &Arena<Content>, nid: NodeId) -> Option<Node> {
    match arena.get(nid)?.get() {
        Content::Root | Content::Attribute(..) => None,
        Content::Text(value) => Some(Node::Text(value.clone())),
        Content::Element(name) => {
            let mut attributes = Vec::new();
            let mut children = Vec::new();
            for child in nid.children(arena) {
                match arena.get(child).expect("child node exists").get() {
                    Content::Attribute(key, value) => {
                        attributes.push((key.clone(), value.clone()));
                    }
                    _ => {
                        if let Some(node) = node_from_arena(arena, child) {
                            children.push(node);
                        }
                    }
                }
            }
            Some(Node::Element {
                name: name.clone(),
                attributes,
                children,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- constructor / equality unit tests --------------------------------

    #[test]
    fn attr_helper_builds_owned_pair() {
        assert_eq!(attr("x", "1"), ("x".to_string(), "1".to_string()));
    }

    #[test]
    fn constructors_build_expected_shape() {
        let built = Node::el("a", [attr("x", "1")], [Node::text("hi")]);
        let manual = Node::Element {
            name: "a".to_string(),
            attributes: vec![("x".to_string(), "1".to_string())],
            children: vec![Node::Text("hi".to_string())],
        };
        assert_eq!(built, manual);
    }

    #[test]
    fn empty_attrs_and_children_are_allowed() {
        assert_eq!(
            Node::el("a", [], []),
            Node::Element {
                name: "a".to_string(),
                attributes: vec![],
                children: vec![],
            }
        );
    }

    #[test]
    fn partial_eq_is_attribute_order_sensitive() {
        // Document-order is significant: reordering attributes is NOT equal.
        let xy = Node::el("a", [attr("x", "1"), attr("y", "2")], []);
        let yx = Node::el("a", [attr("y", "2"), attr("x", "1")], []);
        assert_ne!(xy, yx);
    }

    // ---- converter tests (arena -> Document) ------------------------------
    // Arenas are hand-built with the same indextree API the parser uses.

    fn root() -> (Arena<Content>, NodeId) {
        let mut arena = Arena::new();
        let r = arena.new_node(Content::Root);
        (arena, r)
    }
    fn el(arena: &mut Arena<Content>, parent: NodeId, name: &str) -> NodeId {
        let n = arena.new_node(Content::Element(name.to_string()));
        parent.append(n, arena);
        n
    }
    fn at(arena: &mut Arena<Content>, parent: NodeId, k: &str, v: &str) {
        let n = arena.new_node(Content::Attribute(k.to_string(), v.to_string()));
        parent.append(n, arena);
    }
    fn tx(arena: &mut Arena<Content>, parent: NodeId, s: &str) {
        let n = arena.new_node(Content::Text(s.to_string()));
        parent.append(n, arena);
    }

    #[test]
    fn converter_preserves_attribute_order() {
        // THE bug class that disqualified a HashMap-backed DOM: attribute order
        // must survive the arena -> Document conversion exactly.
        let (mut a, r) = root();
        let e = el(&mut a, r, "a");
        at(&mut a, e, "x", "1");
        at(&mut a, e, "y", "2");
        at(&mut a, e, "z", "3");
        let doc = Document::from_content_arena(&a);
        assert_eq!(
            doc,
            Document::of([Node::el(
                "a",
                [attr("x", "1"), attr("y", "2"), attr("z", "3")],
                []
            )])
        );
    }

    #[test]
    fn converter_splits_attrs_from_children() {
        let (mut a, r) = root();
        let e = el(&mut a, r, "a");
        at(&mut a, e, "x", "1");
        tx(&mut a, e, "hi");
        let doc = Document::from_content_arena(&a);
        assert_eq!(
            doc,
            Document::of([Node::el("a", [attr("x", "1")], [Node::text("hi")])])
        );
    }

    #[test]
    fn converter_handles_nested_and_text() {
        let (mut a, r) = root();
        let e = el(&mut a, r, "a");
        let b = el(&mut a, e, "b");
        tx(&mut a, b, "x");
        el(&mut a, e, "c");
        let doc = Document::from_content_arena(&a);
        assert_eq!(
            doc,
            Document::of([Node::el(
                "a",
                [],
                [
                    Node::el("b", [], [Node::text("x")]),
                    Node::el("c", [], []),
                ]
            )])
        );
    }

    #[test]
    fn converter_empty_element() {
        let (mut a, r) = root();
        el(&mut a, r, "a");
        let doc = Document::from_content_arena(&a);
        assert_eq!(doc, Document::of([Node::el("a", [], [])]));
    }

    #[test]
    fn converter_allows_multiple_top_level_nodes() {
        let (mut a, r) = root();
        el(&mut a, r, "a");
        el(&mut a, r, "b");
        let doc = Document::from_content_arena(&a);
        assert_eq!(
            doc,
            Document::of([Node::el("a", [], []), Node::el("b", [], [])])
        );
    }

    #[test]
    fn converter_empty_arena_is_empty_document() {
        let arena: Arena<Content> = Arena::new();
        assert_eq!(Document::from_content_arena(&arena), Document::default());
    }

    // ---- serializer (Document -> XML) unit tests --------------------------

    #[test]
    fn to_xml_basic_shapes() {
        let doc = Document::of([Node::el(
            "a",
            [attr("x", "1"), attr("y", "2")],
            [
                Node::el("b", [], [Node::text("hi")]),
                Node::el("c", [], []),
            ],
        )]);
        assert_eq!(doc.to_xml(), r#"<a x="1" y="2"><b>hi</b><c/></a>"#);
    }

    #[test]
    fn to_xml_escaping_matches_attr_and_text_rules() {
        let doc = Document::of([Node::el(
            "a",
            [attr("k", r#"a&b<c"d>e"#)],
            [Node::text(r#"a&b<c>d"e"#)],
        )]);
        assert_eq!(
            doc.to_xml(),
            r#"<a k="a&amp;b&lt;c&quot;d>e">a&amp;b&lt;c>d"e</a>"#
        );
    }

    #[test]
    fn to_xml_with_state_stamps_top_level_only() {
        let doc = Document::of([Node::el("a", [], [Node::el("b", [], [])])]);
        assert_eq!(
            doc.to_xml_with_state(false, true),
            r#"<a xmlns:ixml="http://invisiblexml.org/NS" ixml:state="ambiguous"><b/></a>"#
        );
    }

    #[test]
    fn to_xml_version_mismatch_takes_precedence_and_follows_real_attrs() {
        let doc = Document::of([Node::el("a", [attr("x", "1")], [])]);
        assert_eq!(
            doc.to_xml_with_state(true, true),
            r#"<a x="1" xmlns="" xmlns:ixml="http://invisiblexml.org/NS" ixml:state="version-mismatch"/>"#
        );
    }
}
