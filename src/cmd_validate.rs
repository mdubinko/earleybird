use std::ffi::OsString;
use std::fs;
use std::collections::HashSet;
use argh::FromArgs;
use earleybird::{grammar::Grammar, debug::DebugLevel};

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

#[derive(FromArgs)]
/// Validate an ixml grammar against the bootstrap grammar
#[argh(subcommand, name = "validate")]
pub struct Validate {
    /// ixml grammar file to validate
    #[argh(option, short = 'g', long = "grammar-file")]
    grammar_file: Option<OsString>,

    /// ixml grammar string to validate
    #[argh(option, long = "grammar-str")]
    grammar_str: Option<String>,

    /// console output level: DEBUG|INFO|SUMMARY|WARNING|ERROR|ALL|NONE|OFF
    #[argh(option, long = "console", default = "default_console_level()")]
    console: String,

    /// console output categories: BOOTSTRAP,QUEUE,SCANNER,OUTPUT,PREDICT,COMPLETE,DEDUP,GRAMMAR
    #[argh(option, long = "console-filter")]
    console_filter: Option<String>,

    /// file output level: DEBUG|INFO|SUMMARY|WARNING|ERROR|ALL|NONE|OFF|FAILURES
    #[argh(option, long = "file", default = "default_file_level()")]
    file: String,

    /// file output categories: BOOTSTRAP,QUEUE,SCANNER,OUTPUT,PREDICT,COMPLETE,DEDUP,GRAMMAR
    #[argh(option, long = "file-filter")]
    file_filter: Option<String>,

    /// output filename for debug/trace information
    #[argh(option, short = 'o', long = "output", default = "default_output_file()")]
    output: String,
}

fn default_console_level() -> String {
    "SUMMARY".to_string()
}

fn default_file_level() -> String {
    "NONE".to_string()
}

fn default_output_file() -> String {
    "validation-output.txt".to_string()
}

impl Validate {
    pub fn run(self) {
        // Parse console and file levels
        let console_level = match parse_level(&self.console) {
            Ok(level) => level,
            Err(e) => {
                eprintln!("Console level error: {}", e);
                std::process::exit(1);
            }
        };

        let file_level = match parse_level(&self.file) {
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

        // Set up debug configuration with category filtering
        let debug_config = earleybird::debug::DebugConfig {
            level: console_level,
            position_filter: None,
            failure_only: false,
            trace_file: if file_level != DebugLevel::Off { Some(self.output.clone()) } else { None },
            enabled_categories: _console_categories.clone(),
        };
        earleybird::debug::set_debug_config(debug_config);
        // Get grammar content from either file or string
        let grammar_content = match (self.grammar_file, self.grammar_str) {
            (Some(file), None) => {
                match fs::read_to_string(&file) {
                    Ok(content) => content,
                    Err(e) => {
                        eprintln!("Error reading grammar file {:?}: {}", file, e);
                        std::process::exit(1);
                    }
                }
            }
            (None, Some(string)) => string,
            (Some(_), Some(_)) => {
                eprintln!("Error: Cannot specify both --grammar-file and --grammar-str");
                std::process::exit(1);
            }
            (None, None) => {
                eprintln!("Error: Must specify either --grammar-file or --grammar-str");
                std::process::exit(1);
            }
        };

        // Validate by attempting to parse the grammar against the bootstrap grammar
        match Grammar::from_ixml_str(&grammar_content) {
            Ok(grammar) => {
                // Validation successful
                if console_level != DebugLevel::Off {
                    println!("✓ Grammar validation successful");
                    println!("  Rules: {}", grammar.get_rule_count());
                    if let Some(root) = grammar.get_root_definition_name() {
                        println!("  Root rule: {}", root);
                    }
                }
                std::process::exit(0);
            }
            Err(e) => {
                // Validation failed
                eprintln!("Grammar validation failed: {}", e);
                std::process::exit(1);
            }
        }
    }
}