use std::ffi::OsString;
use std::fs;
use argh::FromArgs;
use earleybird::{grammar::Grammar, parser::Parser, debug::DebugLevel};
use earleybird::{debug_basic, debug_detailed};

#[derive(FromArgs)]
/// Read an ixml file and parse another file with that grammar
#[argh(subcommand, name = "parse")]
pub struct Parse {
    /// ixml grammar file
    #[argh(option, short = 'g')]
    grammar: OsString,

    /// input document
    #[argh(option, short = 'i')]
    input: OsString,

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
        debug_basic!("Grammar file: {:?}", self.grammar);
        debug_basic!("Input file: {:?}", self.input);
        debug_basic!("");

        // 1. Read ixml grammar file
        let grammar_content = match fs::read_to_string(&self.grammar) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Error reading grammar file {:?}: {}", self.grammar, e);
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

        // 3. Read input file
        let input_content = match fs::read_to_string(&self.input) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("Error reading input file {:?}: {}", self.input, e);
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