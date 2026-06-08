//! # earleybird
//!
//! An [Invisible XML](https://invisiblexml.org/) (ixml) processor: it turns an
//! ixml **grammar** plus an **input string** into an XML document.
//!
//! ## Public API
//!
//! The supported surface is small and centered on three types:
//!
//! - [`grammar::Grammar`] — compile a grammar with [`Grammar::from_ixml_str`].
//! - [`parser::Parser`] — parse input with [`Parser::parse`], which returns the
//!   owned output tree.
//! - [`treebird::Document`] / [`treebird::Node`] — the owned XML-infoset output
//!   tree; serialize it with [`Document::to_xml`].
//! - [`parser::ParseError`] (with [`parser::ErrorCode`], [`parser::ErrorKind`],
//!   [`parser::Span`]) — the structured error type.
//!
//! ```
//! use earleybird::grammar::Grammar;
//! use earleybird::parser::Parser;
//!
//! let grammar = Grammar::from_ixml_str(r#"greeting: "hello"."#).unwrap();
//! let mut parser = Parser::new(grammar);
//! let doc = parser.parse("hello").unwrap();
//! assert_eq!(doc.to_xml(), "<greeting>hello</greeting>");
//! ```
//!
//! Everything else reachable from this crate is implementation detail. Items
//! marked `#[doc(hidden)]` (the indextree arena path, `Content`, the Earley
//! internals, the test scaffolding) are **not** part of the public contract and
//! may change without a major-version bump. See `ARCHITECTURE.md` ("API design
//! principles") for the rationale.
//!
//! [`Grammar::from_ixml_str`]: grammar::Grammar::from_ixml_str
//! [`Parser::parse`]: parser::Parser::parse
//! [`Document::to_xml`]: treebird::Document::to_xml

/// Project-wide small immutable string for nonterminal names and identity keys.
/// Defined in one place so the backing small-string crate can be swapped here
/// without touching call sites. The accessed surface is string-like only
/// (`from`/`new` constructors plus `Deref<str>`), so any SSO crate can stand in.
#[doc(hidden)]
pub type EarleyStr = smol_str::SmolStr;

// ---- Public API ---------------------------------------------------------
pub mod grammar;
pub mod parser;
pub mod treebird;

// ---- Implementation detail (reachable for the `eb` binary and tests, but
//      not part of the public contract; see ARCHITECTURE.md, ADR 3) --------
#[doc(hidden)]
pub mod alloc_count;
#[doc(hidden)]
pub mod debug;
#[doc(hidden)]
pub mod ixml_bootstrap;
#[doc(hidden)]
pub mod test_grammars;
#[doc(hidden)]
pub mod testsuite_utils;
#[doc(hidden)]
pub mod unicode_ranges;
#[doc(hidden)]
pub mod utils;
#[doc(hidden)]
pub mod validator;
