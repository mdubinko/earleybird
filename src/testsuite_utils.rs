use std::ffi::OsString;
use quick_xml::{de::from_str, DeError};
use serde::Deserialize;
type URL = String;
type XmlString = String;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// xmlns="https://github.com/invisibleXML/ixml/test-catalog"
pub struct TestCatalog {
    //description: String,
    #[serde(alias = "test-set", alias = "test-set-ref")]
    test_sets: Vec<TestSet>,
    //grammar_test_sets: Vec<GrammarTestSet>,
}

#[derive(Debug, Deserialize)]
/// A test-set, either inline, or referenced via URL
pub enum TestSet {
    #[serde(rename = "test-set-ref")]
    Href(URL),
    #[serde(rename = "test-set")]
    Inline(TestSetInline),
}

#[derive(Debug, Deserialize)]
/// A <test-set>. The test grammar permits nested test-sets, which we're ignoring for now.
/// Instead, treating everyting as a flattened set of test-sets
pub struct TestSetInline {
    //#[serde(rename = "grammar-ref")]
    //ixml_grammar_ref: URL,
    #[serde(alias = "test-case", alias = "test-case-ref")]
    cases: Vec<TestCase>,
}

#[derive(Debug, Deserialize)]
/// an ixml grammar, either inline or referenced via URL
pub enum IxmlGrammar {
    Href(URL),
    Inline(String),
}

#[derive(Debug, Deserialize)]
pub enum TestCase {
    #[serde(rename = "test-case-ref")]
    Href(URL),
    #[serde(rename = "test-case")]
    Inline(TestCaseInline)
}

#[derive(Debug, Deserialize)]
/// a <test-case>
pub struct TestCaseInline {
    name: String,
    #[serde(alias = "test-string", alias = "test-string-ref")]
    input: TestInput,
    /// normally only a single expected result, except in ambiguous tests
    #[serde(alias = "assert-xml", alias = "assert-xml-ref", alias = "assert-not-a-sentence")]
    expected: Vec<TestResult>,
}

#[derive(Debug, Deserialize)]
pub enum TestInput {
    #[serde(rename = "test-string-ref")]
    Href(URL),
    #[serde(rename = "test-string")]
    Inline(String),
}

#[derive(Debug, Deserialize)]
pub enum TestResult {
    #[serde(rename = "assert-not-a-sentence")]
    AssertNotASentence,
    #[serde(rename = "assert-xml-ref")]
    AssertXmlHref(URL),
    #[serde(rename = "$unflatten=assert-xml")]
    AssertXml(XmlString),
}

mod arbitrary_xml {
    use serde::{self, Deserialize, Serializer, Deserializer};

    const FORMAT: &'static str = "%Y-%m-%d %H:%M:%S";

    // The signature of a serialize_with function must follow the pattern:
    //
    //    fn serialize<S>(&T, S) -> Result<S::Ok, S::Error>
    //    where
    //        S: Serializer
    //
    // although it may also be generic over the input types T.
    pub fn serialize<S>(content: &String, serializer: S,) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = format!("{}", content);
        serializer.serialize_str(&s)
    }

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        println!("here! {}", "!");
        let s = String::deserialize(deserializer);
        println!(" xxx{:?}xxx", s);
        s
        //Utc.datetime_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
    }
}

    // to figure out:
    // 1) how to handle multiple levels of <test-set>
    // 2) how to get XML content inside <assert-xml>
    // 3) how to make sure alternatives are working e.g. <test-string> vs <test-string-ref>
    // 4) put this sample XML below in a test case somewhere

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
      <assert-xml><x>ok</x></assert-xml>
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

#[test]
fn test_serde_deserialize() {
    let test_catalog: Result<TestCatalog, DeError> = from_str(TEST_CATALOG_EG);
    println!("{:?}", test_catalog);
    assert!(test_catalog.is_ok());
}