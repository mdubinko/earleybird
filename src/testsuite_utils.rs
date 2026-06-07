use std::collections::HashMap;
use std::path::PathBuf;
use std::str::from_utf8;
use std::{fmt, fs};

use itertools::Itertools;
use quick_xml::escape::unescape;
use quick_xml::events::attributes::Attributes;
use quick_xml::events::Event;
use quick_xml::name::QName;
use quick_xml::reader::Reader;

use crate::grammar::Grammar;
use crate::parser::Content;
use indextree::{Arena, NodeId};

type XmlString = String;

/// Unicode general-category version compiled into earleybird via unicode-character-database.
/// Tests that declare a different Unicode-version dependency are not loaded.
const SUPPORTED_UNICODE_VERSION: &str = "14.0";

#[derive(Clone, Debug)]
/// a test case. We duplicate the grammar (from parent test-set) if needed, for one-stop shopping
pub struct TestCase {
    pub name: String,
    /// if multiple test grammars present, identical results expected from using each
    pub grammars: Vec<TestGrammar>,
    pub input: String,
    /// normally only a single expected result, except in ambiguous tests
    pub expected: Vec<TestResult>,
}

#[derive(Clone, Debug)]
pub enum TestGrammar {
    Unparsed(String),
    Parsed(Grammar),
    /// Parse the test input using the built-in ixml grammar.
    /// Used by grammar-test/assert-xml: input = grammar source text, grammar = ixml.ixml.
    BootstrapIxml,
    /// The grammar tree was syntactically valid XML but semantically invalid (S-error).
    /// Used for VXML grammars that represent grammars containing static errors.
    FailedToLoad(String),
}

