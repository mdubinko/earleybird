use earleybird::grammar::Mark;
use earleybird::parser::{Parser, ParseError};
use earleybird::builtin_grammars::*;
use earleybird::testsuite_utils::{TestResult, TestGrammar, xml_canonicalize};
use smol_str::SmolStr;

#[test]
fn test_all_builtin() {

    for testcase in all_builtin_tests().into_iter() {

        // until CLI in place, temporary filter capability //
        let filter = "Smoke";
        let name = testcase.name;
        if !name.contains(filter) {
            continue;
        }

        let input = testcase.input;
        let grammar = if let TestGrammar::Parsed(g) = testcase.grammar {
            g
        } else {
            panic!("Invalid test setup");
        };
        let expecteds = testcase.expected;
        // for purposes here, assume only one valid result
        let expected = if let Some(TestResult::AssertXml(x)) = expecteds.get(0) {
            x
        } else {
            panic!("Tests not set up for ambiguity");
        };

        // run the test
        let mut parser = Parser::new(grammar);
        let arena = parser.parse(&input).unwrap_or_else(|e| panic!("{e}"));
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(xml_canonicalize(&result), xml_canonicalize(expected), " on test {name}");
    }
}
