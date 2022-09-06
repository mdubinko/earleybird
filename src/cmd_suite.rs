
use argh::FromArgs;
use earleybird::{testsuite_utils::{self, xml_canonicalize}, grammar::Grammar, parser::Parser, ixml_grammar::ixml_tree_to_grammar};
use crate::cmd_suite::testsuite_utils::TestResult::*;

#[derive(FromArgs)]
/// Run the test suite in the specified directory
#[argh(subcommand, name = "suite")]
pub struct RunSuite {
    /// directory containing test suite
    #[argh(positional)]
    dir: String,
}

impl RunSuite {
    pub fn run(self) {
        println!("_{}_", self.dir);

        let tests = testsuite_utils::read_test_catalog(self.dir);

        // stats
        let mut count = 0;
        let mut pass = 0;
        let mut fail = 0;
        let mut abort = 0;
        let mut failures: Vec<String> = Vec::new();

        for test in tests {
            let name = test.name;
            println!("Test {name}");

            count += 1;
            let grammar = test.grammar;
            let ixml_grammar = Grammar::new("ixml");
            let mut grammar_parser = Parser::new(ixml_grammar);
            grammar_parser.parse(&grammar);
            let target_grammar_tree = grammar_parser.unpack_parse_tree("ixml");
            let target_grammar = ixml_tree_to_grammar(&target_grammar_tree, "ixml");

            let target_root: String = target_grammar.get_root_definition_name().to_string();
            let mut target_parser = Parser::new(target_grammar);
            let input = test.input;
            target_parser.parse(&input);
            let target_tree = target_parser.unpack_parse_tree(&target_root);
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
                }
            }


        }

        println!("Total tests: {count}. ({pass} passed, {fail} failed, {abort} aborted)");
    }
}