impl fmt::Display for TestGrammar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unparsed(s) => write!(f, "{s}"),
            Self::Parsed(g) => write!(f, "Grammar with {} rules", g.get_rule_count()),
            Self::BootstrapIxml => write!(f, "<built-in ixml grammar>"),
            Self::FailedToLoad(e) => write!(f, "<grammar load failed: {e}>"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum TestResult {
    AssertNotASentence,
    AssertDynamicError(String), // error code, e.g. "D01", "D02", ...
    AssertXml(XmlString),
    AssertNotAGrammar, // the grammar source itself should fail to compile (S-errors)
}

#[derive(Debug)]
pub enum TestOutcome {
    Pass,
    Fail {
        expected: String,
        actual: String,
    },
    // Phase 1: iXML validation errors (comment preprocessing, syntax validation)
    ValidationError(String),
    // Phase 2: Bootstrap grammar parsing errors (malformed grammar, unsupported features)
    BootstrapParseError(String),
    // Phase 3: Grammar tree conversion errors (unimplemented features, malformed trees)
    ConversionError(String),
    // Phase 4: Target grammar parsing errors (input doesn't match target grammar)
    InputParseError(String),
    // Infrastructure
    Panic(String),
    Skip(String),
    Todo(String),
    // Legacy - remove after migration
    #[deprecated(note = "Use specific error types instead")]
    GrammarParseError(String),
}

struct TestCaseBuilder {
    pub name: Option<String>,
    pub grammar: Vec<TestGrammar>,
    pub input: Option<String>,
    pub expected: Vec<TestResult>,
    pub unicode_version_required: Option<String>,
}

impl TestCaseBuilder {
    fn new() -> Self {
        Self {
            name: None,
            grammar: Vec::new(),
            input: None,
            expected: Vec::new(),
            unicode_version_required: None,
        }
    }

    /// Build the [`TestCase`]. Resets the builder.
    fn build(&mut self) -> Option<TestCase> {
        if self.name.is_none() {
            eprintln!("Warning: Skipping test case with no name");
            return None;
        }
        if self.grammar.is_empty() {
            eprintln!(
                "Warning: Skipping test case '{}' with no grammar",
                self.name.as_ref().unwrap()
            );
            return None;
        }
        if self.input.is_none() {
            eprintln!(
                "Warning: Skipping test case '{}' with no input",
                self.name.as_ref().unwrap()
            );
            return None;
        }
        if self.expected.is_empty() {
            eprintln!(
                "Warning: Skipping test case '{}' with no expected results",
                self.name.as_ref().unwrap()
            );
            return None;
        }
        // Drop tests whose Unicode-version dependency we don't satisfy.
        if let Some(ref required) = self.unicode_version_required {
            if required != SUPPORTED_UNICODE_VERSION {
                self.name = None;
                self.grammar.clear();
                self.input = None;
                self.expected.clear();
                self.unicode_version_required = None;
                return None;
            }
        }

        let name = self.name.take();
        self.name = None;
        let grammar = self.grammar.drain(..).collect();
        self.grammar.clear();
        let input = self.input.take();
        self.input = None;
        let expected = self.expected.drain(..).collect();
        self.expected.clear();
        self.unicode_version_required = None;

        Some(TestCase {
            name: name.unwrap(),
            grammars: grammar,
            input: input.unwrap(),
            expected,
        })
    }
}

/// Example test catalog format for documentation purposes
#[allow(dead_code)]
static TEST_CATALOG_FORMAT_EXAMPLE: &str = r##"<test-catalog xmlns='https://github.com/invisibleXML/ixml/test-catalog'>
<description>x</description>
  <test-set>
    <test-case name='all-local'>
      <test-string>x</test-string>
      <assert-xml-ref href='x'/>
    </test-case>
    <test-case name='inline-xml'>
      <test-string>x</test-string>
      <assert-xml>ok</assert-xml>
    </test-case>
    <!--
    <test-case name='test-string-href'>
      <test-string href='text'/>
      <assert-xml-ref href='xml'/>
    </test-case>
    <test-case-ref href='abc'/>
    -->
</test-set>
</test-catalog>"##;

/// Read test catalog, handling test-set-ref elements by recursively loading referenced catalogs
pub fn read_test_catalog(path: String) -> Vec<TestCase> {
    read_test_catalog_with_prefix(path, None)
}

/// Helper function that includes directory prefix in test names
fn read_test_catalog_with_prefix(path: String, dir_prefix: Option<String>) -> Vec<TestCase> {
    let pathbuf = PathBuf::from(&path);
    let basepath = pathbuf.parent().unwrap();
    let file = fs::read_to_string(&path).expect("The file could not be read");

    //let file = TEST_CATALOG_EG;
    //println!("{file}");

    let mut reader = Reader::from_str(&file);
    reader.config_mut().trim_text(true);
    reader.config_mut().expand_empty_elements = true;

    let mut buf = Vec::new();
    let mut test_set_nesting: Vec<String> = Vec::new();
    let mut current_grammar = TestGrammar::Unparsed(String::new());
    let mut builder = TestCaseBuilder::new();
    let mut test_cases: Vec<TestCase> = Vec::new();

    // grammar-test state: tests the grammar source itself (parse tree or S-error validation)
    let mut in_grammar_test = false;
    let mut grammar_test_name: Option<String> = None;
    let mut grammar_test_expected: Vec<TestResult> = Vec::new();
    // app-info blocks hold optional processor-specific hints, not required conformance assertions
    let mut in_app_info = false;

    // to capture <assert-xml> arbitrary content, we just store a buch of u8 in a Vec
    // (and later turn it into a String)
    let mut raw_xml_accum: Vec<u8> = Vec::new();
    let mut enable_accum = false;

    // The `Reader` does not implement `Iterator` because it outputs borrowed data (`Cow`s)
    loop {
        // NOTE: this is the generic case when we don't know about the input BufRead.
        // when the input is a &str or a &[u8], we don't actually need to use another
        // buffer, we could directly call `reader.read_event()`
        match reader.read_event_into(&mut buf) {
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            // exits the loop when reaching end of file
            Ok(Event::Eof) => break,

            Ok(Event::Start(e)) => {
                // Use local_name() to strip any namespace prefix (e.g. "tc:test-case" → "test-case")
                match e.name().local_name().as_ref() {
                    b"test-set" => {
                        let name = attr_by_name(&e.attributes(), "name");
                        test_set_nesting.push(name);
                    }
                    b"ixml-grammar" => {
                        let grammar = reader.read_text(e.to_end().name());
                        let raw_grammar = grammar.expect("parse error reading inline grammar");
                        // read_text yields raw BytesText; unescape() decodes the charset and
                        // resolves XML entities in one step.
                        // Ignore grammars inside app-info blocks (those are processor hints,
                        // e.g., parse-forest grammars, not the grammar under test).
                        if !in_app_info {
                            let decoded = raw_grammar.decode().expect("Failed to decode inline grammar");
                            current_grammar = TestGrammar::Unparsed(
                                unescape(&decoded)
                                    .expect("Failed to unescape inline grammar")
                                    .to_string(),
                            );
                        }
                    }
                    b"ixml-grammar-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(href);
                        if !in_app_info {
                            current_grammar = TestGrammar::Unparsed(
                                fs::read_to_string(fullpath).expect("Error reading grammar file"),
                            );
                        }
                    }
                    b"vxml-grammar-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(href);
                        current_grammar = read_vxml_grammar(fullpath);
                    }
                    b"grammar-test" => {
                        in_grammar_test = true;
                        grammar_test_expected.clear();
                        // Build test name from current nesting
                        let mut fullname = String::new();
                        if let Some(ref prefix) = dir_prefix {
                            fullname.push_str(prefix);
                            fullname.push('/');
                        }
                        if !test_set_nesting.is_empty() {
                            fullname.push_str(&test_set_nesting.join("/"));
                            fullname.push('/');
                        }
                        fullname.push_str("grammar-test");
                        grammar_test_name = Some(fullname);
                    }
                    b"dependencies" => {
                        let uv = attr_by_name(&e.attributes(), "Unicode-version");
                        if !uv.is_empty() {
                            builder.unicode_version_required = Some(uv);
                        }
                    }
                    b"test-case" => {
                        let name = attr_by_name(&e.attributes(), "name");
                        builder = TestCaseBuilder::new();
                        let mut fullname = String::new();

                        // Add directory prefix if present
                        if let Some(ref prefix) = dir_prefix {
                            fullname.push_str(prefix);
                            fullname.push('/');
                        }

                        // Add test-set nesting
                        if !test_set_nesting.is_empty() {
                            fullname.push_str(&test_set_nesting.join("/"));
                            fullname.push('/');
                        }

                        fullname.push_str(&name);
                        builder.name = Some(fullname);
                        builder.grammar.push(current_grammar.clone());
                    }
                    b"test-case-ref" => {
                        // TODO: maybe just note these somewhere...
                    }
                    b"test-set-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(&href);

                        // Extract directory name as prefix for test names
                        let href_path = PathBuf::from(&href);
                        let dir_name = href_path
                            .parent()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown");

                        // Recursively load the referenced catalog with directory prefix
                        let mut referenced_tests = read_test_catalog_with_prefix(
                            fullpath.to_string_lossy().to_string(),
                            Some(dir_name.to_string()),
                        );
                        test_cases.append(&mut referenced_tests);
                    }
                    b"test-string" => {
                        // Disable trim_text so whitespace-only inputs (e.g. " ") are preserved;
                        // trim_text(true) would collapse them to empty string.
                        reader.config_mut().trim_text(false);
                        let input = reader.read_text(e.to_end().name());
                        reader.config_mut().trim_text(true);
                        let raw_input = input.expect("parse error reading inline test-string");
                        // read_text yields raw BytesText: decode() handles the charset,
                        // then unescape() resolves XML entities.
                        let decoded = raw_input.decode().expect("Failed to decode test-string");
                        builder.input = Some(
                            unescape(&decoded)
                                .expect("Failed to unescape test-string")
                                .to_string(),
                        );
                    }
                    b"test-string-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(href);
                        builder.input =
                            Some(fs::read_to_string(fullpath).expect("Error reading grammar file"));
                    }
                    b"app-info" => {
                        in_app_info = true;
                    }
                    b"assert-not-a-sentence" => {
                        if !in_app_info {
                            builder.expected.push(TestResult::AssertNotASentence);
                        }
                    }
                    b"assert-not-a-grammar" => {
                        // The grammar source itself should fail to compile (S-error)
                        if !in_app_info {
                            if in_grammar_test {
                                grammar_test_expected.push(TestResult::AssertNotAGrammar);
                            } else {
                                builder.expected.push(TestResult::AssertNotAGrammar);
                            }
                        }
                    }
                    b"assert-dynamic-error" => {
                        if !in_app_info {
                            let mut codes = attr_by_name(&e.attributes(), "error-code");
                            if codes.is_empty() {
                                codes = attr_by_name(&e.attributes(), "code");
                            }
                            for code in codes.split(' ') {
                                if !code.is_empty() {
                                    builder
                                        .expected
                                        .push(TestResult::AssertDynamicError(String::from(code)));
                                }
                            }
                        }
                    }
                    b"assert-xml" => {
                        if !in_app_info {
                            enable_accum = true;
                            // Capture the expected XML verbatim. In quick-xml 0.40 entity
                            // references split the text into separate events; trimming would
                            // drop spaces adjacent to them. Canonicalization normalizes
                            // layout whitespace later, so capturing untrimmed is safe.
                            reader.config_mut().trim_text(false);
                        }
                    }
                    b"assert-xml-ref" => {
                        if !in_app_info {
                            let href = attr_by_name(&e.attributes(), "href");
                            let mut fullpath = basepath.to_path_buf();
                            fullpath.push(href);
                            let xml = fs::read_to_string(fullpath).expect("Error reading assert-xml file");
                            if in_grammar_test {
                                grammar_test_expected.push(TestResult::AssertXml(xml));
                            } else {
                                builder.expected.push(TestResult::AssertXml(xml));
                            }
                        }
                    }
                    _ => {
                        if enable_accum {
                            raw_xml_accum.push(b'<');
                            raw_xml_accum.extend(e.iter());
                            raw_xml_accum.push(b'>');
                        }
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if enable_accum {
                    raw_xml_accum.extend(t.iter());
                }
            }
            Ok(Event::End(e)) => {
                // Use local_name() to strip any namespace prefix
                match e.name().local_name().as_ref() {
                    b"test-set" => {
                        test_set_nesting.pop();
                    }
                    b"test-case" => {
                        if let Some(test_case) = builder.build() {
                            test_cases.push(test_case);
                        }
                    }
                    b"app-info" => {
                        in_app_info = false;
                    }
                    b"grammar-test" => {
                        in_grammar_test = false;
                        if let Some(name) = grammar_test_name.take() {
                            if !grammar_test_expected.is_empty() {
                                let has_not_a_grammar = grammar_test_expected
                                    .iter()
                                    .any(|e| matches!(e, TestResult::AssertNotAGrammar));
                                let has_assert_xml = grammar_test_expected
                                    .iter()
                                    .any(|e| matches!(e, TestResult::AssertXml(_)));

                                if has_not_a_grammar {
                                    // Grammar should fail to compile: use grammar source as subject.
                                    // Filter out any assert-xml entries that snuck into the same block.
                                    if let TestGrammar::Unparsed(ref source) = current_grammar {
                                        let ang_expected: Vec<_> = grammar_test_expected
                                            .iter()
                                            .filter(|e| !matches!(e, TestResult::AssertXml(_)))
                                            .cloned()
                                            .collect();
                                        test_cases.push(TestCase {
                                            name: name.clone(),
                                            grammars: vec![TestGrammar::Unparsed(source.clone())],
                                            input: String::new(),
                                            expected: ang_expected,
                                        });
                                    }
                                }

                                if has_assert_xml && !has_not_a_grammar {
                                    // Parse the grammar source as iXML using the built-in grammar
                                    // and compare the resulting parse tree to the expected XML.
                                    if let TestGrammar::Unparsed(ref source) = current_grammar {
                                        let xml_expected: Vec<_> = grammar_test_expected
                                            .iter()
                                            .filter(|e| matches!(e, TestResult::AssertXml(_)))
                                            .cloned()
                                            .collect();
                                        test_cases.push(TestCase {
                                            name,
                                            grammars: vec![TestGrammar::BootstrapIxml],
                                            input: source.clone(),
                                            expected: xml_expected,
                                        });
                                    }
                                }
                            }
                        }
                        grammar_test_expected.clear();
                    }
                    b"assert-xml" => {
                        enable_accum = false;
                        reader.config_mut().trim_text(true);
                        let xml_string = from_utf8(&raw_xml_accum)
                            .expect("UTF-8 error in assert-xml")
                            .to_string();

                        // Warn about special ixml:state values
                        let active_name = if in_grammar_test { &grammar_test_name } else { &builder.name };
                        if let Some(ref name) = active_name {
                            if xml_string.contains("ixml:state") {
                                if xml_string.contains("ambiguous") {
                                    eprintln!("🫥 Ambiguous case: {}", name);
                                }
                                if xml_string.contains("version-mismatch") {
                                    eprintln!("📦 Version mismatch: {}", name);
                                }
                                if xml_string.contains("failed") {
                                    eprintln!("💥 Expected failure: {}", name);
                                }
                            }
                        }

                        let result = TestResult::AssertXml(xml_string);
                        if in_grammar_test {
                            grammar_test_expected.push(result);
                        } else {
                            builder.expected.push(result);
                        }
                        raw_xml_accum.clear();
                    }
                    _ => {
                        if enable_accum {
                            raw_xml_accum.push(b'<');
                            raw_xml_accum.push(b'/');
                            raw_xml_accum.extend(e.iter());
                            raw_xml_accum.push(b'>');
                        }
                    }
                }
            }
            Ok(Event::Empty(_b)) => (),
            Ok(Event::CData(_b)) => (),
            Ok(Event::Comment(_b)) => (),
            Ok(Event::PI(_b)) => (),
            Ok(Event::Decl(_b)) => (),
            Ok(Event::DocType(_b)) => (),
            Ok(Event::GeneralRef(r)) => {
                // quick-xml 0.40 emits entity references in text as their own events;
                // keep the literal `&name;` in the accumulated assert-xml so that
                // canonicalization later resolves it the same way as the actual output.
                if enable_accum {
                    raw_xml_accum.push(b'&');
                    raw_xml_accum.extend(r.iter());
                    raw_xml_accum.push(b';');
                }
            }
        }
        // if we don't keep a borrow elsewhere, we can clear the buffer to keep memory usage low
        buf.clear();
    }
    // println!("read {} cases", test_cases.len());
    test_cases
}

