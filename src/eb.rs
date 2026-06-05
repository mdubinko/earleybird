use argh::FromArgs;
mod cmd_bench;
mod cmd_parse;
mod cmd_suite;
mod cmd_validate;

#[derive(FromArgs)]
#[argh(description = r#"EarleyBird: iXML parser with granular debug control.

Use --console and/or --file with DEBUG|INFO|SUMMARY|WARNING|ERROR|ALL|NONE levels.
Filtering: Use --console-filter or --file-filter with a comma separated list of:
   BOOTSTRAP
   COMPLETE
   DEDUP
   GRAMMAR
   OUTPUT
   PREDICT
   QUEUE
   SCANNER

Examples:
'eb parse --grammar-str "test: a." --input-str "a" -f XML --console SUMMARY -o debug.log'
'eb validate -g file.ixml --console DEBUG --console-filter BOOTSTRAP -o validation.log'

Run 'eb COMMAND --help' for detailed options."#)]
struct Args {
    #[argh(subcommand)]
    subcommand: Subcommand,
}

#[derive(FromArgs)]
/// ixml parser and validator
#[argh(subcommand)]
enum Subcommand {
    Parse(cmd_parse::Parse),
    Suite(cmd_suite::RunSuite),
    Validate(cmd_validate::Validate),
    Bench(cmd_bench::Bench),
}

impl Subcommand {
    fn run(self) {
        match self {
            Subcommand::Parse(cmd) => cmd.run(),
            Subcommand::Suite(cmd) => cmd.run(),
            Subcommand::Validate(cmd) => cmd.run(),
            Subcommand::Bench(cmd) => cmd.run(),
        }
    }
}

fn main() {
    env_logger::init();

    argh::from_env::<Args>().subcommand.run();
}
