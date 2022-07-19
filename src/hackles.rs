
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

    let test = hackles_earley::get_simple1_grammer();
    let mut parser = hackles_earley::HacklesParser::new();
    parser.parse("ab", test, "doc");
    dbg!(&parser);

    argh::from_env::<Args>().subcommand.run();
    
}