fn read_vxml_grammar(path: PathBuf) -> TestGrammar {
    let xml = fs::read_to_string(&path).expect("Error reading VXML grammar file");
    let arena = vxml_to_arena(&xml);
    match Grammar::from_parse_tree(&arena) {
        Ok(g) => TestGrammar::Parsed(g),
        Err(e) => TestGrammar::FailedToLoad(e.to_string()),
    }
}

fn vxml_to_arena(xml: &str) -> Arena<Content> {
    let mut arena = Arena::new();
    let root = arena.new_node(Content::Root);
    let mut stack: Vec<NodeId> = vec![root];
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => panic!(
                "Error parsing VXML grammar at position {}: {:?}",
                reader.buffer_position(),
                e
            ),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let element = append_vxml_element(&mut arena, *stack.last().unwrap(), &e);
                stack.push(element);
            }
            Ok(Event::Empty(e)) => {
                append_vxml_element(&mut arena, *stack.last().unwrap(), &e);
            }
            Ok(Event::End(_)) => {
                stack.pop();
            }
            Ok(Event::Text(t)) => {
                if let Ok(decoded) = t.decode() {
                    if let Ok(text) = unescape(&decoded) {
                        if !text.trim().is_empty() {
                            let text_node = arena.new_node(Content::Text(text.to_string()));
                            stack.last().unwrap().append(text_node, &mut arena);
                        }
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                // Resolve the entity reference and keep its (non-whitespace) text.
                let name = r.decode().expect("decode entity reference");
                if let Ok(resolved) = unescape(&format!("&{};", name)) {
                    if !resolved.trim().is_empty() {
                        let text_node = arena.new_node(Content::Text(resolved.to_string()));
                        stack.last().unwrap().append(text_node, &mut arena);
                    }
                }
            }
            Ok(Event::CData(_))
            | Ok(Event::Comment(_))
            | Ok(Event::PI(_))
            | Ok(Event::Decl(_))
            | Ok(Event::DocType(_)) => {}
        }
        buf.clear();
    }
    arena
}

