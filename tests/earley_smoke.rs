use earleybird::grammar::Mark;
use earleybird::parser::{Parser, ParseError};
use earleybird::builtin_grammars::*;
use smol_str::SmolStr;

#[test]
fn run_smoke_chars() -> Result<(), ParseError> {
    let inputs = SmokeChars::get_inputs();
    let expecteds = SmokeChars::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeChars::get_grammar();
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_seq() -> Result<(), ParseError> {
    let inputs = SmokeSeq::get_inputs();
    let expecteds = SmokeSeq::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeSeq::get_grammar();
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_alt() -> Result<(), ParseError> {
    let inputs = SmokeAlt::get_inputs();
    let expecteds = SmokeAlt::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeAlt::get_grammar();
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_nt() -> Result<(), ParseError> {
    let inputs = SmokeNT::get_inputs();
    let expecteds = SmokeNT::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeNT::get_grammar();
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_opt() -> Result<(), ParseError> {
    let inputs = SmokeOpt::get_inputs();
    let expecteds = SmokeOpt::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeOpt::get_grammar();
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_star() -> Result<(), ParseError> {
    let inputs = SmokeStar::get_inputs();
    let expecteds = SmokeStar::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeStar::get_grammar();
        println!("  Against grammar:\n{}", g);
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_plus() -> Result<(), ParseError> {
    let inputs = SmokePlus::get_inputs();
    let expecteds = SmokePlus::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokePlus::get_grammar();
        println!("  Against grammar:\n{}", g);
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_star_sep() -> Result<(), ParseError> {
    let inputs = SmokeStarSep::get_inputs();
    let expecteds = SmokeStarSep::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeStarSep::get_grammar();
        println!("  Against grammar:\n{}", g);
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_plus_sep() -> Result<(), ParseError> {
    let inputs = SmokePlusSep::get_inputs();
    let expecteds = SmokePlusSep::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokePlusSep::get_grammar();
        println!("  Against grammar:\n{}", g);
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_elem() -> Result<(), ParseError> {
    let inputs = SmokeElem::get_inputs();
    let expecteds = SmokeElem::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeElem::get_grammar();
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_attr() -> Result<(), ParseError> {
    let inputs = SmokeAttr::get_inputs();
    let expecteds = SmokeAttr::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeAttr::get_grammar();
        
        // make sure grammar definition has Mark::Attr
        let def = g.get_definition("name").clone();
        assert_eq!(Mark::Attr, def.mark());

        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;

        // make sure trace accounts for @name
        let trace = parser.test_inspect_trace(Some(SmolStr::new("name")));
        for task in trace {
            assert_eq!(Mark::Attr, task.mark())
        }

        // make sure indextree accounts for @name
        assert_ne!(0, arena.count());
        for node in arena.iter() {
            match node.get() {
                earleybird::parser::Content::Attribute(name, value) => {
                    println!("Content::Attribute {name}=\"{value}\"");
                    if name=="name" {
                        assert!(true, "@ name correctly matched as Attribute");
                        assert!(value.len() > 0);
                    }
                }
                earleybird::parser::Content::Element(name) => {
                    println!("Content::Element {name}");
                    if name=="name" {
                        assert!(false, "@name incorrectly matched as Element, not Attribute");
                    }
                }
                _ => {}
            }
        }

        //finally test end-state output
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_smoke_mute() -> Result<(), ParseError> {
    let inputs = SmokeMute::get_inputs();
    let expecteds = SmokeMute::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SmokeMute::get_grammar();
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

#[test]
fn run_suite_wiki() -> Result<(), ParseError> {
    let inputs = SuiteWiki::get_inputs();
    let expecteds = SuiteWiki::get_expected();
    for i in 0..inputs.len() {
        println!("==== input = {}", inputs[i].chars().take(20).collect::<String>());
        let g = SuiteWiki::get_grammar();
        let mut parser = Parser::new(g);
        let arena = parser.parse(inputs[i])?;
        let result = Parser::tree_to_testfmt(&arena);
        assert_eq!(expecteds[i], result);
    }
    Ok(())
}

