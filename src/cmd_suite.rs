
use argh::FromArgs;
use earleybird::{testsuite_utils::{self, xml_canonicalize, TestGrammar, TestOutcome}, parser::Parser, grammar::Grammar, debug::DebugLevel};
use crate::cmd_suite::testsuite_utils::TestResult::*;
use std::path::{Path, PathBuf};
use std::process;
use std::fs::OpenOptions;
use std::io::Write;
use std::collections::HashSet;

// Helper functions for parsing case-insensitive CLI options
fn parse_level(level_str: &str) -> Result<DebugLevel, String> {
    match level_str.to_uppercase().as_str() {
        "DEBUG" | "ALL" => Ok(DebugLevel::Trace),
        "INFO" => Ok(DebugLevel::Detailed),
        "SUMMARY" => Ok(DebugLevel::Basic),
        "WARNING" => Ok(DebugLevel::Basic),
        "ERROR" => Ok(DebugLevel::Basic),
        "NONE" | "OFF" => Ok(DebugLevel::Off),
        "FAILURES" => Ok(DebugLevel::Basic), // Special case for file output
        _ => Err(format!("Invalid level: {}. Valid levels: DEBUG, INFO, SUMMARY, WARNING, ERROR, ALL, NONE, OFF, FAILURES", level_str))
    }
}

fn parse_categories(categories_str: Option<&String>) -> Result<Option<HashSet<String>>, String> {
    match categories_str {
        None => Ok(None),
        Some(cats) => {
            let mut category_set = HashSet::new();
            for cat in cats.split(',') {
                let cat_upper = cat.trim().to_uppercase();
                match cat_upper.as_str() {
                    "BOOTSTRAP" | "QUEUE" | "SCANNER" | "OUTPUT" |
                    "PREDICT" | "COMPLETE" | "DEDUP" | "GRAMMAR" => {
                        category_set.insert(cat_upper);
                    }
                    _ => return Err(format!("Invalid category: {}. Valid categories: BOOTSTRAP, QUEUE, SCANNER, OUTPUT, PREDICT, COMPLETE, DEDUP, GRAMMAR", cat))
                }
            }
            Ok(Some(category_set))
        }
    }
}

/// Parse with built-in trace size limit to prevent infinite loops
fn parse_with_trace_limit(parser: &mut Parser, input: &str) -> Result<indextree::Arena<earleybird::parser::Content>, earleybird::parser::ParseError> {
    // The parser now has built-in trace size limit protection
    parser.parse(input)
}

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

    /// console output level: DEBUG|INFO|SUMMARY|WARNING|ERROR|ALL|NONE|OFF
    #[argh(option, long = "console", default = "String::from(\"INFO\")")]
    console: String,

    /// console output categories: BOOTSTRAP,QUEUE,SCANNER,OUTPUT,PREDICT,COMPLETE,DEDUP,GRAMMAR
    #[argh(option, long = "console-filter")]
    console_filter: Option<String>,

    /// file output level: DEBUG|INFO|SUMMARY|WARNING|ERROR|ALL|NONE|OFF|FAILURES
    #[argh(option, long = "file", default = "String::from(\"FAILURES\")")]
    file: String,

    /// file output categories: BOOTSTRAP,QUEUE,SCANNER,OUTPUT,PREDICT,COMPLETE,DEDUP,GRAMMAR
    #[argh(option, long = "file-filter")]
    file_filter: Option<String>,

    /// output filename for results
    #[argh(option, short = 'o', long = "output", default = "String::from(\"log/conformance-results.txt\")")]
    output: String,
}

