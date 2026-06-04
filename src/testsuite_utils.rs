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
use string_builder::Builder;

use crate::grammar::Grammar;

type XmlString = String;

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
}

impl fmt::Display for TestGrammar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unparsed(s) => write!(f, "{s}"),
            Self::Parsed(g) => write!(f, "Grammar with {} rules", g.get_rule_count()),
        }
    }
}

#[derive(Clone, Debug)]
pub enum TestResult {
    AssertNotASentence,
    AssertDynamicError(String), // error code, e.g. "D01", "D02", ...
    AssertXml(XmlString),
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
}

impl TestCaseBuilder {
    fn new() -> Self {
        Self {
            name: None,
            grammar: Vec::new(),
            input: None,
            expected: Vec::new(),
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
        let name = self.name.take();
        self.name = None;
        let grammar = self.grammar.drain(..).collect();
        self.grammar.clear();
        let input = self.input.take();
        self.input = None;
        let expected = self.expected.drain(..).collect();
        self.expected.clear();
        // println!("built test case ===={}====", name.clone().unwrap());

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
    reader.trim_text(true);
    reader.expand_empty_elements(true);

    let mut buf = Vec::new();
    let mut test_set_nesting: Vec<String> = Vec::new();
    let mut current_grammar = String::new();
    let mut builder = TestCaseBuilder::new();
    let mut test_cases: Vec<TestCase> = Vec::new();

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
                match e.name().as_ref() {
                    b"test-set" => {
                        let name = attr_by_name(&e.attributes(), "name");
                        test_set_nesting.push(name);
                    }
                    b"ixml-grammar" => {
                        let grammar = reader.read_text(e.to_end().name());
                        let raw_grammar = grammar.expect("parse error reading inline grammar");
                        // quick-xml's read_text() doesn't decode entities, so we use unescape()
                        current_grammar = unescape(&raw_grammar)
                            .expect("Failed to unescape inline grammar")
                            .to_string();
                    }
                    b"ixml-grammar-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(href);
                        //println!("ixml-grammar-ref {}", fullpath.to_string_lossy());
                        current_grammar =
                            fs::read_to_string(fullpath).expect("Error reading grammar file");
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
                        builder
                            .grammar
                            .push(TestGrammar::Unparsed(current_grammar.clone()));
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
                        let input = reader.read_text(e.to_end().name());
                        let raw_input = input.expect("parse error reading inline test-string");
                        // quick-xml's read_text() doesn't decode entities, so we use unescape()
                        builder.input = Some(
                            unescape(&raw_input)
                                .expect("Failed to unescape test-string")
                                .to_string(),
                        );
                    }
                    b"test-string-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(href);
                        //println!("test-string-ref {}", fullpath.to_string_lossy());
                        builder.input =
                            Some(fs::read_to_string(fullpath).expect("Error reading grammar file"));
                    }
                    b"assert-not-a-sentence" => {
                        builder.expected.push(TestResult::AssertNotASentence);
                    }
                    b"assert-dynamic-error" => {
                        let codes = attr_by_name(&e.attributes(), "code");
                        for code in codes.split(' ') {
                            builder
                                .expected
                                .push(TestResult::AssertDynamicError(String::from(code)));
                        }
                    }
                    b"assert-xml" => {
                        //let inner_content = reader.read_to_end(e.to_end().name());
                        //builder.expected.push(TestResult::AssertXml(from_utf8(inner_content.expect("Error reading inline assert-xml")).unwrap()));
                        enable_accum = true;
                    }
                    b"assert-xml-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(href);
                        //println!("assert-xml-ref {}", fullpath.to_string_lossy());
                        builder.expected.push(TestResult::AssertXml(
                            fs::read_to_string(fullpath).expect("Error reading assert-xml file"),
                        ));
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
                match e.name().as_ref() {
                    b"test-set" => {
                        test_set_nesting.pop();
                    }
                    b"test-case" => {
                        if let Some(test_case) = builder.build() {
                            test_cases.push(test_case);
                        }
                    }
                    b"assert-xml" => {
                        enable_accum = false;
                        let xml_string = from_utf8(&raw_xml_accum)
                            .expect("UTF-8 error in assert-xml")
                            .to_string();

                        // Warn about special ixml:state values (features not yet supported)
                        if let Some(ref name) = builder.name {
                            // Match ixml:state containing specific words (may have other text)
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

                        //println!("assert-xml literal {xml_string}");
                        builder.expected.push(TestResult::AssertXml(xml_string));
                        raw_xml_accum.clear(); // Clear buffer for next assert-xml
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
        }
        // if we don't keep a borrow elsewhere, we can clear the buffer to keep memory usage low
        buf.clear();
    }
    // println!("read {} cases", test_cases.len());
    test_cases
}

/// Not "Canonical XML" but close enough for our purposes here
/// Formats an XML document in a conveniently-diffable format
/// Not namespace-aware, and does its own thing with newlines
pub fn xml_canonicalize(input_xml: &str) -> String {
    let mut builder = Builder::default();

    let mut reader = Reader::from_str(input_xml);
    reader.trim_text(true);
    reader.expand_empty_elements(true);

    let mut buf = Vec::new();

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
            Ok(Event::Eof) => break,

            Ok(Event::Start(e)) => {
                let attrs = all_attrs(e.attributes());
                builder.append("<");
                builder.append(
                    from_utf8(e.name().into_inner()).expect("UTF-8 parse error on element start"),
                );
                if !attrs.is_empty() {
                    for (k, v) in attrs.into_iter().sorted() {
                        builder.append(" ");
                        builder.append(k);
                        builder.append("=\"");
                        builder.append(
                            v.replace('&', "&amp;")
                                .replace('<', "&lt;")
                                .replace('"', "&quot;"),
                        );
                        builder.append("\"")
                    }
                }
                builder.append("\n>");
            }
            Ok(Event::Text(t)) => {
                match t.unescape() {
                    Ok(unescaped) => {
                        builder.append(
                            unescaped
                                .to_string()
                                .replace('&', "&amp;")
                                .replace('<', "&lt;"),
                        );
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to unescape text content, using raw text: `{}` error: {}", String::from_utf8_lossy(&t).to_string(), e);
                        // Fall back to raw text without unescaping
                        builder.append(
                            String::from_utf8_lossy(&t)
                                .to_string()
                                .replace('&', "&amp;")
                                .replace('<', "&lt;"),
                        );
                    }
                }
            }
            Ok(Event::End(e)) => {
                builder.append("</");
                builder.append(
                    from_utf8(&e.into_owned()).expect("UTF-8 parse error on element close"),
                );
                builder.append("\n>");
            }
            _ => (),
        }
        buf.clear();
    } // loop
    let rs = builder.string();

    rs.unwrap()
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
                    match a.unescape_value() {
                        Ok(value) => {
                            hashmap.insert(name, value.to_string());
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to unescape attribute '{}' value, using raw value: `{}` error: {}", name, String::from_utf8_lossy(&a.value).to_string(), e);
                            // Fall back to raw value without unescaping
                            hashmap.insert(name, String::from_utf8_lossy(&a.value).to_string());
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
