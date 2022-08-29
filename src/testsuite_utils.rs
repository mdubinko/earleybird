use std::ffi::OsString;
use quick_xml::{de::from_str, DeError};
use serde::Deserialize;
type URL = String;
type XmlString = String;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
/// xmlns="https://github.com/invisibleXML/ixml/test-catalog"
pub struct TestCatalog {
    description: Option<String>,
    #[serde(rename = "test-set")]
    inline_test_sets: Vec<TestSetInline>,
    #[serde(rename = "test-set-ref")]
    ref_test_sets: Option<Vec<URL>>,
    //grammar_test_sets: Vec<GrammarTestSet>,
}

#[derive(Debug, Deserialize)]
/// A <test-set>. The test grammar permits nested test-sets, which we're ignoring for now.
/// Instead, treating everyting as a flattened set of test-sets
pub struct TestSetInline {
    //#[serde(rename = "grammar-ref")]
    //ixml_grammar_ref: URL,
    #[serde(rename = "test-case")]
    inline_cases: Vec<TestCaseInline>,
    #[serde(rename = "test-case-ref")]
    ref_cases: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
/// a <test-case>
pub struct TestCaseInline {
    name: String,
    #[serde(rename = "test-string")]
    inline_input: Option<String>,
    #[serde(rename = "test-string-ref")]
    ref_input: Option<URL>,

    /// normally only a single expected result, except in ambiguous tests
    #[serde(rename = "assert-xml")]
    inline_expected: Option<Vec<String>>,
    #[serde(rename = "assert-xml-ref")]
    ref_expected: Option<Vec<URL>>,
    #[serde(rename = "assert-not-a-sentence")]
    assert_not_a_sentence: Option<NotASentence>,
}

#[derive(Debug, Deserialize)]
pub enum NotASentence {
    Empty
}

#[derive(Debug, Deserialize)]
pub enum TestResult {
    #[serde(rename = "assert-not-a-sentence")]
    AssertNotASentence,
    #[serde(rename = "assert-xml-ref")]
    AssertXmlHref(URL),
    #[serde(rename = "assert-xml")]
    AssertXml(XmlString),
}

mod arbitrary_xml {
    use serde::{self, Deserialize, Serializer, Deserializer};

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

#[test]
fn test_serde_deserialize() {
    let test_catalog: Result<TestCatalog, DeError> = from_str(TEST_CATALOG_EG);
    println!("{:?}", test_catalog);
    assert!(test_catalog.is_ok());
}