use std::ffi::OsString;
use std::fs;
use argh::FromArgs;
use earleybird::{grammar::Grammar, parser::Parser, debug::DebugLevel};
use earleybird::{debug_basic, debug_detailed};

#[derive(FromArgs)]
/// Parse an input file or string using an ixml grammar
#[argh(subcommand, name = "parse")]
pub struct Parse {
    /// ixml grammar file
    #[argh(option, short = 'g', long = "grammar-file")]
    grammar_file: Option<OsString>,

    /// ixml grammar string
    #[argh(option, long = "grammar-str")]
    grammar_str: Option<String>,

    /// input file
    #[argh(option, short = 'i', long = "input-file")]
    input_file: Option<OsString>,

    /// input string
    #[argh(option, long = "input-str")]
    input_str: Option<String>,

    /// output format
    #[argh(option, short = 'o', default = "default_output_fmt()")]
    out_format: String,

    /// verbosity level: off, basic, detailed, trace
    #[argh(option, short = 'v', default = "default_verbose()")]
    verbose: String,

    /// debug only at specific input position (for trace mode)
    #[argh(option, long = "debug-pos")]
    debug_pos: Option<usize>,

    /// write trace output to file instead of stdout
    #[argh(option, long = "trace-file")]
    trace_file: Option<String>,
}

impl Parse {
    pub fn run(self) {
        // Set up debug configuration
        let debug_level = match DebugLevel::from_str(&self.verbose) {
            Ok(level) => level,
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        };
        let debug_config = earleybird::debug::DebugConfig {
            level: debug_level,
            position_filter: self.debug_pos,
            failure_only: false,
            trace_file: self.trace_file.clone(),
        };
        earleybird::debug::set_debug_config(debug_config);

        debug_basic!("=== {} DEBUG MODE ===", self.verbose.to_uppercase());
        debug_basic!("");

        // 1. Get grammar content from either file or string
        let grammar_content = match (self.grammar_file, self.grammar_str) {
            (Some(file), None) => {
                debug_basic!("Grammar file: {:?}", file);
                match fs::read_to_string(&file) {
                    Ok(content) => content,
                    Err(e) => {
                        eprintln!("Error reading grammar file {:?}: {}", file, e);
                        std::process::exit(1);
                    }
                }
            }
            (None, Some(string)) => {
                debug_basic!("Grammar string: {}", string);
                string
            }
            (Some(_), Some(_)) => {
                eprintln!("Error: Cannot specify both --grammar-file and --grammar-str");
                std::process::exit(1);
            }
            (None, None) => {
                eprintln!("Error: Must specify either --grammar-file or --grammar-str");
                std::process::exit(1);
            }
        };

        // 2. Parse ixml grammar file and generate target grammar
        let target_grammar = match Grammar::from_ixml_str(&grammar_content) {
            Ok(grammar) => {
                debug_detailed!("✓ Grammar parsed successfully");
                debug_detailed!("  Rules: {}", grammar.get_rule_count());
                if let Some(root) = grammar.get_root_definition_name() {
                    debug_detailed!("  Root rule: {}", root);
                }
                debug_detailed!("");
                grammar
            }
            Err(e) => {
                eprintln!("Error parsing ixml grammar: {}", e);
                debug_basic!("Grammar content: {}", grammar_content);
                std::process::exit(1);
            }
        };

        // 3. Get input content from either file or string
        let input_content = match (self.input_file, self.input_str) {
            (Some(file), None) => {
                debug_basic!("Input file: {:?}", file);
                match fs::read_to_string(&file) {
                    Ok(content) => content,
                    Err(e) => {
                        eprintln!("Error reading input file {:?}: {}", file, e);
                        std::process::exit(1);
                    }
                }
            }
            (None, Some(string)) => {
                debug_basic!("Input string: {}", string);
                string
            }
            (Some(_), Some(_)) => {
                eprintln!("Error: Cannot specify both --input-file and --input-str");
                std::process::exit(1);
            }
            (None, None) => {
                eprintln!("Error: Must specify either --input-file or --input-str");
                std::process::exit(1);
            }
        };

        // 4. Parse input file against target grammar
        let mut parser = Parser::new(target_grammar);
        let parse_tree = match parser.parse(&input_content) {
            Ok(tree) => {
                debug_detailed!("✓ Input parsed successfully");
                tree
            }
            Err(e) => {
                eprintln!("Error parsing input file: {}", e);
                debug_basic!("Input content: {}", input_content);
                earleybird::debug::debug_parse_failure(&input_content, 0, &e.to_string());
                std::process::exit(1);
            }
        };

        // 5. Format and output results
        match self.out_format.as_str() {
            "XML" => {
                let xml_output = Parser::tree_to_test_format(&parse_tree);
                println!("{}", xml_output);
            }
            _ => {
                eprintln!("Unsupported output format: {}", self.out_format);
                std::process::exit(1);
            }
        }
    }
}

fn default_output_fmt() -> String {
    "XML".to_string()
}

fn default_verbose() -> String {
    "off".to_string()
}