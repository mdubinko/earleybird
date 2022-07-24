
pub mod parse;
use argh::FromArgs;


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

    let grammar = hackles_earley::get_simple2_grammer();
    let mut parser = hackles_earley::Parser::new(grammar);
    parser.parse("b", "doc");
    //dbg!(&parser);
    let tree = parser.unpack_parse_tree("doc");
    println!("{}", &tree);

    argh::from_env::<Args>().subcommand.run();
    
}
