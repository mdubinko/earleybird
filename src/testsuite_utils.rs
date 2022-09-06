use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::str::from_utf8;

use itertools::Itertools;
use quick_xml::events::Event;
use quick_xml::events::attributes::{Attributes, AttrError};
use quick_xml::name::QName;
use quick_xml::reader::Reader;
use string_builder::Builder;

type XmlString = String;

static TEMP_BASE_URL: &str = "../../ixml/tests/";

/// relative to the home directory of the test sutes, all known test-catalog documents
static TEST_CATALOGS: [&str; 16] = [
    "test-catalog.xml",
    "syntax/catalog-as-grammar-tests.xml",
    "syntax/catalog-as-instance-tests-ixml.xml",
    "syntax/catalog-as-instance-tests-xml.xml",
    "syntax/catalog-of-correct-tests.xml",
    "ambiguous/test-catalog.xml",
    "correct/test-catalog.xml",
    "ixml/test-catalog.xml",
    "parse/test-catalog.xml",
    "error/test-catalog.xml",
    "grammar-misc/test-catalog.xml",
    "grammar-misc/prolog-tests.xml",
    "grammar-misc/insertion-tests.xml",
    "misc/misc-001-020-catalog.xml",
    "misc/misc-021-040-catalog.xml",
    "misc/misc-041-060-catalog.xml",
];

#[derive(Clone, Debug)]
/// a test case. We duplicate the grammar (from parent test-set) if needed, for one-stop shopping
pub struct TestCase {
    pub name: String,
    pub grammar: String,
    pub input: String,
    /// normally only a single expected result, except in ambiguous tests
    pub expected: Vec<TestResult>,
}

#[derive(Clone, Debug)]
pub enum TestResult {
    AssertNotASentence,
    AssertDynamicError(String), // error code, e.g. "D01", "D02", ...
    AssertXml(XmlString),
}

struct TestCaseBuilder {
    pub name: Option<String>,
    pub grammar: Option<String>,
    pub input: Option<String>,
    pub expected: Vec<TestResult>,
}

impl TestCaseBuilder {
    fn new() -> Self {
        TestCaseBuilder { name: None, grammar: None, input: None, expected: Vec::new() }
    }

    /// Build the [TestCase]. Resets the builder.
    fn build(&mut self) -> TestCase {
        assert!(self.name.is_some());
        assert!(self.grammar.is_some());
        assert!(self.input.is_some());
        assert!(self.expected.len() > 0);
        let name = self.name.take();
        self.name = None;
        let grammar = self.grammar.take();
        self.grammar = None;
        let input = self.input.take();
        self.input = None;
        let expected = self.expected.drain(..).collect();
        self.expected.clear();
        println!("built test case ===={}====", name.clone().unwrap());

        TestCase { name: name.unwrap(), grammar: grammar.unwrap(), input: input.unwrap(), expected }
    }
}

pub static TEST_CATALOG_EG: &str =
r##"<test-catalog xmlns='https://github.com/invisibleXML/ixml/test-catalog'>
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


