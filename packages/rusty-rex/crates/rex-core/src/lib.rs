pub mod ast;
pub mod bytecode;
pub mod decompile;
pub mod interpret;
pub mod json_fast;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod syntax;
pub mod typecheck;

/// Compile Rex source to REXC bytecode with full optimizations.
/// Always uses the full Rex pipeline (lex → parse → CST → lower → encode).
pub fn compile(source: &str) -> String {
    let tokens = lexer::lex(source);
    let (green, _errors) = parser::parse(source, &tokens);
    let root = syntax::SyntaxNode::new_root(green);
    let value = lower::lower(&root);
    bytecode::encode_dedup(&value)
}

/// Compile Rex source to REXC bytecode without pointer deduplication.
/// Use this when dedup causes issues with pointers across conditional branches.
pub fn compile_no_dedup(source: &str) -> String {
    let tokens = lexer::lex(source);
    let (green, _errors) = parser::parse(source, &tokens);
    let root = syntax::SyntaxNode::new_root(green);
    let value = lower::lower(&root);
    bytecode::encode(&value)
}

/// Encode a JSON/data value to RX bytecode with deduplication.
/// Uses the fast path (tokens → Value, no CST).
pub fn encode_value(value: &bytecode::Value) -> String {
    bytecode::encode_dedup(value)
}
