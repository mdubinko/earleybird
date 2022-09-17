use std::ffi::OsString;
use argh::FromArgs;

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
}

impl Parse {
    pub fn run(self) {
        todo!()
        // 1. Read ixml grammar file

        // 1.5 Validate grammar

        // 2. Parse ixml grammar file

        // 3. Generate target grammar

        // 4. Read input file

        // 5. Parse input file against target grammar

        // 6. Format output
    }
}

fn default_output_fmt() -> String {
    "XML".to_string()
}