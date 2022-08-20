use earleybird::{grammar::{Grammar, Rule}, parser::Parser};
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
    g.define("doc", Rule::seq().repeat0( Rule::seq().ch('a')));

    let mut parser = Parser::new(g);
    parser.parse("aaaa");
    let arena = parser.unpack_parse_tree("doc");
    let s = Parser::tree_to_testfmt(&arena);
    println!("{}", s.len());
    println!("{s}");

    //dbg!(arena);


    argh::from_env::<Args>().subcommand.run();
    
}