fn run(suite_spec: Option<String>, console: &str, _console_filter: Option<&String>, file: &str, _file_filter: Option<&String>, output_file: &str) {
    let (catalog_path, filter) = resolve_suite_spec(suite_spec);
    println!("Running tests from: {}", catalog_path);

    let all_tests = testsuite_utils::read_test_catalog(catalog_path.clone());
    let filtered_tests = match &filter {
        Some(filter_str) => {
            println!("Filtering tests containing: '{}'", filter_str);
            all_tests.into_iter()
                .filter(|test| test.name.contains(filter_str))
                .collect()
        }
        None => all_tests
    };

    println!("Loaded {} test cases", filtered_tests.len());

    // Parse levels for internal use
    let console_level = parse_level(console).unwrap_or(DebugLevel::Basic);
    let file_level = parse_level(file).unwrap_or(DebugLevel::Off);

    // Handle file output based on file level
    let mut file_writer: Option<std::fs::File> = if file_level == DebugLevel::Off {
        if console_level != DebugLevel::Off {
            println!("File output disabled");
        }
        None
    } else {
        if console_level != DebugLevel::Off {
            println!("Writing results to: {}", output_file);
        }
        // Create parent directory if it doesn't exist
        if let Some(parent) = Path::new(output_file).parent() {
            std::fs::create_dir_all(parent).expect("Could not create output directory");
        }
        Some(OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(output_file)
            .expect("Could not create output file"))
    };

    // Write file header if file output enabled
    if let Some(ref mut file) = file_writer {
        writeln!(file, "=== iXML Conformance Test Results ===").unwrap();
        writeln!(file, "Test catalog: {}", catalog_path).unwrap();
        writeln!(file, "Filter: {:?}", filter).unwrap();
        writeln!(file, "Total tests: {}", filtered_tests.len()).unwrap();
        writeln!(file, "").unwrap();
    }


    // Statistics
    let mut stats = std::collections::HashMap::new();
    let mut count = 0;

    for test in filtered_tests {
        count += 1;
        let test_name = test.name.clone();

        // Print test start based on console level
        match console_level {
            DebugLevel::Trace | DebugLevel::Detailed => {
                print!("🧪 Test {test_name} ... ");
                std::io::stdout().flush().unwrap();
            }
            DebugLevel::Basic | DebugLevel::Off => {
                // Don't print individual test progress for SUMMARY or OFF
            }
        }

        let outcome = run_single_test(test);

        // Update statistics
        let category = match &outcome {
            TestOutcome::Pass => "pass",
            TestOutcome::Fail { .. } => "fail",
            TestOutcome::ValidationError(_) => "validation_error",
            TestOutcome::BootstrapParseError(_) => "bootstrap_error",
            TestOutcome::ConversionError(_) => "conversion_error",
            TestOutcome::InputParseError(_) => "parse_error",
            TestOutcome::Panic(_) => "panic",
            TestOutcome::Skip(_) => "skip",
            TestOutcome::Todo(_) => "todo",
            #[allow(deprecated)]
            TestOutcome::GrammarParseError(_) => "grammar_error", // Legacy
        };
        *stats.entry(category).or_insert(0) += 1;

        // Determine what to write to file
        let should_write_to_file = match file_level {
            DebugLevel::Off => false,
            DebugLevel::Trace | DebugLevel::Detailed => true, // ALL cases
            DebugLevel::Basic => {
                // For FAILURES level, only write non-pass results
                if file == "FAILURES" {
                    !matches!(outcome, TestOutcome::Pass)
                } else {
                    true
                }
            }
        };

        // Print result based on console level
        let should_print_result = match console_level {
            DebugLevel::Trace | DebugLevel::Detailed => true, // DEBUG/INFO: show all
            DebugLevel::Basic => false, // SUMMARY: don't show individual results
            DebugLevel::Off => false, // NONE/OFF: silent
        };

        // Write result to file and/or print status
        match &outcome {
            TestOutcome::Pass => {
                if should_print_result {
                    println!("✅ PASS");
                }
                if should_write_to_file && file_writer.is_some() {
                    writeln!(file_writer.as_mut().unwrap(), "PASS {}", test_name).unwrap();
                }
            }
            TestOutcome::Fail { expected, actual } => {
                if should_print_result {
                    println!("❌ FAIL");
                }
                if should_write_to_file && file_writer.is_some() {
                    let file = file_writer.as_mut().unwrap();
                    writeln!(file, "FAIL {}", test_name).unwrap();
                    writeln!(file, "  Expected:").unwrap();
                    writeln!(file, "{}", expected).unwrap();
                    writeln!(file, "  Actual:").unwrap();
                    writeln!(file, "{}", actual).unwrap();
                    writeln!(file, "").unwrap();
                }
            }
            TestOutcome::ValidationError(err) => {
                if should_print_result {
                    println!("🧹 VALIDATION ERROR");
                }
                if should_write_to_file && file_writer.is_some() {
                    let file = file_writer.as_mut().unwrap();
                    writeln!(file, "VALIDATION_ERROR {}", test_name).unwrap();
                    writeln!(file, "  Error: {}", err).unwrap();
                    writeln!(file, "").unwrap();
                }
            }
            TestOutcome::BootstrapParseError(err) => {
                if should_print_result {
                    println!("🔥 BOOTSTRAP ERROR");
                }
                if should_write_to_file && file_writer.is_some() {
                    let file = file_writer.as_mut().unwrap();
                    writeln!(file, "BOOTSTRAP_ERROR {}", test_name).unwrap();
                    writeln!(file, "  Error: {}", err).unwrap();
                    writeln!(file, "").unwrap();
                }
            }
            TestOutcome::ConversionError(err) => {
                if should_print_result {
                    println!("🔧 CONVERSION ERROR");
                }
                if should_write_to_file && file_writer.is_some() {
                    let file = file_writer.as_mut().unwrap();
                    writeln!(file, "CONVERSION_ERROR {}", test_name).unwrap();
                    writeln!(file, "  Error: {}", err).unwrap();
                    writeln!(file, "").unwrap();
                }
            }
            #[allow(deprecated)]
            TestOutcome::GrammarParseError(err) => {
                if should_print_result {
                    println!("🔥 GRAMMAR ERROR (LEGACY)");
                }
                if should_write_to_file && file_writer.is_some() {
                    let file = file_writer.as_mut().unwrap();
                    writeln!(file, "GRAMMAR_ERROR {}", test_name).unwrap();
                    writeln!(file, "  Error: {}", err).unwrap();
                    writeln!(file, "").unwrap();
                }
            }
            TestOutcome::InputParseError(err) => {
                if should_print_result {
                    println!("⚠️ PARSE ERROR");
                }
                if should_write_to_file && file_writer.is_some() {
                    let file = file_writer.as_mut().unwrap();
                    writeln!(file, "PARSE_ERROR {}", test_name).unwrap();
                    writeln!(file, "  Error: {}", err).unwrap();
                    writeln!(file, "").unwrap();
                }
            }
            TestOutcome::Panic(err) => {
                if should_print_result {
                    println!("💥 PANIC");
                }
                if should_write_to_file && file_writer.is_some() {
                    let file = file_writer.as_mut().unwrap();
                    writeln!(file, "PANIC {}", test_name).unwrap();
                    writeln!(file, "  Error: {}", err).unwrap();
                    writeln!(file, "").unwrap();
                }
            }
            TestOutcome::Skip(reason) => {
                if should_print_result {
                    println!("⏭️ SKIP");
                }
                if should_write_to_file && file_writer.is_some() {
                    let file = file_writer.as_mut().unwrap();
                    writeln!(file, "SKIP {}", test_name).unwrap();
                    writeln!(file, "  Reason: {}", reason).unwrap();
                    writeln!(file, "").unwrap();
                }
            }
            TestOutcome::Todo(reason) => {
                if should_print_result {
                    println!("🚧 TODO");
                }
                if should_write_to_file && file_writer.is_some() {
                    let file = file_writer.as_mut().unwrap();
                    writeln!(file, "TODO {}", test_name).unwrap();
                    writeln!(file, "  Reason: {}", reason).unwrap();
                    writeln!(file, "").unwrap();
                }
            }
        }
    }

    // Write summary to file if enabled
    if let Some(ref mut file) = file_writer {
        writeln!(file, "").unwrap();
        writeln!(file, "=== SUMMARY ===").unwrap();
        writeln!(file, "Total tests: {}", count).unwrap();
        for (category, count) in &stats {
            writeln!(file, "{}: {}", category, count).unwrap();
        }
    }

    // Print summary to stdout based on console level
    match console_level {
        DebugLevel::Off => {
            // Print nothing to stdout
        }
        DebugLevel::Basic => {
            println!("=== SUMMARY ===");
            println!("Total: {} | Pass: {} | Fail: {} | Bootstrap: {} | Conversion: {} | Parse: {}",
                count,
                stats.get("pass").unwrap_or(&0),
                stats.get("fail").unwrap_or(&0),
                stats.get("bootstrap_error").unwrap_or(&0),
                stats.get("conversion_error").unwrap_or(&0),
                stats.get("parse_error").unwrap_or(&0)
            );
            // Show detailed breakdown if any validation errors
            let validation = stats.get("validation_error").unwrap_or(&0);
            let legacy_grammar = stats.get("grammar_error").unwrap_or(&0);
            if *validation > 0 || *legacy_grammar > 0 {
                println!("Details: Validation: {} | Legacy Grammar: {}", validation, legacy_grammar);
            }
            if file_level != DebugLevel::Off {
                println!("Results written to: {}", output_file);
            }
        }
        DebugLevel::Detailed | DebugLevel::Trace => {
            println!("");
            println!("=== SUMMARY ===");
            println!("Total tests: {}", count);
            for (category, count) in &stats {
                println!("{}: {}", category, count);
            }
            if file_level != DebugLevel::Off {
                println!("Results written to: {}", output_file);
            }
        }
    }
}

