use xrust::{forest::Forest, Evaluator, Constructor, SequenceTrait, xpath, Sequence, StaticContext, Item, from_document};
use std::rc::Rc;

// Cargo.toml:
// xrust = "0.7"

#[test]
fn test_xrust_xpath() {
    // how do we get a StaticContext::new_with_builtins() into the Evaluator?
    let parsed_expr = xpath::parse("/child::a/child::*");
    assert!(parsed_expr.is_ok());
    let xpath = parsed_expr.expect("Error parsing expression");
    let mut f = Forest::new();
    let input_doc = f.grow_tree("<a><b>c</b></a>").expect("XML parse failed..."); 
    let doc_node = f.get_ref(input_doc).expect("tree doesn't exist!").get_doc_node();
    let output_doc = f.plant_tree();
    let evaluator = Evaluator::new();
    let result = evaluator.evaluate(
        Some(Sequence::from(doc_node)), // context
        Some(1), // position
        &xpath, // XPath
        &mut f, // forest
        input_doc, // source document
        output_doc); // result document
    assert!(result.is_ok());
    assert_eq!(result.expect("Evaluation error").to_xml(Some(&f)), "<b>c</b>");
}

#[test]
fn test_xrust_xslt() {
    // from https://docs.rs/xrust/latest/xrust/xslt/index.html

    let mut sc = StaticContext::new_with_builtins();

    // Now create a forest for all of the trees
    let mut f = Forest::new();
    
    // The source document (a tree)
    let src = f.grow_tree("<Example><Title>XSLT in Rust</Title><Paragraph>A simple document.</Paragraph></Example>")
        .expect("unable to parse XML");
    
    // Make an item that contains the source document
    let isrc = Rc::new(Item::Node(f.get_ref(src).unwrap().get_doc_node()));
    
    // The XSL stylesheet
    let style = f.grow_tree("<xsl:stylesheet xmlns:xsl='http://www.w3.org/1999/XSL/Transform'>
      <xsl:template match='child::Example'><html><xsl:apply-templates/></html></xsl:template>
      <xsl:template match='child::Title'><head><title><xsl:apply-templates/></title></head></xsl:template>
      <xsl:template match='child::Paragraph'><body><p><xsl:apply-templates/></p></body></xsl:template>
    </xsl:stylesheet>")
        .expect("unable to parse stylesheet");
    
    // Compile the stylesheet
    let ev = from_document(
        &mut f,
        style,
        &mut sc,
        None,
    )
        .expect("failed to compile stylesheet");
    
    // Make a result document
    let rd = f.plant_tree();
    
    // Prime the stylesheet evaluation by finding the template for the document root
    // and making the document root the initial context
    let t = ev.find_match(&isrc, &mut f, src, rd, None)
        .expect("unable to find match");
    
    // Let 'er rip!
    // Evaluate the sequence constructor with the source document as the initial context
    let seq = ev.evaluate(Some(vec![Rc::clone(&isrc)]), Some(0), &t, &mut f, src, rd)
        .expect("evaluation failed");
    
    // Serialise the sequence as XML
    assert_eq!(seq.to_xml(Some(&f)), "<html><head><title>XSLT in Rust</title></head><body><p>A simple document.</p></body></html>");
}