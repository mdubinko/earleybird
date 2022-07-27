use crate::parser::Grammar;

/// temporary hack to get hands-on
/// TODO: Maybe use traits?
//trait ParserTestSuite {
//    fn get_grammar() -> Grammar;
//    fn get_inputs() -> Vec<&'static str>;
//    fn get_expected() -> Vec<&'static str>;
//}

pub struct Suite1 {}
pub struct Suite2 {}
pub struct Suite3 {}
pub struct Suite4 {}
pub struct SuiteWiki {}

impl Suite1 {
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

impl Suite2 {
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

impl Suite3 {
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

impl Suite4 {
    pub fn get_grammar() -> Grammar {
        // doc = "a"*, "b"+
        let mut g = Grammar::new();
        let a = g.add_litchar('a');
        let b = g.add_litchar('b');
        let aaaa = g.add_repeat0(a);
        let bbbb = g.add_repeat1(b);
        let seq = g.add_seq(vec![aaaa, bbbb]);
        g.add_rule("doc", seq);
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["b", "ab", "aab", "aabb"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc>b</doc>", "<doc>ab</doc>", "<doc>aab</doc>", "<doc>aabb</doc>"]
    }
}

impl SuiteWiki {
    pub fn get_grammar() -> Grammar {
        // doc = S
        // S = S "+" M | M
        // M = M "*" T | T
        // T = "1" | "2" | "3" | "4"
        let mut g = Grammar::new();
        let s_expr = g.add_nonterm("S");
        g.add_rule("P", s_expr);

        let m_expr = g.add_nonterm("M");
        let t_expr = g.add_nonterm("T");
        let plus = g.add_litchar('+');
        let star = g.add_litchar('*');
        let seq1 = g.add_seq(vec![s_expr, plus, m_expr]);
        let alt1 = g.add_oneof(vec![seq1, m_expr]);
        let seq2 = g.add_seq(vec![m_expr, star, t_expr]);
        let alt2 = g.add_oneof(vec![seq2, t_expr]);
        let digit = g.add_litcharoneof("1234");

        g.add_rule("doc", alt1);
        g.add_rule("M", alt2);
        g.add_rule("T", digit);
        g
    }
    pub fn get_inputs() -> Vec<&'static str> {
        vec!["1", "1+2", "1+2*3", "0"]
    }
    pub fn get_expected() -> Vec<&'static str> {
        vec!["<doc>ab</doc>", "<doc>Ab</doc>", "<doc>AB</doc>", "<doc>aB</doc>"]
    }
}    