fn run_single_test(test: testsuite_utils::TestCase) -> TestOutcome {
    // Parse grammar
    let grammar = match test.grammars.into_iter().next() {
        Some(g) => g,
        None => return TestOutcome::Skip("No grammar available".to_string()),
    };

    let target_grammar = match grammar {
        TestGrammar::Parsed(g) => g,
        TestGrammar::Unparsed(ixml) => {
            match Grammar::from_ixml_str_detailed(&ixml) {
                Ok(g) => g,
                Err(e) => {
                    use earleybird::grammar::GrammarConstructionError;
                    return match e {
                        GrammarConstructionError::ValidationError(msg) => TestOutcome::ValidationError(msg),
                        GrammarConstructionError::BootstrapParseError(err) => TestOutcome::BootstrapParseError(err.to_string()),
                        GrammarConstructionError::ConversionError(msg) => TestOutcome::ConversionError(msg),
                    };
                }
            }
        }
    };

    // Test each expected result
    for expected in test.expected {
        let outcome = match expected {
            AssertNotASentence => {
                // Try to parse - this should fail
                let mut parser = Parser::new(target_grammar.clone());
                match parse_with_trace_limit(&mut parser, &test.input) {
                    Ok(_) => TestOutcome::Fail {
                        expected: "parse failure".to_string(),
                        actual: "parse succeeded".to_string(),
                    },
                    Err(_) => TestOutcome::Pass,
                }
            }
            AssertDynamicError(expected_code) => {
                TestOutcome::Todo(format!("AssertDynamicError({}) not yet implemented", expected_code))
            }
            AssertXml(expected_xml) => {
                let version_mismatch = target_grammar.has_version_mismatch();
                let mut parser = Parser::new(target_grammar.clone());
                match parse_with_trace_limit(&mut parser, &test.input) {
                    Ok(tree) => {
                        let actual_xml = Parser::tree_to_test_format_with_version(&tree, version_mismatch);
                        if xml_canonicalize(&actual_xml) == xml_canonicalize(&expected_xml) {
                            TestOutcome::Pass
                        } else {
                            TestOutcome::Fail {
                                expected: xml_canonicalize(&expected_xml),
                                actual: xml_canonicalize(&actual_xml),
                            }
                        }
                    }
                    Err(e) => TestOutcome::InputParseError(e.to_string()),
                }
            }
        };

        // Return first non-pass result, or pass if all expectations pass
        if !matches!(outcome, TestOutcome::Pass) {
            return outcome;
        }
    }

    TestOutcome::Pass
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

        // Parse console and file levels
        let _console_level = match parse_level(&self.console) {
            Ok(level) => level,
            Err(e) => {
                eprintln!("Console level error: {}", e);
                std::process::exit(1);
            }
        };

        let _file_level = match parse_level(&self.file) {
            Ok(level) => level,
            Err(e) => {
                eprintln!("File level error: {}", e);
                std::process::exit(1);
            }
        };

        // Parse categories
        let _console_categories = match parse_categories(self.console_filter.as_ref()) {
            Ok(cats) => cats,
            Err(e) => {
                eprintln!("Console filter error: {}", e);
                std::process::exit(1);
            }
        };

        let _file_categories = match parse_categories(self.file_filter.as_ref()) {
            Ok(cats) => cats,
            Err(e) => {
                eprintln!("File filter error: {}", e);
                std::process::exit(1);
            }
        };

        let _result = run(self.suite, &self.console, self.console_filter.as_ref(), &self.file, self.file_filter.as_ref(), &self.output);
    }
}
