
use crate::{grammar::{Grammar, Rule, Lit, Mark, TMark}, testsuite_utils::{TestCase, TestGrammar, TestResult}};

pub trait ParserTestSet {
    fn get_name(&self) -> &'static str;
    fn get_grammar(&self) -> Grammar;
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)>;
}

#[derive(Debug)]
pub struct SmokeTests {
    tests: Vec<TestCase>
}

impl SmokeTests {
    fn default() -> Self {
        let v = vec![

        ];
        Self { tests: v }
    }

    /// add one or more test cases against a provided grammar
    fn add<T: ParserTestSet>(&mut self, testset: &T) {
        let name = testset.get_name();
        let ins_outs = testset.get_inputs_expected();
        for (input, expected) in ins_outs {
            self.tests.push(
                TestCase {
                    name: testset.get_name().to_owned(),
                    grammar: TestGrammar::Parsed(testset.get_grammar()),
                    input: input.to_string(),
                    expected: vec![TestResult::AssertXml(expected.to_string())],
                }
            )
       }
   }
}

impl IntoIterator for SmokeTests {
    type Item = TestCase;
    type IntoIter = std::vec::IntoIter<TestCase>;

    fn into_iter(self) -> Self::IntoIter {
        self.tests.into_iter()
    }
}

pub fn all_builtin_tests() -> SmokeTests {
    let mut tests = SmokeTests::default();
    tests.add(&SmokeChars {});
    tests.add(&SmokeSeq {});
    tests.add(&SmokeAlt {});
    tests.add(&SmokeNT {});
    tests.add(&SmokeOpt {});
    tests.add(&SmokeStar {});
    tests.add(&SmokePlus{});
    tests.add(&SmokeStarSep {});
    tests.add(&SmokePlusSep {});
    tests.add(&SmokeElem {});
    tests.add(&SmokeAttr {});
    tests.add(&SmokeMute {});
    tests.add(&SmokeWiki {});
    tests
}

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
/// `SmokeElem` is intended as a "control" case, identical with `SmokeAttr` other than the @ Marks
pub struct SmokeElem {}
pub struct SmokeAttr {}
pub struct SmokeMute {}

// test suites
pub struct SmokeWiki {}

impl ParserTestSet for SmokeChars {
    fn get_name(&self) -> &'static str { "SmokeChars" }
    fn get_grammar(&self) -> Grammar {
        // exercise all the different kinds of character matching
        // doc = ["0"-"9"], [Zs], ~["0"-"9"; "a"-"f"; "A"-"F"], ["abcdef"].
        let mut g = Grammar::new();
        g.define("doc", Rule::seq()
            .ch_range('0', '9')
            .ch_unicode("Zs")
            .lit( Lit::union().exclude().ch_range('0', '9').ch_range('a', 'f').ch_range('A', 'F') )
            .ch_in("abcdef") );
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("0 Ga", "<doc>0 Ga</doc>"),
            ("9\u{00a0}!f", "<doc>9\u{00a0}!f</doc>")
            ]
    }
}

impl ParserTestSet for SmokeSeq {
    fn get_name(&self) -> &'static str { "SmokeSeq" }
    fn get_grammar(&self) -> Grammar {
        // doc = "a", "b".
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().ch('a').ch('b') );
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("ab", "<doc>ab</doc>"),
            ]
    }
}

impl ParserTestSet for SmokeAlt {
    fn get_name(&self) -> &'static str { "SmokeAlt" }
    fn get_grammar(&self) -> Grammar {
        // doc = "a" | "b".
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().ch('a') );
        g.define("doc", Rule::seq().ch('b') );
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("a", "<doc>a</doc>"),
            ("b", "<doc>b</doc>")
            ]
    }
}

impl ParserTestSet for SmokeNT {
    fn get_name(&self) -> &'static str { "SmokeNT" }
    fn get_grammar(&self) -> Grammar {
        // doc = a, b.
        // a = "a" | "A".
        // b = "b" | "B".
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().nt("a").nt("b") );
        g.define("a", Rule::seq().ch('a') );
        g.define("a", Rule::seq().ch('A') );
        g.define("b", Rule::seq().ch('b') );
        g.define("b", Rule::seq().ch('B') );
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("ab", "<doc><a>a</a><b>b</b></doc>"),
            ("Ab", "<doc><a>A</a><b>b</b></doc>"),
            ("AB", "<doc><a>A</a><b>B</b></doc>"),
            ("aB", "<doc><a>a</a><b>B</b></doc>"),
            ]
    }
}

impl ParserTestSet for SmokeOpt {
    fn get_name(&self) -> &'static str { "SmokeOpt" }
    fn get_grammar(&self) -> Grammar {
        // doc = "a"?.
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().opt(Rule::seq().ch('a')));
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("", "<doc></doc>"),
            ("a", "<doc>a</doc>"),
            ]
    }
}

impl ParserTestSet for SmokeStar {
    fn get_name(&self) -> &'static str { "SmokeStar" }
    fn get_grammar(&self) -> Grammar {
        // doc = "a"*.
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().repeat0( Rule::seq().ch('a')));
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("", "<doc></doc>"),
            ("a", "<doc>a</doc>"),
            ("aa", "<doc>aa</doc>"),
            ("aaa", "<doc>aaa</doc>"),
            ]
    }
}

