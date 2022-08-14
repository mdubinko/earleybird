use earleybird::parser::Parser;
use earleybird::builtin_grammars::*;

#[test]
fn run_smoke_seq() {
    let inputs = SmokeSeq::get_inputs();
    let expecteds = SmokeSeq::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeSeq::get_grammar();
        let mut parser = Parser::new(g);
        let _trace = parser.parse(inputs[i]);
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

#[test]
fn run_smoke_alt() {
    let inputs = SmokeAlt::get_inputs();
    let expecteds = SmokeAlt::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeAlt::get_grammar();
        let mut parser = Parser::new(g);
        let _trace = parser.parse(inputs[i]);
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

#[test]
fn run_smoke_nt() {
    let inputs = SmokeNT::get_inputs();
    let expecteds = SmokeNT::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeNT::get_grammar();
        let mut parser = Parser::new(g);
        let _trace = parser.parse(inputs[i]);
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

#[test]
fn run_smoke_opt() {
    let inputs = SmokeOpt::get_inputs();
    let expecteds = SmokeOpt::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeOpt::get_grammar();
        let mut parser = Parser::new(g);
        let _trace = parser.parse(inputs[i]);
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

#[test]
fn run_smoke_star() {
    let inputs = SmokeStar::get_inputs();
    let expecteds = SmokeStar::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeStar::get_grammar();
        println!("  Against grammar:\n{:?}", g);
        let mut parser = Parser::new(g);
        let _trace = parser.parse(inputs[i]);
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

#[test]
fn run_suite_wiki() {
    let inputs = SuiteWiki::get_inputs();
    let expecteds = SuiteWiki::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SuiteWiki::get_grammar();
        let mut parser = Parser::new(g);
        let _trace = parser.parse(inputs[i]);
        let result = parser.unpack_parse_tree("doc");
        assert_eq!(expecteds[i], result);
    }
}

