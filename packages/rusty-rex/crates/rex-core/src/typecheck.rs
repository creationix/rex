//! Type checker for Rex programs.
//!
//! Infers types from `.rexd` domain interface files and Rex source code.
//! No user-written type annotations — all types are inferred from domain
//! files, literals, operators, and type predicates.

use std::collections::HashMap;

use crate::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

// ── Type representation ───────────────────────────────────────────────────

/// A Rex type. All object/map forms are unified into `Object`.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Some,
    None,
    Never,
    Null,
    Bool,
    Int,
    Number,
    Str,
    LiteralStr(String),
    Array(Box<Type>),
    Object {
        fields: Vec<(String, Type)>,
        wildcard: Option<Box<Type>>,
    },
    Union(Vec<Type>),
    Ref(String),
}

impl Type {
    pub fn unknown() -> Type {
        Type::Union(vec![Type::Some, Type::None])
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Type::None)
    }

    /// Remove `None` from a type (for existence narrowing).
    pub fn remove_none(&self) -> Type {
        match self {
            Type::None => Type::Never,
            Type::Union(types) => {
                let filtered: Vec<Type> = types.iter()
                    .filter(|t| !t.is_none())
                    .cloned()
                    .collect();
                match filtered.len() {
                    0 => Type::Never,
                    1 => filtered.into_iter().next().unwrap(),
                    _ => Type::Union(filtered),
                }
            }
            other => other.clone(),
        }
    }

    /// Add `None` to a type if not already present.
    pub fn add_none(&self) -> Type {
        if self.contains_none() {
            return self.clone();
        }
        match self {
            Type::Union(types) => {
                let mut types = types.clone();
                types.push(Type::None);
                Type::Union(types)
            }
            other => Type::Union(vec![other.clone(), Type::None]),
        }
    }

    fn contains_none(&self) -> bool {
        match self {
            Type::None => true,
            Type::Union(types) => types.iter().any(|t| t.contains_none()),
            _ => false,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Type::Some => "some".into(),
            Type::None => "none".into(),
            Type::Never => "never".into(),
            Type::Null => "null".into(),
            Type::Bool => "boolean".into(),
            Type::Int => "integer".into(),
            Type::Number => "number".into(),
            Type::Str => "string".into(),
            Type::LiteralStr(s) => format!("\"{s}\""),
            Type::Array(t) => format!("[{}]", t.display()),
            Type::Object { fields, wildcard } => {
                let mut parts = Vec::new();
                for (k, v) in fields {
                    parts.push(format!("{k}: {}", v.display()));
                }
                if let Some(w) = wildcard {
                    parts.push(format!("*: {}", w.display()));
                }
                format!("{{{}}}", parts.join(", "))
            }
            Type::Union(types) => {
                types.iter().map(|t| t.display()).collect::<Vec<_>>().join(" | ")
            }
            Type::Ref(name) => name.clone(),
        }
    }
}

// ── Domain schema ─────────────────────────────────────────────────────────

/// Parsed domain schema from a `.rexd` file.
#[derive(Debug, Default)]
pub struct DomainSchema {
    pub type_aliases: HashMap<String, Type>,
    pub globals: HashMap<String, GlobalEntry>,
    pub functions: HashMap<String, FunctionSig>,
}

