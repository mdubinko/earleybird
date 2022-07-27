use earleybird::parser::{Grammar, Parser};
use earleybird::builtin_grammars::*;

#[test]
fn run_suite1() {
    let inputs = Suite1::get_inputs();
    let expecteds = Suite1::get_expected();
    for i in 0..inputs.len() {
        let g = Suite1::get_grammar();
        let mut parser = Parser::new(g);
        let trace = parser.parse(inputs[i], "doc");
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

#[test]
fn run_suite2() {
    let inputs = Suite2::get_inputs();
    let expecteds = Suite2::get_expected();
    for i in 0..inputs.len() {
        let g = Suite2::get_grammar();
        let mut parser = Parser::new(g);
        let trace = parser.parse(inputs[i], "doc");
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

#[test]
fn run_suite3() {
    let inputs = Suite3::get_inputs();
    let expecteds = Suite3::get_expected();
    for i in 0..inputs.len() {
        let g = Suite3::get_grammar();
        let mut parser = Parser::new(g);
        let trace = parser.parse(inputs[i], "doc");
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

#[test]
fn run_suite4() {
    let inputs = Suite4::get_inputs();
    let expecteds = Suite4::get_expected();
    for i in 0..inputs.len() {
        let g = Suite4::get_grammar();
        let mut parser = Parser::new(g);
        let trace = parser.parse(inputs[i], "doc");
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

#[test]
fn run_suite_wiki() {
    let inputs = SuiteWiki::get_inputs();
    let expecteds = SuiteWiki::get_expected();
    for i in 0..inputs.len() {
        let g = SuiteWiki::get_grammar();
        let mut parser = Parser::new(g);
        let trace = parser.parse(inputs[i], "doc");
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

