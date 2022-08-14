use argh::FromArgs;

#[derive(FromArgs)]
/// Read an ixml file and parse another file with that grammar
#[argh(subcommand, name = "parse")]
pub struct Parse {}

impl Parse {
    pub fn run(self) {
        todo!()
    }
}