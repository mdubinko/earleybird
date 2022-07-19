#[cfg(test)]
mod tests {
    #[test]
    fn smoke_test_1() {
        let test = hackles_earley::get_simple1_grammer();
        let mut parser = hackles_earley::HacklesParser::new();
        let trace = parser.parse("ab", test, "doc");
        dbg!(&trace);
        assert_eq!(4, 4);
    }
}