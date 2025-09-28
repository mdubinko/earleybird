use argh::FromArgs;
mod cmd_parse;
mod cmd_suite;
mod cmd_validate;

#[derive(FromArgs)]
/// An experimental ixml implementation in Rust
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
}

impl Subcommand {
    fn run(self) {
        match self {
            Subcommand::Parse(cmd) => cmd.run(),
            Subcommand::Suite(cmd) => cmd.run(),
            Subcommand::Validate(cmd) => cmd.run(),
        }
    }
}

fn main() {
    env_logger::init();

    argh::from_env::<Args>().subcommand.run();
}

