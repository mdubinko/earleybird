use hackles_earley;

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
            Subcommand::Parse(x) => x.run(),
        }
    }
}

fn main() {

    let test = hackles_earley::test();
    println!("{}", test);

    argh::from_env::<Args>().subcommand.run();
    
}
