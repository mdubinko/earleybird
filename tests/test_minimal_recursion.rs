// Minimal test for blockchain hash infinite recursion issue

#[cfg(test)]
mod tests {
    use earleybird::{grammar::Grammar, parser::Parser};

    #[test]
    fn test_trace_size_limit_mechanism() {
        // Test that trace size limits work correctly
        let grammar_str = r#"test: "a"."#;
        let grammar = Grammar::from_ixml_str(grammar_str).expect("Simple grammar should parse");

        // Create parser with very low trace limit
        let mut parser = Parser::new_with_trace_limit(grammar, 10);
        let result = parser.parse("a");

        // This simple case should succeed even with low limit
        assert!(result.is_ok(), "Simple grammar should work with low trace limit");
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
                let mut parser = Parser::new_with_trace_limit(grammar, 1000);  // Low but reasonable limit
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