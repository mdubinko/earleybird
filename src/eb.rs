use earleybird::parser::Parser;
use earleybird::builtin_grammars;

use argh::FromArgs;
mod parse;

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
    Parse(parse::Parse),
}

impl Subcommand {
    fn run(self) {
        match self {
            Subcommand::Parse(p) => p.run(),
        }
    }
}

fn main() {

    let grammar = builtin_grammars::SmokeStar::get_grammar();
    let mut parser = Parser::new(grammar);
    let inputs = builtin_grammars::SmokeStar::get_inputs();
    parser.parse(inputs[3], "doc");
    dbg!(&parser);
    let tree = parser.unpack_parse_tree("doc");
    println!("{}", &tree);

    argh::from_env::<Args>().subcommand.run();
    
}
