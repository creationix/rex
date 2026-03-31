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

/// Compile Rex source without local variable renaming (readable bytecode).
pub fn compile_debug(source: &str) -> String {
    compile(source)
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

/// Compile Rex source with domain-aware opcode rewriting.
/// Parses extern function declarations from domain source (.rexd) and rewrites
/// `namespace.method(args)` calls to direct opcode calls.
pub fn compile_with_domain(source: &str, domain: &str) -> String {
    let tokens = lexer::lex(source);
    let (green, _errors) = parser::parse(source, &tokens);
    let root = syntax::SyntaxNode::new_root(green);
    let mut value = lower::lower(&root);

    let opcode_map = extract_opcode_map(domain);
    if !opcode_map.is_empty() {
        rewrite_domain_calls(&mut value, &opcode_map);
    }

    bytecode::encode_dedup(&value)
}

fn extract_opcode_map(domain: &str) -> std::collections::HashMap<(String, String), String> {
    let mut map = std::collections::HashMap::new();
    let mut used = std::collections::HashSet::new();

    for line in domain.lines() {
        let line = line.trim();
        if !line.starts_with("extern ") { continue; }
        let rest = line[7..].trim();
        if let Some(paren) = rest.find('(') {
            let name_part = &rest[..paren];
            if let Some(dot) = name_part.find('.') {
                let ns = &name_part[..dot];
                let method = &name_part[dot+1..];
                if ns.is_empty() || method.is_empty() { continue; }
                let mut mn = format!("{}{}", &ns[..1], &method[..1]);
                let base = mn.clone();
                let mut i = 2;
                while used.contains(&mn) { mn = format!("{base}{i}"); i += 1; }
                used.insert(mn.clone());
                map.insert((ns.to_string(), method.to_string()), mn);
            }
        }
    }
    map
}

fn rewrite_domain_calls(value: &mut bytecode::Value, map: &std::collections::HashMap<(String, String), String>) {
    use bytecode::Value;
    match value {
        Value::Call(items) => {
            // Match: Call([Call([Variable(ns), String(method)]), args...])
            let rewrite = if !items.is_empty() {
                if let Value::Call(inner) = &items[0] {
                    if inner.len() == 2 {
                        if let (Value::Variable(ns), Value::String(method)) = (&inner[0], &inner[1]) {
                            map.get(&(ns.clone(), method.clone())).cloned()
                        } else { None }
                    } else { None }
                } else { None }
            } else { None };
            if let Some(mn) = rewrite {
                items[0] = Value::Opcode(mn);
            }
            for item in items.iter_mut() { rewrite_domain_calls(item, map); }
        }
        Value::Block(v) | Value::Array(v) | Value::When(v) | Value::Or(v) | Value::And(v)
        | Value::ForIn(v) | Value::ForOf(v) | Value::While(v)
        | Value::ListCompIn(v) | Value::ListCompOf(v) | Value::ListCompWhile(v)
        | Value::MapCompIn(v) | Value::MapCompOf(v) | Value::MapCompWhile(v)
        | Value::Chain(v) => { for item in v.iter_mut() { rewrite_domain_calls(item, map); } }
        Value::Object(pairs) => { for (k, v) in pairs.iter_mut() { rewrite_domain_calls(k, map); rewrite_domain_calls(v, map); } }
        Value::Set(a, b) | Value::Swap(a, b) => { rewrite_domain_calls(a, map); rewrite_domain_calls(b, map); }
        Value::Delete(a) | Value::Return(a) => { rewrite_domain_calls(a, map); }
        _ => {}
    }
}

/// Encode a JSON/data value to RX bytecode with deduplication.
pub fn encode_value(value: &bytecode::Value) -> String {
    bytecode::encode_dedup(value)
}
