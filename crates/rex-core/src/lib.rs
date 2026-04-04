pub mod ast;
pub mod bytecode;
pub mod decompile;
pub mod format;
pub mod heap;
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
pub fn compile_no_dedup(source: &str) -> String {
    let tokens = lexer::lex(source);
    let (green, _errors) = parser::parse(source, &tokens);
    let root = syntax::SyntaxNode::new_root(green);
    let value = lower::lower(&root);
    bytecode::encode(&value)
}

/// Compile Rex source with domain-aware shortcode rewriting.
/// Parses extern declarations from domain source (.rexd) and rewrites
/// variables/calls with explicit shortcodes to refs/opcodes.
pub fn compile_with_domain(source: &str, domain: &str) -> String {
    let tokens = lexer::lex(source);
    let (green, _errors) = parser::parse(source, &tokens);
    let root = syntax::SyntaxNode::new_root(green);
    let mut value = lower::lower(&root);

    let shortcodes = extract_shortcodes(domain);
    if !shortcodes.is_empty() {
        rewrite_shortcodes(&mut value, &shortcodes);
    }

    bytecode::encode_dedup(&value)
}

/// A shortcode mapping extracted from an extern declaration.
enum Shortcode {
    /// Binding: `extern "r" req = ...` → Variable("req") becomes Ref("r")
    Ref(String),
    /// Function: `extern "jp" json.parse(...)` → json.parse() call becomes Opcode("jp")
    Opcode(String),
}