impl ParserTestSet for SmokePlus {
    fn get_name(&self) -> &'static str { "SmokePlus" }
    fn get_grammar(&self) -> Grammar {
        // doc = "a"+.
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().repeat1( Rule::seq().ch('a')));
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("a", "<doc>a</doc>"),
            ("aa", "<doc>aa</doc>"),
            ("aaa", "<doc>aaa</doc>"),
            ]
    }
}

impl ParserTestSet for SmokeStarSep {
    fn get_name(&self) -> &'static str { "SmokeStarSep" }
    fn get_grammar(&self) -> Grammar {
        // doc = "a"**" ".
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().repeat0_sep(
            Rule::seq().ch('a'),
            Rule::seq().ch(' '))
        );
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("", "<doc></doc>"),
            ("a a", "<doc>a a</doc>"),
            ("a a a", "<doc>a a a</doc>"),
            ]
    }
}

impl ParserTestSet for SmokePlusSep {
    fn get_name(&self) -> &'static str { "SmokePlusSep" }
    fn get_grammar(&self) -> Grammar {
        // doc = "a"++" ".
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().repeat1_sep(
            Rule::seq().ch('a'),
            Rule::seq().ch(' '))
        );
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("a a", "<doc>a a</doc>"),
            ("a a a", "<doc>a a a</doc>")
            ]
    }
}

impl ParserTestSet for SmokeElem {
    fn get_name(&self) -> &'static str { "SmokeElem" }
    fn get_grammar(&self) -> Grammar {
        // doc = name, ":", value.
        // name = ["a"-"z"]+.
        // value = ["a"-"z"]+.
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().nt("name").ch(':').nt("value"));
        g.define("name", Rule::seq().repeat1( Rule::seq().ch_range('a', 'z')));
        g.define("value", Rule::seq().repeat1( Rule::seq().ch_range('a', 'z')));
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("a:b", "<doc><name>a</name>:<value>b</value></doc>"),
            ("abc:def", "<doc><name>abc</name>:<value>def</value></doc>")
            ]
    }
}

impl ParserTestSet for SmokeAttr {
    fn get_name(&self) -> &'static str { "SmokeAttr" }
    fn get_grammar(&self) -> Grammar {
        // n.b. identical to SmokeElem other than @ Mark on the name definition
        // doc = name, ":", value.
        // @name = ["a"-"z"]+.
        // value = ["a"-"z"]+.
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().nt("name").ch(':').nt("value"));
        g.mark_define(Mark::Attr, "name", Rule::seq().repeat1( Rule::seq().ch_range('a', 'z')));
        g.define("value", Rule::seq().repeat1( Rule::seq().ch_range('a', 'z')));
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("a:b", r#"<doc name="a">:<value>b</value></doc>"#),
            ("abc:def", r#"<doc name="abc">:<value>def</value></doc>"#),
            ]
    }
}

impl ParserTestSet for SmokeMute {
    fn get_name(&self) -> &'static str { "SmokeMute" }
    fn get_grammar(&self) -> Grammar {
        // Several different ways to mute...
        // doc = a, -":", -b, c.
        // -a = ["a"-"z"]+.
        // b = ["a"-"m"]+.
        // c = ["n"-"z"]+.
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().nt("a").mark_ch(':', TMark::Mute).mark_nt("b", Mark::Mute).nt("c"));
        g.mark_define(Mark::Mute, "a", Rule::seq().repeat1( Rule::seq().ch_range('a', 'z')));
        g.define("b", Rule::seq().repeat1( Rule::seq().ch_range('a', 'm')));
        g.define("c", Rule::seq().repeat1( Rule::seq().ch_range('n', 'z')));
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("a:bz", "<doc>ab<c>z</c></doc>"),
            ("abc:defxyz", "<doc>abcdef<c>xyz</c></doc>")
            ]
    }
}


/// The example grammar from https://en.wikipedia.org/wiki/Earley_parser
impl ParserTestSet for SmokeWiki {
    fn get_name(&self) -> &'static str { "SmokeWiki" }
    fn get_grammar(&self) -> Grammar {
        // doc = S.
        // S = S, "+", M | M.
        // M = M, "*", T | T.
        // T = ["1234"].
        let mut g = Grammar::new();
        g.define("doc", Rule::seq().nt("S") );
        g.define("S", Rule::seq().nt("S").ch('+').nt("M") );
        g.define("S", Rule::seq().nt("M") );
        g.define("M", Rule::seq().nt("M").ch('*').nt("T") );
        g.define("M", Rule::seq().nt("T") );
        g.define("T", Rule::seq().ch_in("1234") );
        g
    }
    fn get_inputs_expected(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("1", "<doc><S><M><T>1</T></M></S></doc>"),
            ("1+2", "<doc><S><S><M><T>1</T></M></S>+<M><T>2</T></M></S></doc>"),
            ("1+2*3", "<doc><S><S><M><T>1</T></M></S>+<M><M><T>2</T></M>*<T>3</T></M></S></doc>"),
            ("0", ""), // TODO: better failure cases
            ]
    }
}    
