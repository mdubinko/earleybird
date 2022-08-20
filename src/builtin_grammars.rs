use crate::grammar::{Grammar, Rule, Lit};

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

impl SmokeChars {
    pub fn get_grammar() -> Grammar {
        // exercise all the different kinds of character matching
        // doc = ["0"-"9"], [Zs], ~["0"-"9"; "a"-"f"; "A"-"F"], ["abcdef"].
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::seq()
            .ch_range('0', '9')
            .ch_unicode("Zs")
            .lit( Lit::union().exclude().ch_range('0', '9').ch_range('a', 'f').ch_range('A', 'F') )
            .ch_in("abcdef") );
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["0 Ga", "9\u{00a0}!f"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc>0 Ga</doc>", "<doc>9\u{00a0}!f</doc>"]
    }
}

impl SmokeSeq {
    pub fn get_grammar() -> Grammar {
        // doc = "a", "b".
        let mut g = Grammar::new("doc");
        g.define("doc", Rule::seq().ch('a').ch('b') );
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
        g.define("doc", Rule::seq().ch('a') );
        g.define("doc", Rule::seq().ch('b') );
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
        g.define("doc", Rule::seq().nt("a").nt("b") );
        g.define("a", Rule::seq().ch('a') );
        g.define("a", Rule::seq().ch('A') );
        g.define("b", Rule::seq().ch('b') );
        g.define("b", Rule::seq().ch('B') );
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
        g.define("doc", Rule::seq().opt(Rule::seq().ch('a')));
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
        g.define("doc", Rule::seq().repeat0( Rule::seq().ch('a')));
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
        g.define("doc", Rule::seq().repeat1( Rule::seq().ch('a')));
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
        g.define("doc", Rule::seq().repeat0_sep(
            Rule::seq().ch('a'),
            Rule::seq().ch(' '))
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
        g.define("doc", Rule::seq().repeat1_sep(
            Rule::seq().ch('a'),
            Rule::seq().ch(' '))
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
        g.define("doc", Rule::seq().nt("S") );
        g.define("S", Rule::seq().nt("S").ch('+').nt("M") );
        g.define("S", Rule::seq().nt("M") );
        g.define("M", Rule::seq().nt("M").ch('*').nt("T") );
        g.define("M", Rule::seq().nt("T") );
        g.define("T", Rule::seq().ch_in("1234") );
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
