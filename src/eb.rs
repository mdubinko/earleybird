use argh::FromArgs;
mod cmd_parse;
mod cmd_suite;

#[derive(FromArgs)]
/// An experimental ixml implementation in Rust
struct Args {
    #[argh(subcommand)]
    subcommand: Subcommand,
}

#[derive(FromArgs)]
/// parse something
#[argh(subcommand)]
enum Subcommand {
    Parse(cmd_parse::Parse),
    Suite(cmd_suite::RunSuite),
}

impl Subcommand {
    fn run(self) {
        match self {
            Subcommand::Parse(cmd) => cmd.run(),
            Subcommand::Suite(cmd) => cmd.run(),
        }
    }
}

fn main() {


    argh::from_env::<Args>().subcommand.run();
}