pub fn read_test_catalog(path: String) -> Vec<TestCase> {
    let pathbuf = PathBuf::from(&path);
    let basepath = pathbuf.parent().unwrap();
    println!("{}", basepath.to_string_lossy());
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
                    },
                    b"ixml-grammar" => {
                        let grammar = reader.read_text(e.to_end().name());
                        current_grammar = grammar.expect("parse error reading inline grammar").to_string();
                    },
                    b"ixml-grammar-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(href);
                        //println!("ixml-grammar-ref {}", fullpath.to_string_lossy());
                        current_grammar = fs::read_to_string(fullpath).expect("Error reading grammar file");
                    },
                    b"test-case" => {
                        let name = attr_by_name(&e.attributes(), "name");
                        builder = TestCaseBuilder::new();
                        let mut fullname = test_set_nesting.join("/");
                        fullname.push('/');
                        fullname.push_str(&name);
                        builder.name = Some(fullname);
                        builder.grammar = Some(current_grammar.clone());
                    },
                    b"test-case-ref" => {
                        // TODO: maybe just note these somewhere...
                    },
                    b"test-string" => {
                        let input = reader.read_text(e.to_end().name());
                        builder.input = Some(input.expect("parse error reading inline test-string").to_string());
                    },
                    b"test-string-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(href);
                        //println!("test-string-ref {}", fullpath.to_string_lossy());
                        builder.input = Some(fs::read_to_string(fullpath).expect("Error reading grammar file"));
                    },
                    b"assert-not-a-sentence" => {
                        builder.expected.push(TestResult::AssertNotASentence);
                    },
                    b"assert-dynamic-error" => {
                        let codes = attr_by_name(&e.attributes(), "code");
                        for code in codes.split(' ') {
                            builder.expected.push(TestResult::AssertDynamicError(String::from(code)));
                        }
                    },
                    b"assert-xml" => {
                        //let inner_content = reader.read_to_end(e.to_end().name());
                        //builder.expected.push(TestResult::AssertXml(from_utf8(inner_content.expect("Error reading inline assert-xml")).unwrap()));
                        enable_accum = true;
                    },
                    b"assert-xml-ref" => {
                        let href = attr_by_name(&e.attributes(), "href");
                        let mut fullpath = basepath.to_path_buf();
                        fullpath.push(href);
                        //println!("assert-xml-ref {}", fullpath.to_string_lossy());
                        builder.expected.push(TestResult::AssertXml(fs::read_to_string(fullpath).expect("Error reading assert-xml file")));
                    }
                    _ => {
                        if enable_accum {
                            raw_xml_accum.push('<' as u8);
                            raw_xml_accum.extend(e.iter());
                            raw_xml_accum.push('>' as u8);
                        }
                    },
                }
            }
            Ok(Event::Text(t)) => {
                if enable_accum {
                    raw_xml_accum.extend(t.iter());
                }
                
            },
            Ok(Event::End(e)) => {
                match e.name().as_ref() {
                    b"test-set" => {
                       test_set_nesting.pop();
                    },
                    b"test-case" => {
                        test_cases.push(builder.build());
                    },
                    b"assert-xml" => {
                        enable_accum = false;
                        let xml_string = from_utf8(&raw_xml_accum).expect("UTF-8 error in assert-xml").to_string();
                        //println!("assert-xml literal {xml_string}");
                        builder.expected.push(TestResult::AssertXml(xml_string));
                    }
                    _ => {
                        if enable_accum {
                            raw_xml_accum.push('<' as u8);
                            raw_xml_accum.push('/' as u8);
                            raw_xml_accum.extend(e.iter());
                            raw_xml_accum.push('>' as u8);
                        }
                    }
                }
            },
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
    println!("read {} cases", test_cases.len());
    test_cases
}

/// Not "Canonical XML" but close enough for our purposes here
/// Formats an XML document in a conveniently-diffable format
/// Not namespace-aware, and does its own thing with newlines
pub fn xml_canonicalize(input_xml: &str) -> String {
    let mut builder = Builder::default();
    
    let mut reader = Reader::from_str(&input_xml);
    reader.trim_text(true);
    reader.expand_empty_elements(true);

    let mut buf = Vec::new();
    
    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            // exits the loop when reaching end of file
            Ok(Event::Eof) => break,

            Ok(Event::Start(e)) => {
                let attrs = all_attrs(e.attributes());
                builder.append("<");
                builder.append(from_utf8(e.name().into_inner()).expect("UTF-8 parse error on element start"));
                if attrs.len() > 0 {
                    for (k,v) in attrs.into_iter().sorted() {
                        builder.append(" ");
                        builder.append(k);
                        builder.append("=\"");
                        builder.append(v.replace("\"", "&quot;"));
                        builder.append("\"")
                    }
                }
                builder.append("\n>");
            }
            Ok(Event::Text(t)) => {
                builder.append(t.unescape().expect("UTF-8 parse error on text").to_string().replace("<", "&lt;"));
            },
            Ok(Event::End(e)) => {
                builder.append("</");
                builder.append(from_utf8(&e.into_owned()).expect("UTF-8 parse error on element close"));
                builder.append("\n>");
            },
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
    attrs.clone()
        .filter(|a| if let Ok(f) = a {f.key==qname } else { false })
        .map(|a| String::from(from_utf8(a.unwrap().value.as_ref()).unwrap()))
        .collect()
}

/// Just grab all the attributes as a HashMap
/// Assumes everything here is UTF-8 valid, otherwise panics
/// Silently deletes the xmlns pseudo-attribute
fn all_attrs(attrs: Attributes) -> HashMap<String, String> {
    let mut hashmap: HashMap<String, String> = HashMap::new();
    for attr in attrs {
        let _ = match attr {
            Ok(a) => {
                let name: String = from_utf8(&a.key.into_inner()).expect("UTF-8 error parsing attribute name").to_string();
                if name != "xmlns" {
                    hashmap.insert(name, a.unescape_value().expect("UTF-8 error parsing attribute value").to_string());
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

#[test]
fn test_canonize_xml() {
    // N.B. extra various whitespace, attribute order, single vs double quotes, char entities
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