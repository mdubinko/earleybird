use crate::grammar::{Grammar, Rule};

/// temporary hack to get hands-on
/// TODO: Maybe use traits?
//trait ParserTestSuite {
//    fn get_grammar() -> Grammar;
//    fn get_inputs() -> Vec<&'static str>;
//    fn get_expected() -> Vec<&'static str>;
//}

// smoke tests
pub struct SmokeChars {}
pub struct SmokeSeq {}
pub struct SmokeAlt {}
pub struct SmokeNT {}
pub struct SmokeOpt {}
pub struct SmokeStar {}
pub struct SmokePlus {}
pub struct SmokeStarSep {}
pub struct SmokePlusSep {}

// test suites
pub struct SuiteWiki {}

impl SmokeSeq {
    pub fn get_grammar() -> Grammar {
        // doc = "a", "b".
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::build().ch('a').ch('b') );
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["ab"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc>ab</doc>"]
    }
}

impl SmokeAlt {
    pub fn get_grammar() -> Grammar {
        // doc = "a" | "b".
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::build().ch('a') );
        g.define("doc", Rule::build().ch('b') );
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["a", "b"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc>a</doc>", "<doc>b</doc>"]
    }
}

impl SmokeNT {
    pub fn get_grammar() -> Grammar {
        // doc = a, b.
        // a = "a" | "A".
        // b = "b" | "B".
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::build().nt("a").nt("b") );
        g.define("a", Rule::build().ch('a') );
        g.define("a", Rule::build().ch('A') );
        g.define("b", Rule::build().ch('b') );
        g.define("b", Rule::build().ch('B') );
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["ab", "Ab", "AB", "aB"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc><a>a</a><b>b</b></doc>", "<doc><a>A</a><b>b</b></doc>",
            "<doc><a>A</a><b>B</b></doc>", "<doc><a>a</a><b>B</b></doc>"]
    }
}

impl SmokeOpt {
    pub fn get_grammar() -> Grammar {
        // doc = "a"?.
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::build().opt(Rule::build().ch('a')));
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["", "a"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc></doc>", "<doc>a</doc>"]
    }
}

impl SmokeStar {
    pub fn get_grammar() -> Grammar {
        // doc = "a"*.
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::build().repeat0( Rule::build().ch('a')));
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["", "a", "aa", "aaa"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc></doc>", "<doc>a</doc>", "<doc>aa</doc>", "<doc>aaa</doc>"]
    }
}

impl SmokePlus {
    pub fn get_grammar() -> Grammar {
        // doc = "a"+.
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::build().repeat1( Rule::build().ch('a')));
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["a", "aa", "aaa"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc>a</doc>", "<doc>aa</doc>", "<doc>aaa</doc>"]
    }
}

impl SmokeStarSep {
    pub fn get_grammar() -> Grammar {
        // doc = "a"**" ".
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::build().repeat0_sep(
            Rule::build().ch('a'),
            Rule::build().ch(' '))
        );
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["", "a a", "a a a"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc></doc>", "<doc>a a</doc>", "<doc>a a a</doc>"]
    }
}

impl SmokePlusSep {
    pub fn get_grammar() -> Grammar {
        // doc = "a"++" ".
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::build().repeat1_sep(
            Rule::build().ch('a'),
            Rule::build().ch(' '))
        );
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["a a", "a a a"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc>a a</doc>", "<doc>a a a</doc>"]
    }
}


/// The example grammar from https://en.wikipedia.org/wiki/Earley_parser
impl SuiteWiki {
    pub fn get_grammar() -> Grammar {
        // doc = S.
        // S = S, "+", M | M.
        // M = M, "*", T | T.
        // T = ["1234"].
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::build().nt("S") );
        g.define("S", Rule::build().nt("S").ch('+').nt("M") );
        g.define("S", Rule::build().nt("M") );
        g.define("M", Rule::build().nt("M").ch('*').nt("T") );
        g.define("M", Rule::build().nt("T") );
        g.define("T", Rule::build().ch_in("1234") );
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["1", "1+2", "1+2*3", "0"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc><S><M><T>1</T></M></S></doc>",
             "<doc><S><S><M><T>1</T></M></S>+<M><T>2</T></M></S></doc>",
             "<doc><S><S><M><T>1</T></M></S>+<M><M><T>2</T></M>*<T>3</T></M></S></doc>",
             ""] // TODO: better failure cases
    }
}    