fn append_vxml_element<'a>(
    arena: &mut Arena<Content>,
    parent: NodeId,
    elem: &quick_xml::events::BytesStart<'a>,
) -> NodeId {
    let name = from_utf8(elem.name().as_ref())
        .expect("UTF-8 error parsing VXML element name")
        .to_string();
    let elem_node = arena.new_node(Content::Element(name));
    parent.append(elem_node, arena);

    for (name, value) in all_attrs(elem.attributes()) {
        let attr_node = arena.new_node(Content::Attribute(name, value));
        elem_node.append(attr_node, arena);
    }

    elem_node
}

/// Not "Canonical XML" but close enough for our purposes here
/// Formats an XML document in a conveniently-diffable format
/// Not namespace-aware, and does its own thing with newlines
pub fn xml_canonicalize(input_xml: &str) -> String {
    let mut builder = String::new();
    // Accumulates a run of consecutive text and entity-reference events. quick-xml 0.40
    // emits entity references (`&lt;`, `&#xD7;`, …) as their own events, splitting the
    // text around them. Buffering the run and flushing it at the next structural event
    // lets us trim leading/trailing layout whitespace from the *logical* text as a whole,
    // while preserving spaces that sit next to an entity reference.
    let mut pending_text = String::new();

    let mut reader = Reader::from_str(input_xml);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = true;

    let mut buf = Vec::new();

    // Trim the buffered text run and, if any content remains, escape `&`/`<` and emit it.
    fn flush(builder: &mut String, pending: &mut String) {
        let trimmed = pending.trim();
        if !trimmed.is_empty() {
            builder.push_str(&trimmed.replace('&', "&amp;").replace('<', "&lt;"));
        }
        pending.clear();
    }

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                eprintln!(
                    "Warning: XML parsing error at position {}: {:?} - returning input as-is",
                    reader.buffer_position(),
                    e
                );
                return input_xml.to_string();
            }
            // exits the loop when reaching end of file
            Ok(Event::Eof) => {
                flush(&mut builder, &mut pending_text);
                break;
            }

            Ok(Event::Start(e)) => {
                flush(&mut builder, &mut pending_text);
                let attrs = all_attrs(e.attributes());
                builder.push('<');
                builder.push_str(
                    from_utf8(e.name().into_inner()).expect("UTF-8 parse error on element start"),
                );
                if !attrs.is_empty() {
                    for (k, v) in attrs.into_iter().sorted() {
                        builder.push(' ');
                        builder.push_str(&k);
                        builder.push_str("=\"");
                        builder.push_str(
                            &v.replace('&', "&amp;")
                                .replace('<', "&lt;")
                                .replace('"', "&quot;"),
                        );
                        builder.push('"')
                    }
                }
                builder.push_str("\n>");
            }
            Ok(Event::Text(t)) => {
                let decoded = t.decode().unwrap_or_else(|_| String::from_utf8_lossy(&t));
                match unescape(&decoded) {
                    Ok(unescaped) => pending_text.push_str(&unescaped),
                    Err(e) => {
                        eprintln!("Warning: Failed to unescape text content, using raw text: `{}` error: {}", decoded, e);
                        // Fall back to decoded text without entity unescaping
                        pending_text.push_str(&decoded);
                    }
                }
            }
            Ok(Event::GeneralRef(r)) => {
                // Resolve the entity reference and add its character(s) to the text run.
                let name = r.decode().expect("decode entity reference");
                let entity = format!("&{};", name);
                let resolved = unescape(&entity).map(|c| c.to_string()).unwrap_or(entity);
                pending_text.push_str(&resolved);
            }
            Ok(Event::End(e)) => {
                flush(&mut builder, &mut pending_text);
                builder.push_str("</");
                builder.push_str(
                    from_utf8(&e.into_owned()).expect("UTF-8 parse error on element close"),
                );
                builder.push_str("\n>");
            }
            // Any other structural event ends the current text run.
            Ok(_) => flush(&mut builder, &mut pending_text),
        }
        buf.clear();
    } // loop
    builder
}

