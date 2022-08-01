use crate::parser::Grammar;

/// temporary hack to get hands-on
/// TODO: Maybe use traits?
//trait ParserTestSuite {
//    fn get_grammar() -> Grammar;
//    fn get_inputs() -> Vec<&'static str>;
//    fn get_expected() -> Vec<&'static str>;
//}

// smoke tests
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
        // doc = "a", "b"
        let mut g = Grammar::new();
        let a = g.add_litchar('a');
        let b = g.add_litchar('b');
        let seq = g.add_seq(vec![a,b]);
        g.add_rule("doc", seq);
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
        // doc = "a" | "b"
        let mut g = Grammar::new();
        let a = g.add_litchar('a');
        let b = g.add_litchar('b');
        let choice = g.add_oneof(vec![a,b]);
        g.add_rule("doc", choice);
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
        // doc = a, b
        // a = "a" | "A"
        // b = "b" | "B"
        let mut g = Grammar::new();
        let aref = g.add_nonterm("a");
        let bref = g.add_nonterm("b");
        let a = g.add_litchar('a');
        let a_ = g.add_litchar('A');
        let a_choice = g.add_oneof(vec![a,a_]);
        let b = g.add_litchar('b');
        let b_ = g.add_litchar('B');
        let b_choice = g.add_oneof(vec![b, b_]);
        let seq = g.add_seq(vec![aref, bref]);
        g.add_rule("doc", seq);
        g.add_rule("a", a_choice);
        g.add_rule("b", b_choice);
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
        // doc = "a"?
        let mut g = Grammar::new();
        let a = g.add_litchar('a');
        let opt = g.add_optional(a);
        g.add_rule("doc", opt);
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
        // doc = "a"*
        let mut g = Grammar::new();
        let a = g.add_litchar('a');
        let a_star = g.add_repeat0(a);
        g.add_rule("doc", a_star);
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["", "a", "aa", "aaa"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc></doc>", "<doc>a</doc>", "<doc>aa</doc>", "<doc>aaa</doc>"]
    }
}

impl SuiteWiki {
    pub fn get_grammar() -> Grammar {
        // doc = S
        // S = S "+" M | M
        // M = M "*" T | T
        // T = "1" | "2" | "3" | "4"
        let mut g = Grammar::new();
        let s_nt = g.add_nonterm("S");
        g.add_rule("doc", s_nt);

        let m_nt = g.add_nonterm("M");
        let t_nt = g.add_nonterm("T");
        let plus = g.add_litchar('+');
        let star = g.add_litchar('*');
        let s_plus_m = g.add_seq(vec![s_nt, plus, m_nt]);
        let s_plus_m_or_m = g.add_oneof(vec![s_plus_m, m_nt]);
        let m_star_t = g.add_seq(vec![m_nt, star, t_nt]);
        let m_star_t_or_t = g.add_oneof(vec![m_star_t, t_nt]);
        let digit = g.add_litcharoneof("1234");

        g.add_rule("S", s_plus_m_or_m);
        g.add_rule("M", m_star_t_or_t);
        g.add_rule("T", digit);
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
