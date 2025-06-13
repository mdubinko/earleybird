
use argh::FromArgs;
use earleybird::{testsuite_utils::{self, xml_canonicalize, TestGrammar}, parser::Parser, grammar::{Grammar, RuleContext}};
use crate::cmd_suite::testsuite_utils::TestResult::*;
use std::path::{Path, PathBuf};
use std::process;

/// Resolve a suite specification to a catalog path and filter string
/// Examples:
/// - None -> use master catalog, no filter (loads all suites)
/// - "syntax" -> use master catalog, filter test names containing "syntax"
/// - "correct" -> use master catalog, filter test names containing "correct"
/// - "misc" -> use master catalog, filter test names containing "misc"
fn resolve_suite_spec(suite_spec: Option<String>) -> (String, Option<String>) {
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let ixml_path = current_dir.join("ixml/tests");
    
    match suite_spec {
        None => {
            // Default: use master catalog, no filtering
            (ixml_path.join("test-catalog.xml").to_string_lossy().to_string(), None)
        }
        Some(filter) => {
            // Always use master catalog, but filter by the provided string
            (ixml_path.join("test-catalog.xml").to_string_lossy().to_string(), Some(filter))
        }
    }
}

#[derive(FromArgs)]
/// Run the test suite
#[argh(subcommand, name = "suite")]
pub struct RunSuite {
    /// optional specific suite to run (default: all suites)
    #[argh(positional)]
    suite: Option<String>,
}

fn run(suite_spec: Option<String>) {
    let (catalog_path, filter) = resolve_suite_spec(suite_spec);
    println!("Running tests from: {}", catalog_path);

    let all_tests = testsuite_utils::read_test_catalog(catalog_path);
    let filtered_tests = match filter {
        Some(filter_str) => {
            println!("Filtering tests containing: '{}'", filter_str);
            all_tests.into_iter()
                .filter(|test| test.name.contains(&filter_str))
                .collect()
        }
        None => all_tests
    };
    println!("Loaded {} test cases", filtered_tests.len());

    // stats
    let mut count = 0;
    let mut pass = 0;
    let mut fail = 0;
    let mut abort = 0;
    let mut failures: Vec<String> = Vec::new();

    for test in filtered_tests {
        let name = test.name;
        println!("🧪 Test {name}");

        count += 1;
        let grammar = test.grammars.into_iter().next().expect("no grammars available for this test");
        println!("{grammar}");
        let target_grammar = match grammar {
            TestGrammar::Parsed(g) => g,
            TestGrammar::Unparsed(ixml) => {
                Grammar::from_ixml_str(&ixml).unwrap_or_else(|e| {
                     abort+=1;
                     failures.push(name.clone());
                     println!("{e}");
                     let mut g = Grammar::new();
                     let ctx = RuleContext::new("error");
                     g.define("error", ctx.seq().ch_in(&e.to_string())); // hack
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
        let target_xml = Parser::tree_to_test_format(&target_tree);

        let expecteds = test.expected;

        // TODO: package this better
        for expected in expecteds {
            let passed = match expected {
                AssertNotASentence => todo!(),
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
        // Check if the ixml directory exists
        let current_dir: PathBuf = std::env::current_dir().expect("Failed to get current directory");
        let ixml_path: PathBuf = current_dir.join("ixml");
      
        if !Path::new(&ixml_path).exists() {
            eprintln!("Error: ixml directory not found");
            eprintln!("Place the official ixml repo (or a symlink to it) at ./ixml/");
            process::exit(1);
        }

        let _result = run(self.suite);
    }
}