/// Helper function to get one particular attribute and return its value
/// Assumes everything here is UTF-8 valid, otherwise panics
fn attr_by_name(attrs: &Attributes, name: &str) -> String {
    let qname = QName(name.as_bytes());
    attrs
        .clone()
        .filter(|a| if let Ok(f) = a { f.key == qname } else { false })
        .map(|a| String::from(from_utf8(a.unwrap().value.as_ref()).unwrap()))
        .collect()
}

/// Just grab all the attributes as a `HashMap`
/// Assumes everything here is UTF-8 valid, otherwise panics
/// Silently deletes xmlns and xmlns:* namespace declarations
fn all_attrs(attrs: Attributes) -> HashMap<String, String> {
    let mut hashmap: HashMap<String, String> = HashMap::new();
    for attr in attrs {
        match attr {
            Ok(a) => {
                let name: String = from_utf8(a.key.into_inner())
                    .expect("UTF-8 error parsing attribute name")
                    .to_string();
                if name != "xmlns" && !name.starts_with("xmlns:") {
                    let raw_value =
                        from_utf8(&a.value).expect("UTF-8 error in attribute value");
                    match unescape(raw_value) {
                        Ok(value) => {
                            hashmap.insert(name, value.to_string());
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to unescape attribute '{}' value, using raw value: `{}` error: {}", name, raw_value, e);
                            // Fall back to raw value without entity unescaping
                            hashmap.insert(name, raw_value.to_string());
                        }
                    }
                };
            }
            Err(e) => {
                println!("{e}");
                panic!("Error iterating through attributes");
            }
        };
    }
    hashmap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonize_xml_basic() {
        // Verify whitespace normalization, attribute sorting, and quote style normalization
        let xml1 = r#" <A xmlns = "" >  <B  value ='"'  name= "foo">text&lt;</B >   </A> "#;
        let xml2 = r#"<A
><B name="foo" value="&quot;"
>text&lt;</B
></A
>"#;
        println!("1: {}", xml_canonicalize(xml1));
        assert_eq!(xml_canonicalize(xml1), xml2);
        println!("2: {}", xml_canonicalize(xml2));
        assert_eq!(xml_canonicalize(xml1), xml_canonicalize(xml2));
    }

    #[test]
    fn test_canonize_xml_apostrophe_in_attr() {
        // Apostrophes in double-quoted attributes should remain literal
        let input = r#"<test a="don't">.</test>"#;
        let expected = r#"<test a="don't"
>.</test
>"#;
        assert_eq!(xml_canonicalize(input), expected);
    }

    #[test]
    fn test_canonize_xml_apostrophe_entity_in_attr() {
        // &apos; entity should be unescaped to literal apostrophe
        let input = r#"<test a="&apos;">.</test>"#;
        let expected = r#"<test a="'"
>.</test
>"#;
        assert_eq!(xml_canonicalize(input), expected);
    }

    #[test]
    fn test_canonize_xml_ampersand_in_attr() {
        // Ampersands in attributes must remain escaped as &amp;
        let input = r#"<test a="&amp;">.</test>"#;
        let expected = r#"<test a="&amp;"
>.</test
>"#;
        assert_eq!(xml_canonicalize(input), expected);
    }

    #[test]
    fn test_canonize_xml_all_entities_in_attr() {
        // Comprehensive entity handling in attributes (matching attribute-value test case)
        // Note: &sol; is not a standard XML entity, so we use literal /
        let input = r#"<test a="&quot;&apos;&lt;>/&amp;">.</test>"#;
        let expected = r#"<test a="&quot;'&lt;>/&amp;"
>.</test
>"#;
        assert_eq!(xml_canonicalize(input), expected);
    }

    #[test]
    fn test_canonize_xml_single_quoted_attr_to_double() {
        // Single-quoted attributes convert to double-quoted with literal apostrophe
        let input = r#"<test a='&apos;'>.</test>"#;
        let expected = r#"<test a="'"
>.</test
>"#;
        assert_eq!(xml_canonicalize(input), expected);
    }

    #[test]
    fn test_canonize_xml_text_entities() {
        // Entity handling in text content
        let input = r#"<test>&quot;&apos;&lt;&gt;&amp;</test>"#;
        let expected = r#"<test
>"'&lt;>&amp;</test
>"#;
        assert_eq!(xml_canonicalize(input), expected);
    }

    #[test]
    fn test_canonize_xml_double_escaped_entity() {
        // Double-escaped entities represent literal entity strings
        let input = r#"<test a="&amp;apos;">.</test>"#;
        let expected = r#"<test a="&amp;apos;"
>.</test
>"#;
        assert_eq!(xml_canonicalize(input), expected);
    }
} // end tests module