#[derive(Debug, Clone)]
pub struct GlobalEntry {
    pub ty: Type,
    pub mutable: bool,
    pub doc: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FunctionSig {
    pub args: Vec<(String, Type)>,
    pub rest: Option<(String, Type)>,
    pub returns: Type,
    pub doc: Option<String>,
}

// ── .rexd CST walker ──────────────────────────────────────────────────────

/// Parse a `.rexd` source string into a `DomainSchema`.
pub fn parse_rexd(source: &str) -> DomainSchema {
    let tokens = crate::lexer::lex(source);
    let (green, _errors) = crate::parser::parse(source, &tokens);
    let root = SyntaxNode::new_root(green);
    extract_schema(&root, source)
}

/// Walk a parsed CST and extract type/extern declarations into a schema.
fn extract_schema(root: &SyntaxNode, _source: &str) -> DomainSchema {
    let mut schema = DomainSchema::default();
    let mut pending_doc: Option<String> = Option::None;

    for child in root.children_with_tokens() {
        match &child {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::LineComment => {
                let text = t.text().trim_start_matches("//").trim();
                pending_doc = Some(match pending_doc.take() {
                    Some(mut existing) => { existing.push('\n'); existing.push_str(text); existing }
                    Option::None => text.to_string(),
                });
                continue;
            }
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Whitespace => {
                // A blank line resets doc comments. Since LineComment tokens include
                // their trailing \n, any \n in whitespace after a comment means a blank line.
                if pending_doc.is_some() && t.text().contains('\n') {
                    pending_doc = Option::None;
                }
                continue;
            }
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::TypeDecl => {
                if let Some((name, ty)) = extract_type_decl(n) {
                    schema.type_aliases.insert(name, ty);
                }
                pending_doc = Option::None;
            }
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::ExternDecl => {
                extract_extern_decl(n, &mut schema, pending_doc.take());
            }
            _ => {
                pending_doc = Option::None;
            }
        }
    }

    schema
}

/// Extract a `type Name = TypeExpr` declaration.
fn extract_type_decl(node: &SyntaxNode) -> Option<(String, Type)> {
    let mut tokens = non_trivia_children(node);

    // Skip `type` keyword
    let kw = tokens.next()?;
    if as_token_kind(&kw) != Some(SyntaxKind::KwType) { return Option::None; }

    // Name
    let name_child = tokens.next()?;
    let name = as_token_text(&name_child)?;

    // Skip `=`
    let eq = tokens.next()?;
    if as_token_kind(&eq) != Some(SyntaxKind::Eq) { return Option::None; }

    // Type expression — next child is the parsed expression
    let type_child = tokens.next()?;
    let ty = interpret_type_child(&type_child);

    Some((name.to_string(), ty))
}

/// Extract an `extern [mut] name = TypeExpr` or `extern [mut] name.fn(args) [-> ReturnType]`.
fn extract_extern_decl(node: &SyntaxNode, schema: &mut DomainSchema, doc: Option<String>) {
    let mut tokens = non_trivia_children(node);

    // Skip `extern` keyword
    let kw = tokens.next();
    if kw.is_none() { return; }

    // Check for `mut`
    let mut mutable = false;
    let next = match tokens.next() {
        Some(child) => {
            if as_token_text(&child) == Some("mut") {
                mutable = true;
                tokens.next()
            } else {
                Some(child)
            }
        }
        Option::None => return,
    };

    let next = match next {
        Some(n) => n,
        Option::None => return,
    };

    // The body is an expression parsed by parse_expr, optionally followed by `-> ReturnType`:
    // - AssignExpr for `name = type`
    // - CallExpr for `name.fn(args)` — may be followed by `-> ReturnType`
    match &next {
        rowan::NodeOrToken::Node(n) => {
            match n.kind() {
                SyntaxKind::AssignExpr => {
                    extract_extern_assign(n, mutable, doc, schema);
                }
                SyntaxKind::CallExpr => {
                    // Check for `-> ReturnType` after the call
                    let mut return_type = Option::None;
                    if let Some(arrow) = tokens.next() {
                        if as_token_kind(&arrow) == Some(SyntaxKind::Arrow) {
                            if let Some(ret_child) = tokens.next() {
                                return_type = Some(interpret_type_child(&ret_child));
                            }
                        }
                    }
                    extract_extern_function(n, return_type, doc, schema);
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// Extract `name = TypeExpr` from an AssignExpr inside an ExternDecl.
/// Always a global declaration (functions use `->` and are handled separately).
fn extract_extern_assign(
    node: &SyntaxNode,
    mutable: bool,
    doc: Option<String>,
    schema: &mut DomainSchema,
) {
    let children: Vec<_> = non_trivia_children(node).collect();

    // Find the `=` token to split LHS and RHS
    let eq_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::Eq));
    let eq_idx = match eq_idx {
        Some(i) => i,
        Option::None => return,
    };

    let lhs = &children[..eq_idx];
    let rhs = &children[eq_idx + 1..];

    if lhs.is_empty() || rhs.is_empty() { return; }

    let name = match extract_dotted_name(lhs) {
        Some(n) => n,
        Option::None => return,
    };
    let ty = interpret_type_expr_from_children(rhs);

    schema.globals.insert(name, GlobalEntry { ty, mutable, doc });
}

/// Extract a function signature from a CallExpr.
fn extract_extern_function(
    call_node: &SyntaxNode,
    return_type: Option<Type>,
    doc: Option<String>,
    schema: &mut DomainSchema,
) {
    let children: Vec<_> = non_trivia_children(call_node).collect();
    if children.is_empty() { return; }

    // First child(ren) before `(` form the function name (could be NavExpr or Ident)
    // Find the LParen
    let lparen_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::LParen));
    let lparen_idx = match lparen_idx {
        Some(i) => i,
        Option::None => return,
    };

    let name_parts = &children[..lparen_idx];
    let name = match extract_dotted_name(name_parts) {
        Some(n) => n,
        Option::None => return,
    };

    // Extract args between ( and )
    let rparen_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::RParen))
        .unwrap_or(children.len());
    let arg_tokens = &children[lparen_idx + 1..rparen_idx];

