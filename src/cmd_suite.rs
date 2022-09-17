
use argh::FromArgs;
use earleybird::{testsuite_utils::{self, xml_canonicalize, TestGrammar}, parser::Parser, ixml_grammar::ixml_str_to_grammar, grammar::{Grammar, Rule}};
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
        let grammar = test.grammars.into_iter().next().expect("no grammars available for this test");
        println!("{grammar}");
        let target_grammar = match grammar {
            TestGrammar::Parsed(g) => g,
            TestGrammar::Unparsed(ixml) => {
                ixml_str_to_grammar(&ixml).unwrap_or_else(|e| {
                     abort+=1;
                     failures.push(name.clone());
                     println!("{e}");
                     let mut g = Grammar::new();
                     g.define("error", Rule::seq().ch_in(&e.to_string())); // hack
                     g
                })
            }
        };

        let mut target_parser = Parser::new(target_grammar);
        let input = test.input;
        let target_tree = match target_parser.parse(&input) {
            Ok(tree) => tree,
            Err(e) => {
                println!("{e}");
                fail += 1;
                failures.push(name.clone());
                break;
            }
        };
        let target_xml = Parser::tree_to_testfmt(&target_tree);

        let expecteds = test.expected;

        // TODO: package this better
        for expected in expecteds {
            let passed = match expected {
                AssertNotASentence => (todo!()),
                AssertDynamicError(_de) => todo!(),
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
