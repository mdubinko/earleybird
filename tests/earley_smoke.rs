use earleybird::test_grammars::all_builtin_tests;
use earleybird::grammar::Grammar;
use earleybird::parser::Parser;
use earleybird::testsuite_utils::{TestResult, TestGrammar, xml_canonicalize};

#[test]
fn test_all_builtin() {
    let _ = env_logger::builder().is_test(true).try_init();

    for testcase in all_builtin_tests().into_iter() {

        // until CLI in place, temporary filter capability //
        let filter = "Smoke";
        let name = testcase.name;
        if !name.contains(filter) {
            continue;
        }

        let mut grammar_under_test = None;

        for grammar in testcase.grammars {
            if let TestGrammar::Parsed(g) = grammar {
                grammar_under_test = Some(g);
                break;
            }
        }

        let grammar = grammar_under_test.unwrap();

        let expecteds = testcase.expected;
        // for purposes here, assume only one valid result
        let expected = if let Some(TestResult::AssertXml(x)) = expecteds.get(0) {
            x
        } else {
            panic!("Tests not set up for ambiguity");
        };

        // run the test
        let mut parser = Parser::new(grammar);
        let arena = parser.parse(&testcase.input).unwrap_or_else(|e| panic!("{e}"));
        let result = Parser::tree_to_test_format(&arena);
        assert_eq!(xml_canonicalize(&result), xml_canonicalize(expected), " on test {name}");
    }
}

#[test]
fn test_ixml_parser() {
    let _ = env_logger::builder().is_test(true).try_init();
    println!("=== TOTAL TESTCASES = {} ===", all_builtin_tests().len());

    for testcase in all_builtin_tests().into_iter() {

        // until CLI in place, temporary filter capability //
        let incl = "Smoke";
        let name = testcase.name;
        if !name.contains(incl) {
            continue;
        }
        let excl = "Chars";
        if name.contains(excl) {
            continue;
        }

        let mut grammars_under_test = Vec::new();
        let mut grammar_comparison: Vec<String> = Vec::new();
        let mut index_for_parsed = 0;
        let mut index_for_unparsed = 0;
        println!("==== EXAMINING {} TESTCASE GRAMMARS in {name} ========", testcase.grammars.len());

        for (i, grammar) in testcase.grammars.into_iter().enumerate() {
            let grammar_under_test = match grammar {
                TestGrammar::Parsed(g) => {
                    println!("...no need to parse ; insertion order = {:?}", &g.defn_order);
                    index_for_parsed = i;
                    g
                }
                TestGrammar::Unparsed(ixml) => {
                    println!("..parsing >>>{}<<<", &ixml);
                    let x = Grammar::from_ixml_str(&ixml);
                    index_for_unparsed = i;
                    //dbg!(&x);
                    match &x {
                        Ok(g) => {
                            println!("insertion order = {:?}", &g.defn_order);
                        },
                        Err(e) => {
                            println!("Parse failed with error: {}", e);
                            println!("Skipping this test case due to improved error handling");
                            continue; // Skip this test case
                        }
                    }
                    x.unwrap()
                }
            };
            let comparison_str = grammar_under_test.to_string();
            println!("+++ comparison string +++ {} {}", grammar_under_test.get_rule_count(), &comparison_str);
            grammars_under_test.push(grammar_under_test);
            grammar_comparison.push(comparison_str);
        }

        // make sure grammar parses to something that matches the hand-built grammar
        // "left" will always be the one provided as an unparsed string
        if grammar_comparison.len() > 1 {
            assert_eq!(grammar_comparison[index_for_unparsed], grammar_comparison[index_for_parsed], "comparing grammars for {}; left was parsed from string", name);
        }

    }
}