    let (args, rest) = extract_function_args(arg_tokens);

    let returns = return_type.unwrap_or(Type::None);

    schema.functions.insert(name, FunctionSig { args, rest, returns, doc });
}

/// Extract function arguments from the raw tokens between ( and ).
/// Handles `name: Type` pairs separated by commas, and `...name: Type` rest params.
///
/// The parser now parses `name: Type` as an AssignExpr (type annotation without `= value`).
/// Each arg is either an AssignExpr node containing [name, Colon, type] or flat tokens.
fn extract_function_args(
    tokens: &[rowan::NodeOrToken<SyntaxNode, SyntaxToken>],
) -> (Vec<(String, Type)>, Option<(String, Type)>) {
    let mut args = Vec::new();
    let mut rest = Option::None;

    let groups = split_by_comma(tokens);

    for group in groups {
        if group.is_empty() { continue; }

        // Check for rest parameter: ...name: Type
        let is_rest = group.len() >= 2
            && as_token_kind(&group[0]) == Some(SyntaxKind::DotDot)
            && as_token_kind(&group[1]) == Some(SyntaxKind::Dot);

        let param_start = if is_rest { 2 } else { 0 };
        if param_start >= group.len() { continue; }

        // Try to extract name: Type from the group
        let (name, ty) = if let Some(n) = as_node(&group[param_start]) {
            if n.kind() == SyntaxKind::AssignExpr {
                // AssignExpr contains: [name, Colon, type-expr, ...]
                extract_typed_param(n)
            } else {
                continue;
            }
        } else {
            // Flat tokens — flatten Error nodes and find Colon
            let flat: Vec<_> = group[param_start..].iter()
                .flat_map(|t| {
                    if let Some(n) = as_node(t) {
                        if n.kind() == SyntaxKind::Error {
                            return n.children_with_tokens()
                                .filter(|c| c.as_token().map_or(true, |t| !t.kind().is_trivia()))
                                .collect::<Vec<_>>();
                        }
                    }
                    vec![t.clone()]
                })
                .collect();
            let colon_idx = flat.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::Colon));
            if let Some(ci) = colon_idx {
                let name = extract_dotted_name(&flat[..ci]).unwrap_or_default();
                let ty = interpret_type_expr_from_children(&flat[ci + 1..]);
                (name, ty)
            } else {
                continue;
            }
        };

        if is_rest {
            rest = Some((name, ty));
        } else {
            args.push((name, ty));
        }
    }

    (args, rest)
}

