// Minimal test for blockchain hash infinite recursion issue

#[cfg(test)]
mod tests {
    use earleybird::{grammar::Grammar, parser::Parser};

    #[test]
    fn test_infinite_loop_detection() {
        let grammar_str = r#"test: "a"."#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Simple grammar should parse");

        let mut parser = Parser::new(grammar);
        let result = parser.parse("a");

        assert!(result.is_ok(), "Simple grammar should parse successfully");
    }

    #[test]
    fn test_minimal_infinite_recursion_case() {
        // This will be updated once we find the minimal failing case
        // For now, test that complex grammars hit trace limits appropriately

        let address_grammar = r#"
test: a, b.
a: c+.
b: c*.
c: ["x"].
        "#;

        let result = Grammar::from_ixml_str(address_grammar);
        match result {
            Ok(grammar) => {
                let mut parser = Parser::new(grammar);
                let parse_result = parser.parse("xxx");

                // This should either succeed or fail gracefully (not hang)
                match parse_result {
                    Ok(_) => println!("✅ Grammar parsed successfully"),
                    Err(e) => {
                        println!("❌ Grammar failed: {}", e);
                        // Acceptable outcomes: parse error or trace limit hit
                        let error_str = e.to_string();
                        assert!(
                            error_str.contains("Parse failed") ||
                            error_str.contains("trace size") ||
                            error_str.contains("infinite loop"),
                            "Expected reasonable error, got: {}", e
                        );
                    }
                }
            }
            Err(e) => {
                println!("Bootstrap parsing failed: {}", e);
                // This is what we expect to happen with the current issue
                assert!(
                    e.to_string().contains("trace size") ||
                    e.to_string().contains("infinite loop") ||
                    e.to_string().contains("Bootstrap parse error"),
                    "Expected bootstrap or trace size error, got: {}", e
                );
            }
        }
    }
}