/// Extract explicit shortcode mappings from a .rexd domain file.
/// Returns a map from source name to shortcode.
/// - Bindings: key = variable name (e.g. "req")
/// - Functions: key = "namespace.method" (e.g. "json.parse")
fn extract_shortcodes(domain: &str) -> std::collections::HashMap<String, Shortcode> {
    use syntax::SyntaxKind;

    let tokens = lexer::lex(domain);
    let (green, _errors) = parser::parse(domain, &tokens);
    let root = syntax::SyntaxNode::new_root(green);

    let mut map = std::collections::HashMap::new();

    for child in root.children() {
        if child.kind() != SyntaxKind::ExternDecl { continue; }

        // Walk non-trivia children: KwExtern, [String], [mut], body, [Arrow, RetType]
        let mut tokens_iter = child.children_with_tokens()
            .filter(|c| !matches!(c.kind(), SyntaxKind::Whitespace | SyntaxKind::LineComment | SyntaxKind::BlockComment));

        // Skip `extern`
        tokens_iter.next();

        // Check for shortcode string
        let next = match tokens_iter.next() {
            Some(c) => c,
            None => continue,
        };

        let (shortcode, body) = if matches!(next.kind(), SyntaxKind::DoubleString | SyntaxKind::SingleString) {
            let raw = next.as_token().unwrap().text();
            let sc = raw[1..raw.len()-1].to_string(); // strip quotes
            if sc.is_empty() { continue; }
            match tokens_iter.next() {
                Some(body) => (sc, body),
                None => continue,
            }
        } else {
            continue; // no shortcode — skip this extern
        };

        // Skip `mut` if present
        let body = if body.as_token().map(|t| t.text()) == Some("mut") {
            match tokens_iter.next() { Some(b) => b, None => continue }
        } else {
            body
        };

        // Determine if this is a binding (AssignExpr) or function (CallExpr)
        if let Some(node) = body.as_node() {
            match node.kind() {
                SyntaxKind::AssignExpr => {
                    // extern "r" req = ... → key is "req"
                    if let Some(name_token) = node.children_with_tokens()
                        .find(|c| c.kind() == SyntaxKind::Ident)
                    {
                        let name = name_token.as_token().unwrap().text().to_string();
                        map.insert(name, Shortcode::Ref(shortcode));
                    }
                }
                SyntaxKind::CallExpr => {
                    // The callee is either a NavExpr (json.parse) or a bare Ident (html)
                    let first = node.children_with_tokens()
                        .find(|c| !matches!(c.kind(), SyntaxKind::Whitespace | SyntaxKind::LineComment | SyntaxKind::BlockComment));
                    if let Some(first) = first {
                        if let Some(nav) = first.as_node() {
                            if nav.kind() == SyntaxKind::NavExpr {
                                // extern "jp" json.parse(...) → key is "json.parse"
                                let idents: Vec<String> = nav.children_with_tokens()
                                    .filter(|c| c.kind() == SyntaxKind::Ident)
                                    .map(|c| c.as_token().unwrap().text().to_string())
                                    .collect();
                                if idents.len() >= 2 {
                                    let key = format!("{}.{}", idents[0], idents[1]);
                                    map.insert(key, Shortcode::Opcode(shortcode));
                                }
                            }
                        } else if first.kind() == SyntaxKind::Ident {
                            // extern "h" html(...) → key is "html"
                            let name = first.as_token().unwrap().text().to_string();
                            map.insert(name, Shortcode::Opcode(shortcode));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    map
}

/// Rewrite the value tree, replacing variables/calls that have shortcodes.
fn rewrite_shortcodes(value: &mut bytecode::Value, map: &std::collections::HashMap<String, Shortcode>) {
    use bytecode::Value;
    match value {
        Value::Call(items) => {
            if !items.is_empty() {
                let rewrite = match &items[0] {
                    // Dotted: Call([Call([Variable(ns), String(method)]), args...])
                    Value::Call(inner) if inner.len() == 2 => {
                        if let (Value::Variable(ns), Value::String(method)) = (&inner[0], &inner[1]) {
                            let key = format!("{ns}.{method}");
                            if let Some(Shortcode::Opcode(sc)) = map.get(&key) {
                                Some(sc.clone())
                            } else { None }
                        } else { None }
                    }
                    // Bare: Call([Variable(name), args...])
                    Value::Variable(name) => {
                        if let Some(Shortcode::Opcode(sc)) = map.get(name.as_str()) {
                            Some(sc.clone())
                        } else { None }
                    }
                    _ => None,
                };
                if let Some(sc) = rewrite {
                    items[0] = Value::Opcode(sc);
                }
            }
            for item in items.iter_mut() { rewrite_shortcodes(item, map); }
        }
        Value::Variable(name) => {
            // Match binding shortcodes: Variable("req") → Ref("r")
            if let Some(Shortcode::Ref(sc)) = map.get(name.as_str()) {
                *value = Value::Ref(sc.clone());
            }
        }
        Value::Block(v) | Value::Array(v) | Value::IndexedArray(v) | Value::When(v) | Value::Or(v) | Value::And(v)
        | Value::ForIn(v) | Value::ForOf(v) | Value::While(v)
        | Value::ListCompIn(v) | Value::ListCompOf(v) | Value::ListCompWhile(v)
        | Value::MapCompIn(v) | Value::MapCompOf(v) | Value::MapCompWhile(v)
        | Value::Chain(v) => { for item in v.iter_mut() { rewrite_shortcodes(item, map); } }
        Value::Object(pairs) | Value::IndexedObject(pairs) => { for (k, v) in pairs.iter_mut() { rewrite_shortcodes(k, map); rewrite_shortcodes(v, map); } }
        Value::Set(a, b) | Value::Swap(a, b) => { rewrite_shortcodes(a, map); rewrite_shortcodes(b, map); }
        Value::Delete(a) | Value::Return(a) => { rewrite_shortcodes(a, map); }
        _ => {}
    }
}

/// Format Rex source code using the CST-based formatter.
/// Preserves comments, type annotations, extern declarations, and dynamic navigation.
pub fn format(source: &str) -> String {
    format::format(source)
}

/// Encode a JSON/data value to RX bytecode with deduplication.
pub fn encode_value(value: &bytecode::Value) -> String {
    bytecode::encode_dedup(value)
}
