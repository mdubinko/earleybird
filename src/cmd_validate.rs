use std::ffi::OsString;
use std::fs;
use argh::FromArgs;
use earleybird::grammar::Grammar;

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
}

impl Validate {
    pub fn run(self) {
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
                println!("✓ Grammar validation successful");
                println!("  Rules: {}", grammar.get_rule_count());
                if let Some(root) = grammar.get_root_definition_name() {
                    println!("  Root rule: {}", root);
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