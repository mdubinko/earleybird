
use argh::FromArgs;
use earleybird::{testsuite_utils::{self, xml_canonicalize, TestGrammar}, parser::Parser, ixml_grammar::{ixml_tree_to_grammar, self}};
use crate::cmd_suite::testsuite_utils::TestResult::*;

#[derive(FromArgs)]
/// Run the test suite in the specified directory
#[argh(subcommand, name = "suite")]
pub struct RunSuite {
    /// directory containing test suite
    #[argh(positional)]
    dir: String,
}

fn run(dir: String) {
    println!("_{}_", dir);

    let tests = testsuite_utils::read_test_catalog(dir);

    // stats
    let mut count = 0;
    let mut pass = 0;
    let mut fail = 0;
    let mut abort = 0;
    let mut failures: Vec<String> = Vec::new();

    for test in tests {
        let name = test.name;
        println!("🧪 Test {name}");

        count += 1;
        let grammar = test.grammar;
        println!("{grammar}");
        let ixml_grammar = ixml_grammar::grammar();
        let mut grammar_parser = Parser::new(ixml_grammar);
        let target_grammar = match grammar {
            TestGrammar::Parsed(g) => g,
            TestGrammar::Unparsed(g) => {
                let target_grammar_tree = match grammar_parser.parse(&g) {
                    Ok(tree) => tree,
                    Err(e) => {
                        println!("{e}");
                        fail += 1;
                        failures.push(name);
                        continue;
                    }
                };
                ixml_tree_to_grammar(&target_grammar_tree)
            }
        };

        let mut target_parser = Parser::new(target_grammar);
        let input = test.input;
        let target_tree = match target_parser.parse(&input) {
            Ok(tree) => tree,
            Err(e) => {
                println!("{e}");
                fail += 1;
                failures.push(name);
                break;
            }
        };
        let target_xml = Parser::tree_to_testfmt(&target_tree);

        let expecteds = test.expected;

        // TODO: package this better
        for expected in expecteds {
            let passed = match expected {
                AssertNotASentence => (todo!()),
                AssertDynamicError(de) => todo!(),
                AssertXml(x) => {
                    xml_canonicalize(&target_xml) == xml_canonicalize(&x)
                }
            };
            if passed {
                pass += 1;
            } else {
                fail += 1;
                failures.push(name.clone());
            }
        }
    }

    println!("Total tests: {count}. ({pass} passed, {fail} failed, {abort} aborted)");
    println!("Failures:");
    println!("{}", failures.join("\n"));
}

impl RunSuite {
    pub fn run(self) {
        let _result = run(self.dir);
    }
}