/// Extract name and type from an AssignExpr node that represents `name: Type`.
fn extract_typed_param(node: &SyntaxNode) -> (String, Type) {
    let children: Vec<_> = non_trivia_children(node).collect();
    let colon_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::Colon));
    if let Some(ci) = colon_idx {
        let name = extract_dotted_name(&children[..ci]).unwrap_or_default();
        // Type is after colon, but before `=` if present (it won't be for function args)
        let eq_idx = children[ci + 1..].iter().position(|c| as_token_kind(c) == Some(SyntaxKind::Eq));
        let type_end = eq_idx.map_or(children.len(), |ei| ci + 1 + ei);
        let ty = interpret_type_expr_from_children(&children[ci + 1..type_end]);
        (name, ty)
    } else {
        (String::new(), Type::unknown())
    }
}

/// Split a token slice by Comma tokens into owned groups.
fn split_by_comma(
    tokens: &[rowan::NodeOrToken<SyntaxNode, SyntaxToken>],
) -> Vec<Vec<rowan::NodeOrToken<SyntaxNode, SyntaxToken>>> {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    for t in tokens {
        if as_token_kind(t) == Some(SyntaxKind::Comma) {
            groups.push(current);
            current = Vec::new();
        } else {
            current.push(t.clone());
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

// ── Type expression interpreter ───────────────────────────────────────────

/// Interpret a sequence of CST children as a type expression.
fn interpret_type_expr_from_children(
    children: &[rowan::NodeOrToken<SyntaxNode, SyntaxToken>],
) -> Type {
    if children.is_empty() {
        return Type::unknown();
    }
    if children.len() == 1 {
        return interpret_type_child(&children[0]);
    }
    // Multiple children — could be from a complex expression that wasn't wrapped in a node
    interpret_type_child(&children[0])
}

/// Interpret a single CST child (node or token) as a type.
fn interpret_type_child(child: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>) -> Type {
    match child {
        rowan::NodeOrToken::Token(t) => interpret_type_token(t),
        rowan::NodeOrToken::Node(n) => interpret_type_node(n),
    }
}

/// Interpret a token as a type.
fn interpret_type_token(token: &SyntaxToken) -> Type {
    match token.kind() {
        SyntaxKind::KwString => Type::Str,
        SyntaxKind::KwNumber => Type::Number,
        SyntaxKind::KwBoolean => Type::Bool,
        SyntaxKind::KwNull => Type::Null,
        SyntaxKind::KwNone => Type::None,
        SyntaxKind::Ident => {
            let text = token.text();
            match text {
                "integer" => Type::Int,
                "some" => Type::Some,
                "unknown" => Type::unknown(),
                "never" => Type::Never,
                _ => Type::Ref(text.to_string()),
            }
        }
        SyntaxKind::DoubleString | SyntaxKind::SingleString => {
            let text = token.text();
            // Strip quotes
            let inner = &text[1..text.len() - 1];
            Type::LiteralStr(inner.to_string())
        }
        _ => Type::unknown(),
    }
}

/// Interpret a composite node as a type.
fn interpret_type_node(node: &SyntaxNode) -> Type {
    match node.kind() {
        SyntaxKind::BinaryExpr => interpret_type_binary(node),
        SyntaxKind::ArrayExpr => interpret_type_array(node),
        SyntaxKind::ObjectExpr => interpret_type_object(node),
        SyntaxKind::GroupExpr => {
            // Parenthesized type — unwrap
            for child in node.children_with_tokens() {
                match &child {
                    rowan::NodeOrToken::Node(n) => return interpret_type_node(n),
                    rowan::NodeOrToken::Token(t) if !t.kind().is_trivia()
                        && t.kind() != SyntaxKind::LParen
                        && t.kind() != SyntaxKind::RParen => {
                        return interpret_type_token(t);
                    }
                    _ => {}
                }
            }
            Type::unknown()
        }
        _ => Type::unknown(),
    }
}

/// Interpret a BinaryExpr as a union type (the `|` operator).
fn interpret_type_binary(node: &SyntaxNode) -> Type {
    let children: Vec<_> = non_trivia_children(node).collect();

    // Find the operator
    let op_idx = children.iter().position(|c| {
        matches!(as_token_kind(c), Some(SyntaxKind::Pipe))
    });

    if let Some(idx) = op_idx {
        let left = interpret_type_expr_from_children(&children[..idx]);
        let right = interpret_type_expr_from_children(&children[idx + 1..]);

        // Flatten nested unions
        let mut types = Vec::new();
        match left {
            Type::Union(ts) => types.extend(ts),
            other => types.push(other),
        }
        match right {
            Type::Union(ts) => types.extend(ts),
            other => types.push(other),
        }
        Type::Union(types)
    } else {
        Type::unknown()
    }
}

/// Interpret an ArrayExpr as an array type: `[T]`.
fn interpret_type_array(node: &SyntaxNode) -> Type {
    let mut inner = Type::unknown();
    for child in node.children_with_tokens() {
        match &child {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::LBracket
                || t.kind() == SyntaxKind::RBracket
                || t.kind().is_trivia() => continue,
            other => {
                inner = interpret_type_child(other);
                break;
            }
        }
    }
    Type::Array(Box::new(inner))
}

/// Interpret an ObjectExpr as an object type: `{key: T, *: U}`.
fn interpret_type_object(node: &SyntaxNode) -> Type {
    let mut fields = Vec::new();
    let mut wildcard = Option::None;

    for child in node.children() {
        if child.kind() == SyntaxKind::Pair {
            let pair_children: Vec<_> = non_trivia_children(&child).collect();

            // Find colon
            let colon_idx = pair_children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::Colon));
            if let Some(ci) = colon_idx {
                let key_parts = &pair_children[..ci];
                let val_parts = &pair_children[ci + 1..];

                // Check if key is `*` (wildcard)
                let is_wildcard = key_parts.len() == 1
                    && as_token_kind(&key_parts[0]) == Some(SyntaxKind::Star);

                let val_type = interpret_type_expr_from_children(val_parts);

                if is_wildcard {
                    wildcard = Some(Box::new(val_type));
                } else {
                    let key_name = extract_dotted_name(key_parts).unwrap_or_default();
                    fields.push((key_name, val_type));
                }
            }
        }
    }

    Type::Object { fields, wildcard }
}

// ── CST helpers ───────────────────────────────────────────────────────────

/// Iterate non-trivia children of a node.
fn non_trivia_children(
    node: &SyntaxNode,
) -> impl Iterator<Item = rowan::NodeOrToken<SyntaxNode, SyntaxToken>> {
    node.children_with_tokens()
        .filter(|c| {
            c.as_token().map_or(true, |t| !t.kind().is_trivia())
        })
}

/// Get the SyntaxKind of a child if it's a token.
fn as_token_kind(child: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>) -> Option<SyntaxKind> {
    child.as_token().map(|t| t.kind())
}

/// Get the text of a child if it's a token.
fn as_token_text<'a>(child: &'a rowan::NodeOrToken<SyntaxNode, SyntaxToken>) -> Option<&'a str> {
    child.as_token().map(|t| t.text()).filter(|t| !t.is_empty())
}

