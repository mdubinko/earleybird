#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test_1() {
        let grammar = hackles_earley::get_simple1_grammer();
        let mut parser = hackles_earley::Parser::new(grammar);
        let trace = parser.parse("ab", "doc");
        assert_eq!(trace.len(), 7);
        //assert_eq!(trace.keys(), ["doc[0,0]", "doc[0,1]"]);
        //assert_eq!(trace["doc"].0, "doc");
        //assert_eq!(trace["doc"].1, 0);
    }

    #[test]
    fn smoke_test_2() {
        let grammar = hackles_earley::get_simple2_grammer();
        let mut parser = hackles_earley::Parser::new(grammar);
        let trace = parser.parse("b", "doc");
        assert_eq!(trace.len(), 3);
    }

    #[test]
    fn smoke_test_3() {
        let grammar = hackles_earley::get_simple3_grammer();
        let mut parser = hackles_earley::Parser::new(grammar);
        let trace = parser.parse("Ab", "doc");
        assert_eq!(trace.len(), 10);
    }
}