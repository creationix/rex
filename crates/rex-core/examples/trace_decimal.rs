fn main() {
    use rex_core::{lexer, parser, syntax, lower, bytecode, decompile};

    let source = "314e-2";
    eprintln!("=== Input: {source}");

    let tokens = lexer::lex(source);
    eprintln!("Tokens: {:?}", tokens.iter().map(|t| (t.kind, &source[t.span.clone()])).collect::<Vec<_>>());

    let (green, errors) = parser::parse(source, &tokens);
    assert!(errors.is_empty());
    let root = syntax::SyntaxNode::new_root(green);

    let value = lower::lower(&root);
    eprintln!("Lowered: {value:?}");

    let rx = bytecode::encode(&value);
    eprintln!("Encoded: {rx}");

    let decoded = bytecode::decode(&rx).unwrap();
    eprintln!("Decoded: {decoded:?}");

    let rex = decompile::decompile(&decoded);
    eprintln!("Decompiled: {rex}");

    // Re-compile
    let tokens2 = lexer::lex(&rex);
    let (green2, _) = parser::parse(&rex, &tokens2);
    let root2 = syntax::SyntaxNode::new_root(green2);
    let value2 = lower::lower(&root2);
    eprintln!("Re-lowered: {value2:?}");

    assert_eq!(value, value2, "MISMATCH!");
    eprintln!("=== Round-trip OK");
}