/// Get the inner node if this is a Node.
fn as_node(child: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>) -> Option<&SyntaxNode> {
    match child {
        rowan::NodeOrToken::Node(n) => Some(n),
        _ => Option::None,
    }
}

/// Extract a dotted name from a sequence of children.
/// Handles `Ident`, `NavExpr(Ident.Ident)`, etc.
fn extract_dotted_name(children: &[rowan::NodeOrToken<SyntaxNode, SyntaxToken>]) -> Option<String> {
    if children.is_empty() { return Option::None; }

    // Single token
    if children.len() == 1 {
        match &children[0] {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Ident => {
                return Some(t.text().to_string());
            }
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::NavExpr => {
                return Some(collect_nav_name(n));
            }
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::CallExpr => {
                // Function name is the callee part of the call
                return extract_call_name(n);
            }
            _ => return Option::None,
        }
    }

    // Multiple children — join idents with dots
    let mut parts = Vec::new();
    for child in children {
        if let Some(text) = as_token_text(child) {
            if text != "." {
                parts.push(text.to_string());
            }
        }
    }
    if parts.is_empty() { return Option::None; }
    Some(parts.join("."))
}

/// Collect a dotted name from a NavExpr node.
fn collect_nav_name(node: &SyntaxNode) -> String {
    let mut parts = Vec::new();
    for child in node.children_with_tokens() {
        match &child {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Ident => {
                parts.push(t.text().to_string());
            }
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::DecimalNumber => {
                parts.push(t.text().to_string());
            }
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::NavExpr => {
                parts.push(collect_nav_name(n));
            }
            _ => {}
        }
    }
    parts.join(".")
}

