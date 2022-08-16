use earleybird::grammar::{Grammar, Rule};

use argh::FromArgs;
mod parse_cmd;

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
    Parse(parse_cmd::Parse),
}

impl Subcommand {
    fn run(self) {
        match self {
            Subcommand::Parse(p) => p.run(),
        }
    }
}

fn main() {

    let mut g = Grammar::new("doc");
    //g.define("doc", Rule::build().repeat0( Rule::build().lit('a')));
    g.define("doc", Rule::build().repeat0( Rule::build().ch('a')));
    println!("{g}");

    argh::from_env::<Args>().subcommand.run();
    
}