/// Extract the function name from a CallExpr (the callee before the parens).
fn extract_call_name(node: &SyntaxNode) -> Option<String> {
    for child in node.children_with_tokens() {
        match &child {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::LParen => break,
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Ident => {
                return Some(t.text().to_string());
            }
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::NavExpr => {
                return Some(collect_nav_name(n));
            }
            _ => {}
        }
    }
    Option::None
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_type_alias_string() {
        let schema = parse_rexd("type Foo = string");
        assert_eq!(schema.type_aliases.get("Foo"), Some(&Type::Str));
    }

    #[test]
    fn parse_type_alias_union() {
        let schema = parse_rexd(r#"type Method = "GET" | "POST""#);
        let ty = schema.type_aliases.get("Method").unwrap();
        assert_eq!(ty, &Type::Union(vec![
            Type::LiteralStr("GET".into()),
            Type::LiteralStr("POST".into()),
        ]));
    }

    #[test]
    fn parse_type_alias_multi_union() {
        let schema = parse_rexd(r#"type M = "GET" | "POST" | "PUT""#);
        let ty = schema.type_aliases.get("M").unwrap();
        match ty {
            Type::Union(types) => assert_eq!(types.len(), 3),
            _ => panic!("expected union, got {ty:?}"),
        }
    }

    #[test]
    fn parse_type_alias_object() {
        let schema = parse_rexd("type Point = {x: integer, y: integer}");
        let ty = schema.type_aliases.get("Point").unwrap();
        match ty {
            Type::Object { fields, wildcard } => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0], ("x".into(), Type::Int));
                assert_eq!(fields[1], ("y".into(), Type::Int));
                assert!(wildcard.is_none());
            }
            _ => panic!("expected object, got {ty:?}"),
        }
    }

    #[test]
    fn parse_type_alias_map() {
        let schema = parse_rexd("type Headers = {*: string}");
        let ty = schema.type_aliases.get("Headers").unwrap();
        match ty {
            Type::Object { fields, wildcard } => {
                assert!(fields.is_empty());
                assert_eq!(wildcard.as_deref(), Some(&Type::Str));
            }
            _ => panic!("expected map, got {ty:?}"),
        }
    }

    #[test]
    fn parse_type_alias_array() {
        let schema = parse_rexd("type Names = [string]");
        let ty = schema.type_aliases.get("Names").unwrap();
        assert_eq!(ty, &Type::Array(Box::new(Type::Str)));
    }

    #[test]
    fn parse_type_alias_ref() {
        let schema = parse_rexd("type Req = {headers: Headers}");
        let ty = schema.type_aliases.get("Req").unwrap();
        match ty {
            Type::Object { fields, .. } => {
                assert_eq!(fields[0], ("headers".into(), Type::Ref("Headers".into())));
            }
            _ => panic!("expected object, got {ty:?}"),
        }
    }

    #[test]
    fn parse_extern_simple() {
        let schema = parse_rexd("extern config = unknown");
        let g = schema.globals.get("config").unwrap();
        assert_eq!(g.ty, Type::unknown());
        assert!(!g.mutable);
    }

    #[test]
    fn parse_extern_mut() {
        let schema = parse_rexd("extern mut status = integer");
        let g = schema.globals.get("status").unwrap();
        assert_eq!(g.ty, Type::Int);
        assert!(g.mutable);
    }

    #[test]
    fn parse_extern_object() {
        let schema = parse_rexd("extern req = {method: string, path: string}");
        let g = schema.globals.get("req").unwrap();
        match &g.ty {
            Type::Object { fields, .. } => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].0, "method");
                assert_eq!(fields[1].0, "path");
            }
            _ => panic!("expected object, got {:?}", g.ty),
        }
    }

    #[test]
    fn parse_extern_function_with_return() {
        let schema = parse_rexd("extern json.parse(text: string) -> some");
        let f = schema.functions.get("json.parse").unwrap();
        assert_eq!(f.args.len(), 1);
        assert_eq!(f.args[0], ("text".into(), Type::Str));
        assert_eq!(f.returns, Type::Some);
    }

    #[test]
    fn parse_extern_function_no_return() {
        let schema = parse_rexd("extern log.info(message: some)");
        let f = schema.functions.get("log.info").unwrap();
        assert_eq!(f.args.len(), 1);
        assert_eq!(f.args[0], ("message".into(), Type::Some));
        assert_eq!(f.returns, Type::None);
    }

    #[test]
    fn parse_extern_function_multiple_args() {
        let schema = parse_rexd("extern db.set(key: string, value: string) -> boolean");
        let f = schema.functions.get("db.set").unwrap();
        assert_eq!(f.args.len(), 2);
        assert_eq!(f.args[0], ("key".into(), Type::Str));
        assert_eq!(f.args[1], ("value".into(), Type::Str));
        assert_eq!(f.returns, Type::Bool);
    }

    #[test]
    fn parse_doc_comments() {
        let schema = parse_rexd("// Parse a JSON string\nextern json.parse(text: string) -> some");
        let f = schema.functions.get("json.parse").unwrap();
        assert_eq!(f.doc.as_deref(), Some("Parse a JSON string"));
    }

    #[test]
    fn parse_doc_comments_multiline() {
        let schema = parse_rexd("// Line one\n// Line two\nextern config = unknown");
        let g = schema.globals.get("config").unwrap();
        assert_eq!(g.doc.as_deref(), Some("Line one\nLine two"));
    }

    #[test]
    fn blank_line_resets_doc() {
        let schema = parse_rexd("// Not attached\n\nextern config = unknown");
        let g = schema.globals.get("config").unwrap();
        assert!(g.doc.is_none());
    }

    #[test]
    fn parse_real_rexd_file() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/knowledge-base/rex-serve.rexd"),
        ).unwrap();
        let schema = parse_rexd(&source);

        // Type aliases
        assert!(schema.type_aliases.contains_key("Headers"));
        assert!(schema.type_aliases.contains_key("HttpMethod"));
        assert!(schema.type_aliases.contains_key("DbEntry"));

        // Globals
        assert!(schema.globals.contains_key("req"));
        assert!(!schema.globals.get("req").unwrap().mutable);
        assert!(schema.globals.contains_key("res"));
        assert!(!schema.globals.get("res").unwrap().mutable); // binding is not mut; fields are
        assert!(schema.globals.contains_key("config"));

        // Functions
        assert!(schema.functions.contains_key("json.parse"));
        assert!(schema.functions.contains_key("json.stringify"));
        assert!(schema.functions.contains_key("db.get"));
        assert!(schema.functions.contains_key("log.info"));

        // Spot-check a function
        let jp = schema.functions.get("json.parse").unwrap();
        assert_eq!(jp.args.len(), 1);
        assert_eq!(jp.args[0].0, "text");
        assert_eq!(jp.returns, Type::Some);
    }
}
