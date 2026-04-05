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
    Num,
    Str,
    LiteralStr(String),
    Array(Box<Type>),
    Object {
        fields: Vec<(String, Type)>,
        wildcard: Option<Box<Type>>,
    },
    Union(Vec<Type>),
    Intersection(Vec<Type>),
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
            Type::Intersection(types) => types.iter().all(|t| t.contains_none()),
            _ => false,
        }
    }

    /// Simplify a type: flatten nested unions, deduplicate, collapse single-element unions.
    pub fn simplify(self) -> Type {
        match self {
            Type::Union(types) => {
                let mut flat = Vec::new();
                for t in types {
                    let t = t.simplify();
                    match t {
                        Type::Union(inner) => flat.extend(inner),
                        other => flat.push(other),
                    }
                }
                // Deduplicate
                let mut seen = Vec::new();
                for t in flat {
                    if !seen.contains(&t) {
                        seen.push(t);
                    }
                }
                // If any branch is `some`, absorb all non-none concrete types
                let has_some = seen.iter().any(|t| matches!(t, Type::Some));
                if has_some {
                    seen.retain(|t| matches!(t, Type::Some | Type::None | Type::Never));
                }
                match seen.len() {
                    0 => Type::Never,
                    1 => seen.into_iter().next().unwrap(),
                    _ => Type::Union(seen),
                }
            }
            Type::Intersection(types) => {
                let mut flat = Vec::new();
                for t in types {
                    let t = t.simplify();
                    match t {
                        Type::Intersection(inner) => flat.extend(inner),
                        other => flat.push(other),
                    }
                }
                let mut seen = Vec::new();
                for t in flat {
                    if !seen.contains(&t) {
                        seen.push(t);
                    }
                }
                match seen.len() {
                    0 => Type::Some,
                    1 => seen.into_iter().next().unwrap(),
                    _ => Type::Intersection(seen),
                }
            }
            other => other,
        }
    }

    /// Check if type `self` is assignable to type `target`.
    pub fn is_assignable_to(&self, target: &Type) -> bool {
        if self == target { return true; }
        match (self, target) {
            // never is assignable to anything (bottom type)
            (Type::Never, _) => true,
            // anything is assignable to unknown (some | none)
            (_, Type::Union(types)) if types.len() == 2
                && types.contains(&Type::Some) && types.contains(&Type::None) => true,
            // any non-none type is assignable to some
            (_, Type::Some) => !self.is_none() && !matches!(self, Type::Union(ts) if ts.iter().any(|t| t.is_none())),
            // integer is assignable to number
            (Type::Int, Type::Num) => true,
            // literal string is assignable to string
            (Type::LiteralStr(_), Type::Str) => true,
            // T is assignable to T | U
            (_, Type::Union(targets)) => targets.iter().any(|t| self.is_assignable_to(t)),
            // Union is assignable if all branches are assignable
            (Type::Union(sources), _) => sources.iter().all(|s| s.is_assignable_to(target)),
            // Intersection is assignable if ANY member is assignable (it satisfies all)
            (Type::Intersection(sources), _) => sources.iter().any(|s| s.is_assignable_to(target)),
            // Assignable to intersection if assignable to ALL members
            (_, Type::Intersection(targets)) => targets.iter().all(|t| self.is_assignable_to(t)),
            // Array covariance
            (Type::Array(a), Type::Array(b)) => a.is_assignable_to(b),
            // Object structural subtyping — source must have all target fields
            (Type::Object { fields: sf, wildcard: sw },
             Type::Object { fields: tf, wildcard: tw }) => {
                // Every target field must be present in source with assignable type
                for (tk, tv) in tf {
                    if let Some((_, sv)) = sf.iter().find(|(k, _)| k == tk) {
                        if !sv.is_assignable_to(tv) { return false; }
                    } else if let Some(sw) = sw {
                        if !sw.is_assignable_to(tv) { return false; }
                    } else {
                        return false;
                    }
                }
                // If target has a wildcard, check source compatibility
                if let Some(tw_type) = tw {
                    if sw.is_none() {
                        // Source is a rigid object, target is a map — allowed if all
                        // source field values are assignable to the wildcard type
                        for (_, sv) in sf {
                            if !sv.is_assignable_to(tw_type) { return false; }
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Resolve property access on this type. Returns the type of `self.key`.
    pub fn resolve_property(&self, key: &str) -> PropertyResult {
        match self {
            Type::Object { fields, wildcard } => {
                if let Some((_, ft)) = fields.iter().find(|(k, _)| k == key) {
                    PropertyResult::Known(ft.clone())
                } else if let Some(wt) = wildcard {
                    PropertyResult::Wildcard(wt.add_none())
                } else {
                    PropertyResult::Unknown
                }
            }
            Type::Array(elem) => {
                match key {
                    "size" => PropertyResult::Known(Type::Int),
                    _ => {
                        // Numeric index access returns element type | none
                        PropertyResult::Known(elem.add_none())
                    }
                }
            }
            Type::Some => PropertyResult::Known(Type::Union(vec![Type::Some, Type::None])),
            Type::None => PropertyResult::Known(Type::None),
            Type::Never => PropertyResult::Known(Type::Never),
            Type::Union(types) => {
                // Resolve on each branch, union the results
                let mut results = Vec::new();
                let mut any_unknown = false;
                for t in types {
                    match t.resolve_property(key) {
                        PropertyResult::Known(ty) | PropertyResult::Wildcard(ty) => results.push(ty),
                        PropertyResult::UnknownInBranch(ty) => {
                            any_unknown = true;
                            results.push(ty);
                        }
                        PropertyResult::Unknown => {
                            any_unknown = true;
                            results.push(Type::None);
                        }
                    }
                }
                let combined = Type::Union(results).simplify();
                if any_unknown {
                    PropertyResult::UnknownInBranch(combined)
                } else {
                    PropertyResult::Known(combined)
                }
            }
            Type::Intersection(types) => {
                // Intersection: if ANY member has the property, it's known
                // (the value satisfies all interfaces)
                for t in types {
                    match t.resolve_property(key) {
                        PropertyResult::Known(ty) => return PropertyResult::Known(ty),
                        PropertyResult::Wildcard(ty) => return PropertyResult::Wildcard(ty),
                        _ => {}
                    }
                }
                PropertyResult::Unknown
            }
            _ => PropertyResult::Unknown,
        }
    }

    /// Check if this type is numeric (integer or number).
    pub fn is_numeric(&self) -> bool {
        match self {
            Type::Int | Type::Num => true,
            Type::Intersection(types) => types.iter().any(|t| t.is_numeric()),
            _ => false,
        }
    }

    /// Check if this type is a string type (string or literal string).
    pub fn is_string(&self) -> bool {
        match self {
            Type::Str | Type::LiteralStr(_) => true,
            Type::Intersection(types) => types.iter().any(|t| t.is_string()),
            _ => false,
        }
    }

    pub fn display(&self) -> String {
        match self {
            Type::Some => "some".into(),
            Type::None => "none".into(),
            Type::Never => "never".into(),
            Type::Null => "null".into(),
            Type::Bool => "bool".into(),
            Type::Int => "int".into(),
            Type::Num => "num".into(),
            Type::Str => "str".into(),
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
                if parts.is_empty() {
                    "{}".into()
                } else {
                    format!("{{ {} }}", parts.join(" "))
                }
            }
            Type::Union(types) => {
                types.iter().map(|t| t.display()).collect::<Vec<_>>().join(" | ")
            }
            Type::Intersection(types) => {
                types.iter().map(|t| t.display()).collect::<Vec<_>>().join(" & ")
            }
            Type::Ref(name) => name.clone(),
        }
    }
}

/// Result of resolving a property access on a type.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyResult {
    /// Property found with this type.
    Known(Type),
    /// Property found via wildcard (map type). Type includes `| none`.
    Wildcard(Type),
    /// Property not found on this type — unknown field error.
    Unknown,
    /// Property unknown on some branches of a union, but resolved on others.
    /// The type is the combined result; the checker should emit a warning.
    UnknownInBranch(Type),
}

// ── Domain schema ─────────────────────────────────────────────────────────

/// Parsed domain schema from a `.rexd` file.
#[derive(Debug, Default, Clone)]
pub struct DomainSchema {
    pub type_aliases: HashMap<String, Type>,
    pub globals: HashMap<String, GlobalEntry>,
    pub functions: HashMap<String, FunctionSig>,
}

#[derive(Debug, Clone)]
pub struct GlobalEntry {
    pub ty: Type,
    pub mutable: bool,
    /// Fields that are writable (from `mut field: T` in the type expression).
    /// Contains dot-paths like "status", "headers". Wildcard `*` means all map keys.
    pub mutable_fields: Vec<String>,
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

/// Extract an `extern [mut] name: TypeExpr` or `extern [mut] name.fn(args) [-> ReturnType]`.
fn extract_extern_decl(node: &SyntaxNode, schema: &mut DomainSchema, doc: Option<String>) {
    let mut tokens = non_trivia_children(node);

    // Skip `extern` keyword
    let kw = tokens.next();
    if kw.is_none() { return; }

    // Skip optional shortcode string: extern "jp" ...
    let next = match tokens.next() {
        Some(child) => child,
        None => return,
    };
    let next = if matches!(next.kind(), SyntaxKind::DoubleString | SyntaxKind::SingleString) {
        match tokens.next() { Some(c) => c, None => return }
    } else {
        next
    };

    // Check for `mut`
    let mut mutable = false;
    let body = if as_token_text(&next) == Some("mut") {
        mutable = true;
        match tokens.next() { Some(c) => c, None => return }
    } else {
        next
    };

    // The body is an expression parsed by parse_expr, optionally followed by `-> ReturnType`:
    // - AssignExpr for `name = type`
    // - CallExpr for `name.fn(args)` — may be followed by `-> ReturnType`
    match &body {
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

/// Extract `name: TypeExpr` from an AssignExpr inside an ExternDecl.
/// Always a global declaration (functions use `->` and are handled separately).
fn extract_extern_assign(
    node: &SyntaxNode,
    mutable: bool,
    doc: Option<String>,
    schema: &mut DomainSchema,
) {
    let children: Vec<_> = non_trivia_children(node).collect();

    // Find `=` or `:` token to split LHS and RHS
    let sep_idx = children.iter().position(|c| {
        let k = as_token_kind(c);
        k == Some(SyntaxKind::Eq) || k == Some(SyntaxKind::Colon)
    });
    let sep_idx = match sep_idx {
        Some(i) => i,
        Option::None => return,
    };

    let lhs = &children[..sep_idx];
    let rhs = &children[sep_idx + 1..];

    if lhs.is_empty() || rhs.is_empty() { return; }

    let name = match extract_dotted_name(lhs) {
        Some(n) => n,
        Option::None => return,
    };
    let ty = interpret_type_expr_from_children(rhs);

    // Extract mutable fields from the CST (look for `mut` before field names in ObjectExpr)
    let mutable_fields = extract_mutable_fields_from_cst(rhs);
    schema.globals.insert(name, GlobalEntry { ty, mutable, mutable_fields, doc });
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
        SyntaxKind::KwNull => Type::Null,
        SyntaxKind::KwNone => Type::None,
        SyntaxKind::Ident => {
            let text = token.text();
            match text {
                "str" => Type::Str,
                "int" => Type::Int,
                "num" => Type::Num,
                "bool" => Type::Bool,
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
        // New type-specific nodes
        SyntaxKind::TypeExpr => {
            // Unwrap: TypeExpr contains one child (the actual type)
            for child in node.children_with_tokens() {
                match &child {
                    rowan::NodeOrToken::Node(n) => return interpret_type_node(n),
                    rowan::NodeOrToken::Token(t) if !t.kind().is_trivia() => {
                        return interpret_type_token(t);
                    }
                    _ => {}
                }
            }
            Type::unknown()
        }
        SyntaxKind::TypeUnion => interpret_type_binary(node),
        SyntaxKind::TypeIntersection => interpret_type_binary(node),
        SyntaxKind::TypeArray => interpret_type_array(node),
        SyntaxKind::TypeObject => interpret_type_object(node),
        SyntaxKind::TypeGroup => {
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
        // Legacy nodes (from expression parser — still used in some contexts)
        SyntaxKind::BinaryExpr => interpret_type_binary(node),
        SyntaxKind::ArrayExpr => interpret_type_array(node),
        SyntaxKind::ObjectExpr => interpret_type_object(node),
        SyntaxKind::GroupExpr => {
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

/// Extract field names that have `mut` prefix from CST children of a type expression.
fn extract_mutable_fields_from_cst(
    children: &[rowan::NodeOrToken<SyntaxNode, SyntaxToken>],
) -> Vec<String> {
    let mut result = Vec::new();
    for child in children {
        if let Some(n) = as_node(child) {
            if n.kind() == SyntaxKind::ObjectExpr || n.kind() == SyntaxKind::TypeObject {
                extract_mut_from_object_node(n, &mut result);
            } else if n.kind() == SyntaxKind::TypeExpr {
                // Unwrap TypeExpr wrapper
                for inner in n.children() {
                    if inner.kind() == SyntaxKind::TypeObject || inner.kind() == SyntaxKind::ObjectExpr {
                        extract_mut_from_object_node(&inner, &mut result);
                    }
                }
            }
        }
    }
    result
}

/// Walk an ObjectExpr node looking for `mut` annotations on Pair children.
fn extract_mut_from_object_node(node: &SyntaxNode, result: &mut Vec<String>) {
    for child in node.children() {
        if child.kind() == SyntaxKind::Pair || child.kind() == SyntaxKind::TypePair {
            let children: Vec<_> = non_trivia_children(&child).collect();
            // Check if first child is Ident("mut")
            if children.len() >= 3 {
                if let Some(text) = as_token_text(&children[0]) {
                    if text == "mut" {
                        // Second child is the field name (or *)
                        if let Some(SyntaxKind::Star) = as_token_kind(&children[1]) {
                            result.push("*".to_string());
                        } else if let Some(name) = as_token_text(&children[1]) {
                            result.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
}

/// Interpret a BinaryExpr/TypeUnion/TypeIntersection as a union or intersection type.
fn interpret_type_binary(node: &SyntaxNode) -> Type {
    let children: Vec<_> = non_trivia_children(node).collect();

    // For flat TypeUnion/TypeIntersection nodes: collect all non-operator children
    if matches!(node.kind(), SyntaxKind::TypeUnion | SyntaxKind::TypeIntersection) {
        let types: Vec<Type> = children.iter()
            .filter(|c| !matches!(as_token_kind(c), Some(SyntaxKind::Pipe | SyntaxKind::Amp)))
            .map(|c| interpret_type_child(c))
            .collect();
        return if node.kind() == SyntaxKind::TypeUnion {
            Type::Union(types)
        } else {
            Type::Intersection(types)
        };
    }

    // Legacy BinaryExpr: find first operator and split left/right
    let op_idx = children.iter().position(|c| {
        matches!(as_token_kind(c), Some(SyntaxKind::Pipe | SyntaxKind::Amp))
    });

    if let Some(idx) = op_idx {
        let op = as_token_kind(&children[idx]);
        let left = interpret_type_expr_from_children(&children[..idx]);
        let right = interpret_type_expr_from_children(&children[idx + 1..]);

        match op {
            Some(SyntaxKind::Amp) => {
                // Intersection: flatten nested intersections
                let mut types = Vec::new();
                match left {
                    Type::Intersection(ts) => types.extend(ts),
                    other => types.push(other),
                }
                match right {
                    Type::Intersection(ts) => types.extend(ts),
                    other => types.push(other),
                }
                Type::Intersection(types)
            }
            _ => {
                // Union: flatten nested unions
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
            }
        }
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
        if child.kind() == SyntaxKind::Pair || child.kind() == SyntaxKind::TypePair {
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

// ── Built-in method types ─────────────────────────────────────────────────

/// Returns the return type for a built-in method call, or None if not a built-in.
fn builtin_method_type(target: &Type, method: &str, _args: &[Type]) -> Option<Type> {
    match target {
        Type::Array(elem) => match method {
            "push" => Some(Type::Array(elem.clone())),
            "pop" => Some(Type::Union(vec![*elem.clone(), Type::None])),
            "join" => Some(Type::Str),
            "indexOf" => Some(Type::Union(vec![Type::Int, Type::None])),
            "contains" => Some(Type::Union(vec![*elem.clone(), Type::None])),
            "slice" => Some(Type::Array(elem.clone())),
            _ => None,
        },
        Type::Str => match method {
            "split" => Some(Type::Array(Box::new(Type::Str))),
            "trim" => Some(Type::Str),
            "upper" => Some(Type::Str),
            "lower" => Some(Type::Str),
            "replace" => Some(Type::Str),
            "slice" => Some(Type::Str),
            "indexOf" => Some(Type::Union(vec![Type::Int, Type::None])),
            "contains" => Some(Type::Union(vec![Type::Str, Type::None])),
            "starts-with" => Some(Type::Union(vec![Type::Str, Type::None])),
            "ends-with" => Some(Type::Union(vec![Type::Str, Type::None])),
            _ => None,
        },
        // For unknown target types, still resolve if the method is known on any type
        Type::Some | Type::Union(_) => match method {
            "push" | "slice" => Some(Type::Some),
            "pop" => Some(Type::Some),
            "join" | "trim" | "upper" | "lower" | "replace" | "split" => Some(Type::Some),
            "indexOf" => Some(Type::Union(vec![Type::Int, Type::None])),
            "contains" | "starts-with" | "ends-with" => Some(Type::Some),
            _ => None,
        },
        _ => None,
    }
}

// ── Type helpers ──────────────────────────────────────────────────────────

/// Remove None from a type (comprehensions filter out none values).
fn strip_none(ty: Type) -> Type {
    match ty {
        Type::None => Type::Never,
        Type::Union(types) => {
            let filtered: Vec<Type> = types.into_iter()
                .filter(|t| !matches!(t, Type::None))
                .collect();
            match filtered.len() {
                0 => Type::Never,
                1 => filtered.into_iter().next().unwrap(),
                _ => Type::Union(filtered),
            }
        }
        other => other,
    }
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
            rowan::NodeOrToken::Token(t) if t.kind().is_keyword() => {
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


// ── Type inference engine ─────────────────────────────────────────────────

/// Diagnostic produced by the type checker.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub span: std::ops::Range<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    Error,
    Warning,
}

/// Type-check a Rex source file against a domain schema.
pub fn check_source(source: &str, schema: &DomainSchema) -> Vec<Diagnostic> {
    let tokens = crate::lexer::lex(source);
    let (green, _errors) = crate::parser::parse(source, &tokens);
    let root = SyntaxNode::new_root(green);
    let mut env = TypeEnv::new(schema);
    env.infer_program(&root);
    env.check_unused_vars();
    env.diagnostics
}

/// Type-check and return both diagnostics and a span→type map for hover.
pub fn check_source_with_types(
    source: &str,
    schema: &DomainSchema,
) -> (Vec<Diagnostic>, Vec<(std::ops::Range<usize>, Type)>, HashMap<String, FunctionSig>) {
    let tokens = crate::lexer::lex(source);
    let (green, _errors) = crate::parser::parse(source, &tokens);
    let root = SyntaxNode::new_root(green);
    let mut env = TypeEnv::new(schema);
    env.infer_program(&root);
    env.check_unused_vars();
    (env.diagnostics, env.span_types, env.inline_functions)
}

/// A narrowing constraint extracted from a condition expression.
#[derive(Debug, Clone)]
enum Narrowing {
    /// Variable exists (not none) — from `when x do`
    Exists(String),
    /// Variable has a specific type — from `when isNumber(x) do`
    TypePredicate(String, Type),
    /// Variable equals a literal — from `when x == "GET" do`
    Equals(String, Type),
    /// Variable is assigned in the condition — from `when x = expr do`
    Assigned(String, Type),
}

/// Type environment — tracks variable scopes and diagnostics.
struct TypeEnv<'a> {
    schema: &'a DomainSchema,
    /// Inline type aliases from `type` declarations in user code.
    inline_aliases: HashMap<String, Type>,
    /// Inline function signatures from `extern` declarations in user code.
    inline_functions: HashMap<String, FunctionSig>,
    scopes: Vec<HashMap<String, Type>>,
    /// Track where variables are assigned (name → span) for unused warnings.
    var_assignments: HashMap<String, std::ops::Range<usize>>,
    /// Track which variables have been read.
    var_reads: std::collections::HashSet<String>,
    diagnostics: Vec<Diagnostic>,
    /// Map from source spans to inferred types (for hover).
    span_types: Vec<(std::ops::Range<usize>, Type)>,
}

impl<'a> TypeEnv<'a> {
    fn new(schema: &'a DomainSchema) -> Self {
        // Seed top-level scope with globals from the schema
        let mut globals = HashMap::new();
        for (name, entry) in &schema.globals {
            globals.insert(name.clone(), entry.ty.clone());
        }
        Self {
            schema,
            inline_aliases: HashMap::new(),
            inline_functions: HashMap::new(),
            scopes: vec![globals],
            var_assignments: HashMap::new(),
            var_reads: std::collections::HashSet::new(),
            diagnostics: Vec::new(),
            span_types: Vec::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn set_var(&mut self, name: &str, ty: Type) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    /// Returns true if a domain schema (.rexd) was provided.
    fn has_domain(&self) -> bool {
        !self.schema.globals.is_empty() || !self.schema.functions.is_empty()
    }

    fn lookup_var(&self, name: &str) -> Option<Type> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
    }

    /// Resolve a type reference (e.g., `Headers` → the actual type from type_aliases).
    /// Recursively resolve type aliases (Ref) throughout a type.
    fn resolve_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Ref(name) => {
                self.schema.type_aliases.get(name)
                    .or_else(|| self.inline_aliases.get(name))
                    .map(|t| self.resolve_type(t))
                    .unwrap_or_else(|| ty.clone())
            }
            Type::Array(elem) => Type::Array(Box::new(self.resolve_type(elem))),
            Type::Union(types) => Type::Union(types.iter().map(|t| self.resolve_type(t)).collect()),
            Type::Intersection(types) => Type::Intersection(types.iter().map(|t| self.resolve_type(t)).collect()),
            Type::Object { fields, wildcard } => Type::Object {
                fields: fields.iter().map(|(k, v)| (k.clone(), self.resolve_type(v))).collect(),
                wildcard: wildcard.as_ref().map(|w| Box::new(self.resolve_type(w))),
            },
            _ => ty.clone(),
        }
    }

    /// Check a type for unresolved references and emit errors.
    fn validate_type(&mut self, ty: &Type, span: &std::ops::Range<usize>) {
        match ty {
            Type::Ref(name) => {
                if !self.schema.type_aliases.contains_key(name)
                    && !self.inline_aliases.contains_key(name)
                {
                    self.error(span.clone(), format!("unknown type '{name}'"));
                }
            }
            Type::Array(elem) => self.validate_type(elem, span),
            Type::Union(types) | Type::Intersection(types) => {
                for t in types {
                    self.validate_type(t, span);
                }
            }
            Type::Object { fields, wildcard } => {
                for (_, v) in fields {
                    self.validate_type(v, span);
                }
                if let Some(w) = wildcard {
                    self.validate_type(w, span);
                }
            }
            _ => {}
        }
    }

    /// Record span_types for tokens in a type annotation so they have hover info.
    /// Each type-name token gets the overall annotation type recorded.
    fn record_type_annotation_spans(
        &mut self,
        children: &[rowan::NodeOrToken<SyntaxNode, SyntaxToken>],
        ty: &Type,
    ) {
        for child in children {
            match child {
                rowan::NodeOrToken::Token(t) => {
                    match t.kind() {
                        SyntaxKind::Ident | SyntaxKind::KwNull | SyntaxKind::KwNone => {
                            let r = t.text_range();
                            // For individual type tokens, record the token's own type
                            let token_ty = interpret_type_token(t);
                            let resolved = self.resolve_type(&token_ty);
                            self.span_types.push((r.start().into()..r.end().into(), resolved));
                        }
                        _ => {}
                    }
                }
                rowan::NodeOrToken::Node(n) => {
                    // Recurse into composite nodes (BinaryExpr for unions, etc.)
                    let inner: Vec<_> = non_trivia_children(n).collect();
                    self.record_type_annotation_spans(&inner, ty);
                }
            }
        }
    }

    fn error(&mut self, span: std::ops::Range<usize>, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            kind: DiagnosticKind::Error,
            span,
            message: message.into(),
        });
    }

    fn warning(&mut self, span: std::ops::Range<usize>, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            kind: DiagnosticKind::Warning,
            span,
            message: message.into(),
        });
    }

    fn span_of(node: &SyntaxNode) -> std::ops::Range<usize> {
        let range = node.text_range();
        range.start().into()..range.end().into()
    }

    // ── Program-level inference ────────────────────────────────────────

    fn infer_program(&mut self, root: &SyntaxNode) {
        for child in root.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    self.infer_node(&n);
                    // After processing a conditional, apply flow narrowing for subsequent code
                    if n.kind() == SyntaxKind::ConditionalExpr {
                        self.apply_flow_narrowing(&n);
                    }
                }
                rowan::NodeOrToken::Token(t) if !t.kind().is_trivia() => { self.infer_token(&t); }
                _ => {}
            }
        }
    }

    // ── Expression inference ──────────────────────────────────────────

    fn infer_child(&mut self, child: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>) -> Type {
        match child {
            rowan::NodeOrToken::Node(n) => self.infer_node(n),
            rowan::NodeOrToken::Token(t) => self.infer_token(t),
        }
    }

    fn infer_node(&mut self, node: &SyntaxNode) -> Type {
        let ty = match node.kind() {
            SyntaxKind::BinaryExpr => self.infer_binary(node),
            SyntaxKind::UnaryExpr => self.infer_unary(node),
            SyntaxKind::AssignExpr => self.infer_assign(node),
            SyntaxKind::RangeExpr => Type::Array(Box::new(Type::Int)),
            SyntaxKind::CallExpr => self.infer_call(node),
            SyntaxKind::NavExpr => self.infer_nav(node),
            SyntaxKind::GroupExpr => self.infer_group(node),
            SyntaxKind::ConditionalExpr => self.infer_conditional(node),
            SyntaxKind::ForExpr => self.infer_for(node),
            SyntaxKind::WhileExpr => self.infer_while(node),
            SyntaxKind::ArrayExpr => self.infer_array(node),
            SyntaxKind::ArrayComprehension => self.infer_array_comp(node),
            SyntaxKind::ObjectExpr => self.infer_object(node),
            SyntaxKind::ObjectComprehension => self.infer_object_comp(node),
            SyntaxKind::TemplateExpr => self.infer_template(node),
            SyntaxKind::ReturnExpr => self.infer_return(node),
            SyntaxKind::TypeDecl => { self.process_type_decl(node); return Type::None }
            SyntaxKind::ExternDecl => { self.process_extern_decl(node); return Type::None }
            SyntaxKind::Block => self.infer_block(node),
            SyntaxKind::SelfExpr => Type::Some, // TODO: track self type through scopes
            _ => Type::unknown(),
        };
        let range = node.text_range();
        self.span_types.push((range.start().into()..range.end().into(), ty.clone()));
        ty
    }

    fn infer_token(&mut self, token: &SyntaxToken) -> Type {
        let ty = match token.kind() {
            SyntaxKind::DecimalNumber => {
                let text = token.text();
                if text.contains('.') || text.contains('e') || text.contains('E') {
                    Type::Num
                } else {
                    Type::Int
                }
            }
            SyntaxKind::HexNumber | SyntaxKind::BinaryNumber => Type::Int,
            SyntaxKind::DoubleString | SyntaxKind::SingleString => Type::Str,
            SyntaxKind::KwTrue | SyntaxKind::KwFalse => Type::Bool,
            SyntaxKind::KwNull => Type::Null,
            SyntaxKind::KwNone => Type::None,
            SyntaxKind::KwNan | SyntaxKind::KwInf => Type::Num,
            // self was removed as a keyword — it's now just an identifier
            SyntaxKind::Ident => {
                let name = token.text();
                self.var_reads.insert(name.to_string());
                match self.lookup_var(name) {
                    Some(ty) => ty,
                    None => {
                        // If a domain schema is loaded, undefined variables are errors
                        if self.has_domain() {
                            let range = token.text_range();
                            self.diagnostics.push(Diagnostic {
                                kind: DiagnosticKind::Error,
                                span: range.start().into()..range.end().into(),
                                message: format!("undefined variable '{name}'"),
                            });
                        }
                        Type::None
                    }
                }
            }
            _ => Type::unknown(),
        };
        let range = token.text_range();
        self.span_types.push((range.start().into()..range.end().into(), ty.clone()));
        ty
    }

    // ── Specific expression types ─────────────────────────────────────

    fn infer_binary(&mut self, node: &SyntaxNode) -> Type {
        let children: Vec<_> = non_trivia_children(node).collect();
        if children.len() < 3 { return Type::unknown(); }

        let lhs_type = self.infer_child(&children[0]);
        let op = as_token_kind(&children[1]);
        let rhs_type = self.infer_child(&children[2]);

        match op {
            // Arithmetic: + - * %
            Some(SyntaxKind::Plus) => {
                let lt = self.resolve_type(&lhs_type);
                let rt = self.resolve_type(&rhs_type);

                let lt_has_str = self.type_contains_string(&lt);
                let rt_has_str = self.type_contains_string(&rt);

                if lt_has_str && rt_has_str {
                    Type::Str // string concatenation
                } else if lt == Type::Int && rt == Type::Int {
                    Type::Int
                } else if lt.is_numeric() && rt.is_numeric() {
                    Type::Num
                } else if self.type_contains_some(&lt) || self.type_contains_some(&rt) {
                    self.error(Self::span_of(node), format!(
                        "cannot use 'some' in arithmetic — narrow first"
                    ));
                    Type::Some
                } else if lt_has_str != rt_has_str {
                    // One side has string, other doesn't — type error
                    self.error(Self::span_of(node), format!(
                        "cannot add {} and {} (use template literal for coercion)",
                        lt.display(), rt.display()
                    ));
                    Type::Str
                } else {
                    Type::Num
                }
            }
            Some(SyntaxKind::Minus | SyntaxKind::Star | SyntaxKind::Percent) => {
                let lt = self.resolve_type(&lhs_type);
                let rt = self.resolve_type(&rhs_type);
                if lt == Type::Int && rt == Type::Int {
                    Type::Int
                } else {
                    Type::Num
                }
            }
            Some(SyntaxKind::Slash) => Type::Num, // division always produces number

            // Comparison: == != > >= < <=
            Some(SyntaxKind::EqEq | SyntaxKind::BangEq
                | SyntaxKind::Gt | SyntaxKind::GtEq
                | SyntaxKind::Lt | SyntaxKind::LtEq) => {
                lhs_type.add_none()
            }

            // Bitwise / boolean value: & | ^
            Some(SyntaxKind::Amp | SyntaxKind::Pipe | SyntaxKind::Caret) => {
                let lt = self.resolve_type(&lhs_type);
                if lt == Type::Bool { Type::Bool } else { Type::Int }
            }

            // Existence: and or
            Some(SyntaxKind::KwAnd) => rhs_type.add_none(),
            Some(SyntaxKind::KwOr) => {
                // First defined value — lhs if not none, else rhs.
                // Remove none from lhs since `or` skips it.
                let lhs_no_none = lhs_type.remove_none();
                Type::Union(vec![lhs_no_none, rhs_type]).simplify()
            }

            _ => Type::unknown(),
        }
    }

    fn infer_unary(&mut self, node: &SyntaxNode) -> Type {
        let children: Vec<_> = non_trivia_children(node).collect();
        if children.len() < 2 { return Type::unknown(); }

        let op = as_token_kind(&children[0]);
        let operand_type = self.infer_child(&children[1]);

        match op {
            Some(SyntaxKind::Minus) => {
                let t = self.resolve_type(&operand_type);
                if t == Type::Int { Type::Int } else { Type::Num }
            }
            Some(SyntaxKind::Tilde) => {
                let t = self.resolve_type(&operand_type);
                if t == Type::Bool { Type::Bool } else { Type::Int }
            }
            Some(SyntaxKind::KwDelete) => {
                // Infer the operand to check it's valid, return none
                Type::None
            }
            _ => operand_type,
        }
    }

    fn infer_assign(&mut self, node: &SyntaxNode) -> Type {
        let children: Vec<_> = non_trivia_children(node).collect();
        if children.len() < 3 { return Type::unknown(); }

        // Check if this is a type-annotated assignment: name : Type [= value]
        let has_colon = children.iter().any(|c| as_token_kind(c) == Some(SyntaxKind::Colon));
        if has_colon {
            return self.infer_typed_assign(node, &children);
        }

        // Find the operator
        let op_idx = children.iter().position(|c| {
            matches!(as_token_kind(c),
                Some(SyntaxKind::Eq | SyntaxKind::ColonEq
                    | SyntaxKind::PlusEq | SyntaxKind::MinusEq
                    | SyntaxKind::StarEq | SyntaxKind::SlashEq
                    | SyntaxKind::PercentEq | SyntaxKind::AmpEq
                    | SyntaxKind::PipeEq | SyntaxKind::CaretEq))
        });
        let op_idx = match op_idx {
            Some(i) => i,
            Option::None => return Type::unknown(),
        };

        let rhs_type = self.infer_child(&children[op_idx + 1]);

        // Extract variable name from LHS
        if let Some(name) = self.extract_assign_target(&children[..op_idx]) {
            let op = as_token_kind(&children[op_idx]);

            // Check mutability for property writes
            if name.contains('.') {
                self.check_mutability(&name, Self::span_of(node));
            }

            match op {
                Some(SyntaxKind::Eq) => {
                    // Track assignment for unused variable detection
                    if !name.contains('.') && !self.schema.globals.contains_key(&name) {
                        self.var_assignments.insert(name.clone(), Self::span_of(node));
                    }
                    self.set_var(&name, rhs_type.clone());
                    rhs_type
                }
                Some(SyntaxKind::ColonEq) => {
                    // Swap: returns old value, sets new
                    let old = self.lookup_var(&name).unwrap_or(Type::None);
                    self.set_var(&name, rhs_type);
                    old
                }
                _ => {
                    // Compound assignment: mark as read (compound uses the variable)
                    self.var_reads.insert(name.clone());
                    rhs_type
                }
            }
        } else {
            rhs_type
        }
    }

    fn infer_typed_assign(&mut self, node: &SyntaxNode, children: &[rowan::NodeOrToken<SyntaxNode, SyntaxToken>]) -> Type {
        // name : Type = value  OR  name : Type (bare annotation, e.g. in function args)
        let colon_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::Colon));
        let colon_idx = match colon_idx {
            Some(i) => i,
            Option::None => return Type::unknown(),
        };

        let eq_idx = children[colon_idx + 1..].iter()
            .position(|c| as_token_kind(c) == Some(SyntaxKind::Eq))
            .map(|i| colon_idx + 1 + i);

        // Parse the type annotation
        let type_end = eq_idx.unwrap_or(children.len());
        let type_children = &children[colon_idx + 1..type_end];
        let declared_type = if type_children.len() == 1 {
            interpret_type_child(&type_children[0])
        } else {
            interpret_type_expr_from_children(type_children)
        };
        let span = Self::span_of(node);
        self.validate_type(&declared_type, &span);
        // Record span_types for type annotation tokens
        self.record_type_annotation_spans(type_children, &declared_type);
        let declared_type = self.resolve_type(&declared_type);

        // If there's a value, infer it and check assignability
        if let Some(eq_i) = eq_idx {
            if eq_i + 1 < children.len() {
                let val_type = self.infer_child(&children[eq_i + 1]);
                let val_type = self.resolve_type(&val_type);
                if !val_type.is_assignable_to(&declared_type) {
                    self.error(Self::span_of(node), format!(
                        "type {} is not assignable to {}",
                        val_type.display(), declared_type.display()
                    ));
                }
            }
        }

        // Set the variable to the declared type
        if let Some(name) = self.extract_assign_target(&children[..colon_idx]) {
            self.set_var(&name, declared_type.clone());
        }

        declared_type
    }

    fn extract_assign_target(&self, children: &[rowan::NodeOrToken<SyntaxNode, SyntaxToken>]) -> Option<String> {
        if children.len() == 1 {
            match &children[0] {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Ident => {
                    Some(t.text().to_string())
                }
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::NavExpr => {
                    Some(collect_nav_name(n))
                }
                _ => None,
            }
        } else {
            None
        }
    }

    /// Check if a type contains string (directly or in a union).
    fn type_contains_string(&self, ty: &Type) -> bool {
        match ty {
            Type::Str | Type::LiteralStr(_) => true,
            Type::Union(types) | Type::Intersection(types) => {
                types.iter().any(|t| self.type_contains_string(t))
            }
            Type::Ref(name) => {
                self.schema.type_aliases.get(name)
                    .map_or(false, |t| self.type_contains_string(t))
            }
            _ => false,
        }
    }

    /// Process a `type Name = T` declaration in user code.
    /// Walks the CST directly to extract name and type, recording spans for hover.
    fn process_type_decl(&mut self, node: &SyntaxNode) {
        let mut tokens = non_trivia_children(node);

        // Skip `type` keyword
        let kw = match tokens.next() {
            Some(c) if as_token_kind(&c) == Some(SyntaxKind::KwType) => c,
            _ => return,
        };
        let _ = kw;

        // Name token
        let name_child = match tokens.next() {
            Some(c) => c,
            _ => return,
        };
        let name = match as_token_text(&name_child) {
            Some(n) => n.to_string(),
            _ => return,
        };

        // Skip `=`
        match tokens.next() {
            Some(c) if as_token_kind(&c) == Some(SyntaxKind::Eq) => {}
            _ => return,
        }

        // Type expression
        let type_child = match tokens.next() {
            Some(c) => c,
            _ => return,
        };
        let ty = interpret_type_child(&type_child);
        let span = Self::span_of(node);
        self.validate_type(&ty, &span);
        self.inline_aliases.insert(name, ty);
    }

    /// Process an `extern [mut] name: T` or `extern fn(args) -> T` declaration in user code.
    /// Walks the CST directly, recording span_types for hover on declaration tokens.
    fn process_extern_decl(&mut self, node: &SyntaxNode) {
        let mut children = non_trivia_children(node);

        // Skip `extern` keyword
        match children.next() {
            Some(c) if as_token_kind(&c) == Some(SyntaxKind::KwExtern) => {}
            _ => return,
        }

        // Skip optional shortcode string
        let next = match children.next() {
            Some(c) => c,
            _ => return,
        };
        let next = if matches!(next.kind(), SyntaxKind::DoubleString | SyntaxKind::SingleString) {
            match children.next() { Some(c) => c, _ => return }
        } else {
            next
        };

        // Check for `mut`
        let body = if as_token_text(&next).map_or(false, |t| t == "mut") {
            match children.next() {
                Some(c) => c,
                _ => return,
            }
        } else {
            next
        };

        match as_node(&body) {
            Some(n) if n.kind() == SyntaxKind::AssignExpr => {
                self.process_extern_var(n, node);
            }
            Some(n) if n.kind() == SyntaxKind::CallExpr => {
                // Collect remaining children for `-> ReturnType`
                let mut return_type = None;
                if let Some(arrow) = children.next() {
                    if as_token_kind(&arrow) == Some(SyntaxKind::Arrow) {
                        if let Some(ret_child) = children.next() {
                            return_type = Some(interpret_type_child(&ret_child));
                        }
                    }
                }
                self.process_extern_fn(n, return_type, node);
            }
            _ => {}
        }
    }

    /// Process `extern name = Type` — a global variable declaration.
    fn process_extern_var(&mut self, assign_node: &SyntaxNode, decl_node: &SyntaxNode) {
        let children: Vec<_> = non_trivia_children(assign_node).collect();

        // Find `=` or `:` to split name and type
        let sep_idx = match children.iter().position(|c| {
            let k = as_token_kind(c);
            k == Some(SyntaxKind::Eq) || k == Some(SyntaxKind::Colon)
        }) {
            Some(i) => i,
            _ => return,
        };

        let lhs = &children[..sep_idx];
        let rhs = &children[sep_idx + 1..];
        if lhs.is_empty() || rhs.is_empty() { return; }

        let name = match extract_dotted_name(lhs) {
            Some(n) => n,
            _ => return,
        };
        let ty = interpret_type_expr_from_children(rhs);
        let span = Self::span_of(decl_node);
        self.validate_type(&ty, &span);
        // Record span_types for type annotation tokens
        self.record_type_annotation_spans(rhs, &ty);

        // Record span_type for the name token(s)
        for child in lhs {
            if let rowan::NodeOrToken::Token(t) = child {
                if t.kind() == SyntaxKind::Ident {
                    let r = t.text_range();
                    self.span_types.push((r.start().into()..r.end().into(), ty.clone()));
                }
            }
        }

        self.set_var(&name, ty);
    }

    /// Process `extern ns.fn(args) -> ReturnType` — a function signature declaration.
    fn process_extern_fn(&mut self, call_node: &SyntaxNode, return_type: Option<Type>, decl_node: &SyntaxNode) {
        let children: Vec<_> = non_trivia_children(call_node).collect();
        if children.is_empty() { return; }

        let lparen_idx = match children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::LParen)) {
            Some(i) => i,
            _ => return,
        };

        let name = match extract_dotted_name(&children[..lparen_idx]) {
            Some(n) => n,
            _ => return,
        };

        let rparen_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::RParen))
            .unwrap_or(children.len());
        let arg_tokens = &children[lparen_idx + 1..rparen_idx];
        let (args, rest) = extract_function_args(arg_tokens);

        let returns = return_type.unwrap_or(Type::None);
        let span = Self::span_of(decl_node);
        self.validate_type(&returns, &span);

        // Set variable type to the return type so hover on the name shows the function
        let returns_clone = returns.clone();
        if !name.contains('.') {
            self.set_var(&name, returns_clone);
        }

        // Record span for the function name for hover
        let call_children: Vec<_> = non_trivia_children(call_node).collect();
        let lparen = call_children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::LParen)).unwrap_or(0);
        for c in &call_children[..lparen] {
            if let Some(t) = c.as_token() {
                let range = t.text_range();
                self.span_types.push((range.start().into()..range.end().into(), returns.clone()));
            }
        }

        // Record spans for parameter names (may be bare idents or inside AssignExpr nodes)
        for c in &call_children[lparen+1..] {
            if as_token_kind(c) == Some(SyntaxKind::RParen) { break; }
            // Bare ident parameter
            if let Some(t) = c.as_token() {
                if t.kind() == SyntaxKind::Ident {
                    let param_name = t.text();
                    if let Some((_, param_ty)) = args.iter().find(|(n, _)| n == param_name) {
                        let range = t.text_range();
                        self.span_types.push((range.start().into()..range.end().into(), param_ty.clone()));
                    }
                }
            }
            // Type-annotated parameter: AssignExpr(ident, colon, type)
            if let Some(n) = c.as_node() {
                if n.kind() == SyntaxKind::AssignExpr {
                    if let Some(first_tok) = n.children_with_tokens()
                        .find_map(|c| c.into_token())
                    {
                        if first_tok.kind() == SyntaxKind::Ident {
                            let param_name = first_tok.text();
                            if let Some((_, param_ty)) = args.iter().find(|(n, _)| n == param_name) {
                                let range = first_tok.text_range();
                                self.span_types.push((range.start().into()..range.end().into(), param_ty.clone()));
                            }
                        }
                    }
                }
            }
        }

        self.inline_functions.insert(name, FunctionSig { args, rest, returns, doc: None });
    }

    /// Check if a type contains `some` (directly or in a union).
    fn type_contains_some(&self, ty: &Type) -> bool {
        match ty {
            Type::Some => true,
            Type::Union(types) | Type::Intersection(types) => {
                types.iter().any(|t| self.type_contains_some(t))
            }
            Type::Ref(name) => {
                self.schema.type_aliases.get(name)
                    .or_else(|| self.inline_aliases.get(name))
                    .map_or(false, |t| self.type_contains_some(t))
            }
            _ => false,
        }
    }

    /// Check if a write to a dotted path is allowed.
    /// Emit warnings for variables that were assigned but never read.
    fn check_unused_vars(&mut self) {
        for (name, span) in &self.var_assignments {
            if !self.var_reads.contains(name) {
                self.diagnostics.push(Diagnostic {
                    kind: DiagnosticKind::Warning,
                    span: span.clone(),
                    message: format!("variable '{}' is assigned but never used", name),
                });
            }
        }
    }

    /// Find the closest matching field name for "did you mean" suggestions.
    fn suggest_field(key: &str, fields: &[(String, Type)]) -> Option<String> {
        let mut best = None;
        let mut best_dist = usize::MAX;
        for (name, _) in fields {
            let dist = Self::edit_distance(key, name);
            if dist < best_dist && dist <= 2 {
                best_dist = dist;
                best = Some(name.clone());
            }
        }
        best
    }

    /// Simple Levenshtein edit distance.
    fn edit_distance(a: &str, b: &str) -> usize {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
        for i in 0..=a.len() { dp[i][0] = i; }
        for j in 0..=b.len() { dp[0][j] = j; }
        for i in 1..=a.len() {
            for j in 1..=b.len() {
                let cost = if a[i-1] == b[j-1] { 0 } else { 1 };
                dp[i][j] = (dp[i-1][j] + 1)
                    .min(dp[i][j-1] + 1)
                    .min(dp[i-1][j-1] + cost);
            }
        }
        dp[a.len()][b.len()]
    }

    fn check_mutability(&mut self, dotted_name: &str, span: std::ops::Range<usize>) {
        let parts: Vec<&str> = dotted_name.splitn(2, '.').collect();
        if parts.len() < 2 { return; }
        let root = parts[0];

        // Check if the root global exists and if writes are allowed
        if let Some(entry) = self.schema.globals.get(root) {
            if entry.mutable {
                return; // entire binding is mut — all writes allowed
            }
            // Check per-field mutability by looking at the type
            // For now, we check if the field path has a `mut` annotation
            // by looking at the mutability_map stored during .rexd parsing
            let field_path = parts[1];
            if !self.is_field_mutable(root, field_path) {
                self.error(span, format!(
                    "cannot assign to read-only property '{}' on '{}'",
                    field_path, root
                ));
            }
        }
    }

    /// Check if a field path on a global is mutable.
    fn is_field_mutable(&self, root: &str, field: &str) -> bool {
        if let Some(entry) = self.schema.globals.get(root) {
            if entry.mutable { return true; }
            // Check per-field mutability
            let first_part = field.split('.').next().unwrap_or(field);
            // Field is mutable if explicitly listed, or wildcard * allows all
            entry.mutable_fields.contains(&first_part.to_string())
                || entry.mutable_fields.contains(&"*".to_string())
        } else {
            // Unknown global — assume mutable (local variable)
            true
        }
    }

    fn infer_call(&mut self, node: &SyntaxNode) -> Type {
        let children: Vec<_> = non_trivia_children(node).collect();

        // Find LParen to split callee and args
        let lparen_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::LParen));
        let lparen_idx = match lparen_idx { Some(i) => i, None => return Type::Some };

        // Extract function name
        let callee_parts = &children[..lparen_idx];
        let func_name = extract_dotted_name(callee_parts);

        // Check if callee is a type predicate (isString, isNumber, etc.)
        if callee_parts.len() == 1 {
            if let Some(text) = as_token_text(&callee_parts[0]) {
                let pred_type = match text {
                    "isString" => Some(Type::Str),
                    "isNumber" => Some(Type::Num),
                    "isInteger" => Some(Type::Int),
                    "isBoolean" => Some(Type::Bool),
                    "isObject" | "isArray" => Some(Type::Some),
                    _ => None,
                };
                if let Some(ty) = pred_type {
                    let rparen = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::RParen)).unwrap_or(children.len());
                    for child in &children[lparen_idx + 1..rparen] {
                        if as_token_kind(child) != Some(SyntaxKind::Comma) {
                            self.infer_child(child);
                        }
                    }
                    return Type::Union(vec![ty, Type::None]).simplify();
                }
            }
        }

        // Infer arg types
        let rparen_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::RParen))
            .unwrap_or(children.len());
        let mut arg_types = Vec::new();
        for child in &children[lparen_idx + 1..rparen_idx] {
            if as_token_kind(child) != Some(SyntaxKind::Comma) {
                arg_types.push(self.infer_child(child));
            }
        }

        // Check schema for function signature (schema + inline)
        if let Some(name) = &func_name {
            let sig = self.schema.functions.get(name.as_str())
                .or_else(|| self.inline_functions.get(name.as_str()))
                .cloned();
            if let Some(sig) = &sig {
                // Check arg count
                if arg_types.len() != sig.args.len() && sig.rest.is_none() {
                    self.error(Self::span_of(node), format!(
                        "{} expects {} argument{}, got {}",
                        name, sig.args.len(),
                        if sig.args.len() == 1 { "" } else { "s" },
                        arg_types.len()
                    ));
                }
                // Check arg types
                for (i, (arg_name, expected)) in sig.args.iter().enumerate() {
                    if let Some(actual) = arg_types.get(i) {
                        let expected = self.resolve_type(expected);
                        let actual = self.resolve_type(actual);
                        if !actual.is_assignable_to(&expected) {
                            self.error(Self::span_of(node), format!(
                                "expected {} for argument '{}' of {}, got {}",
                                expected.display(), arg_name, name, actual.display()
                            ));
                        }
                    }
                }
                return self.resolve_type(&sig.returns);
            }
        }

        // Check for built-in method call: target.method(args)
        if callee_parts.len() == 1 {
            if let Some(nav_node) = callee_parts[0].as_node() {
                if nav_node.kind() == SyntaxKind::NavExpr {
                    // Infer the full NavExpr so all children get span entries
                    self.infer_node(nav_node);
                    let nav_children: Vec<_> = non_trivia_children(nav_node).collect();
                    if nav_children.len() >= 3 {
                        let target_type = self.infer_child(&nav_children[0]);
                        let method_name = nav_children.last()
                            .and_then(|c| c.as_token())
                            .map(|t| t.text().to_string());
                        if let Some(method) = method_name {
                            if let Some(ret) = builtin_method_type(&target_type, &method, &arg_types) {
                                return ret;
                            }
                        }
                    }
                }
            }
        }

        // Infer callee for span types (hover) — after all known checks
        for c in callee_parts { self.infer_child(c); }

        // Unknown function — if it's a navigation call (user.name), resolve property
        if let Some(name) = func_name {
            if let Some(var_type) = self.lookup_var(&name) {
                return var_type;
            }
        }

        Type::Some
    }

    fn infer_nav(&mut self, node: &SyntaxNode) -> Type {
        let children: Vec<_> = non_trivia_children(node).collect();
        if children.len() < 3 { return Type::unknown(); }

        let base_type = self.infer_child(&children[0]);
        let base_type = self.resolve_type(&base_type);

        // Check if this is dynamic navigation .(expr) vs static .key
        let is_dynamic = children[1].as_token()
            .map_or(false, |t| t.kind() == SyntaxKind::DotParen);

        if is_dynamic {
            // Dynamic navigation .(expr) — infer the key expression to track
            // variable reads, even though we can't resolve the property statically
            for c in &children[2..] {
                match c {
                    rowan::NodeOrToken::Node(n) => { self.infer_node(n); }
                    rowan::NodeOrToken::Token(t) if !matches!(t.kind(), SyntaxKind::RParen) => {
                        self.infer_token(t);
                    }
                    _ => {}
                }
            }
            return base_type.resolve_property("*").into_type();
        }

        // Static navigation .key
        let key = match &children[2] {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Ident => t.text().to_string(),
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::DecimalNumber => t.text().to_string(),
            rowan::NodeOrToken::Token(t) if t.kind().is_keyword() => t.text().to_string(),
            _ => return Type::unknown(),
        };

        // Check built-in methods before property lookup
        if let Some(ret) = builtin_method_type(&base_type, &key, &[]) {
            return ret;
        }

        match base_type.resolve_property(&key) {
            PropertyResult::Known(ty) => self.resolve_type(&ty),
            PropertyResult::Wildcard(ty) => self.resolve_type(&ty),
            PropertyResult::Unknown => {
                let suggestion = match &base_type {
                    Type::Object { fields, .. } => Self::suggest_field(&key, fields),
                    _ => None,
                };
                let msg = if let Some(suggested) = suggestion {
                    format!("unknown property '{}' on {}. Did you mean '{}'?", key, base_type.display(), suggested)
                } else {
                    format!("unknown property '{}' on {}", key, base_type.display())
                };
                self.warning(Self::span_of(node), msg);
                Type::None
            }
            PropertyResult::UnknownInBranch(ty) => {
                self.warning(Self::span_of(node), format!(
                    "unknown property '{}' on some branches of {}", key, base_type.display()
                ));
                self.resolve_type(&ty)
            }
        }
    }

    fn infer_group(&mut self, node: &SyntaxNode) -> Type {
        for child in node.children_with_tokens() {
            match &child {
                rowan::NodeOrToken::Node(n) => return self.infer_node(n),
                rowan::NodeOrToken::Token(t) if !t.kind().is_trivia()
                    && t.kind() != SyntaxKind::LParen
                    && t.kind() != SyntaxKind::RParen => {
                    return self.infer_token(t);
                }
                _ => {}
            }
        }
        Type::unknown()
    }

    fn infer_conditional(&mut self, node: &SyntaxNode) -> Type {
        let children: Vec<_> = non_trivia_children(node).collect();
        if children.is_empty() { return Type::None; }

        // First token is `when` or `unless`
        let is_unless = as_token_kind(&children[0]) == Some(SyntaxKind::KwUnless);

        // Find `do` to separate condition from body
        let do_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::KwDo));
        let do_idx = match do_idx { Some(i) => i, None => return Type::None };

        // Infer condition and extract narrowing info
        let cond_children = &children[1..do_idx];
        for child in cond_children {
            self.infer_child(child);
        }
        let narrowings = self.extract_narrowings(cond_children);

        // Infer then block with narrowing applied
        let mut then_type = Type::None;
        let mut else_type: Option<Type> = None;

        for child in &children[do_idx + 1..] {
            if as_token_kind(child) == Some(SyntaxKind::KwEnd) { break; }
            if let Some(n) = as_node(child) {
                if n.kind() == SyntaxKind::Block {
                    self.push_scope();
                    // Apply narrowing: when → apply, unless → apply inverse
                    if is_unless {
                        self.apply_narrowings_inverse(&narrowings);
                    } else {
                        self.apply_narrowings(&narrowings);
                    }
                    then_type = self.infer_block(n);
                    self.pop_scope();
                } else if n.kind() == SyntaxKind::ElseBranch {
                    self.push_scope();
                    // Else branch gets inverse narrowing
                    if is_unless {
                        self.apply_narrowings(&narrowings);
                    } else {
                        self.apply_narrowings_inverse(&narrowings);
                    }
                    else_type = Some(self.infer_else_branch(n));
                    self.pop_scope();
                }
            }
        }

        match else_type {
            Some(et) => Type::Union(vec![then_type, et]).simplify(),
            None => then_type.add_none(),
        }
    }

    fn infer_else_branch(&mut self, node: &SyntaxNode) -> Type {
        for child in node.children() {
            match child.kind() {
                SyntaxKind::Block => return self.infer_block(&child),
                // else when ... — nested conditional inside the else branch
                _ => {
                    // The else branch contains the when/unless keywords and the condition
                    // inline (not wrapped in a ConditionalExpr). Find the Block.
                    if child.kind() == SyntaxKind::ElseBranch {
                        return self.infer_else_branch(&child);
                    }
                }
            }
        }
        // else when ... with inline block
        let mut last = Type::None;
        for child in node.children() {
            if child.kind() == SyntaxKind::Block {
                last = self.infer_block(&child);
            }
        }
        last
    }

    // ── Narrowing ──────────────────────────────────────────────────────

    /// Extract narrowing information from a condition expression.
    fn extract_narrowings(
        &self,
        cond_children: &[rowan::NodeOrToken<SyntaxNode, SyntaxToken>],
    ) -> Vec<Narrowing> {
        let mut narrowings = Vec::new();

        if cond_children.is_empty() { return narrowings; }

        // Single child — could be a variable, call, comparison, assignment, or binary
        if cond_children.len() == 1 {
            self.extract_narrowing_from_child(&cond_children[0], &mut narrowings);
        }

        narrowings
    }

    fn extract_narrowing_from_child(
        &self,
        child: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>,
        narrowings: &mut Vec<Narrowing>,
    ) {
        match child {
            // Bare variable: `when x do` → x exists
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Ident => {
                narrowings.push(Narrowing::Exists(t.text().to_string()));
            }
            rowan::NodeOrToken::Node(n) => {
                match n.kind() {
                    // Call: `when isNumber(x) do` → type predicate
                    SyntaxKind::CallExpr => {
                        if let Some((predicate_type, var_name)) = self.extract_type_predicate(n) {
                            narrowings.push(Narrowing::TypePredicate(var_name, predicate_type));
                        }
                    }
                    // Binary: `when x == "GET" do` → equality narrowing
                    // Also: `when x and y do` → both exist
                    SyntaxKind::BinaryExpr => {
                        let children: Vec<_> = non_trivia_children(n).collect();
                        if children.len() >= 3 {
                            let op = as_token_kind(&children[1]);
                            match op {
                                Some(SyntaxKind::EqEq) => {
                                    // x == literal → narrow x to literal type
                                    if let Some(name) = self.child_as_var_name(&children[0]) {
                                        let rhs_type = self.child_as_literal_type(&children[2]);
                                        if let Some(ty) = rhs_type {
                                            narrowings.push(Narrowing::Equals(name, ty));
                                        }
                                    }
                                }
                                Some(SyntaxKind::KwAnd) => {
                                    // a and b → both narrow
                                    self.extract_narrowing_from_child(&children[0], narrowings);
                                    self.extract_narrowing_from_child(&children[2], narrowings);
                                }
                                _ => {
                                    // Other comparisons: `x > 10` → x exists
                                    if let Some(name) = self.child_as_var_name(&children[0]) {
                                        narrowings.push(Narrowing::Exists(name));
                                    }
                                }
                            }
                        }
                    }
                    // Assignment in condition: `when x = get-data() do`
                    SyntaxKind::AssignExpr => {
                        let children: Vec<_> = non_trivia_children(n).collect();
                        let eq_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::Eq));
                        if eq_idx.is_some() {
                            if let Some(name) = self.child_as_var_name(&children[0]) {
                                // The assigned type was already inferred when we inferred the condition
                                if let Some(ty) = self.lookup_var(&name) {
                                    narrowings.push(Narrowing::Assigned(name, ty));
                                }
                            }
                        }
                    }
                    // Navigation: `when user.name do` → user.name exists
                    SyntaxKind::NavExpr => {
                        let name = collect_nav_name(n);
                        narrowings.push(Narrowing::Exists(name));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Extract type predicate info from a CallExpr like `isNumber(x)`.
    fn extract_type_predicate(&self, call_node: &SyntaxNode) -> Option<(Type, String)> {
        let children: Vec<_> = non_trivia_children(call_node).collect();
        let lparen_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::LParen))?;

        // Check if callee is a type predicate function
        if lparen_idx != 1 { return None; }
        let callee_text = match &children[0] {
            rowan::NodeOrToken::Token(t) => t.text(),
            _ => return None,
        };
        let predicate_type = match callee_text {
            "isString" => Type::Str,
            "isNumber" => Type::Num,
            "isInteger" => Type::Int,
            "isBoolean" => Type::Bool,
            "isObject" => Type::Some, // TODO: more specific object type
            "isArray" => Type::Some,   // TODO: more specific array type
            _ => return None,
        };

        // Get the variable name from the first arg
        let rparen_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::RParen))
            .unwrap_or(children.len());
        for child in &children[lparen_idx + 1..rparen_idx] {
            if let Some(name) = self.child_as_var_name(child) {
                return Some((predicate_type, name));
            }
        }
        None
    }

    fn child_as_var_name(&self, child: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>) -> Option<String> {
        match child {
            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Ident => {
                Some(t.text().to_string())
            }
            rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::NavExpr => {
                Some(collect_nav_name(n))
            }
            _ => None,
        }
    }

    fn child_as_literal_type(&self, child: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>) -> Option<Type> {
        match child {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::DoubleString | SyntaxKind::SingleString => {
                    let text = t.text();
                    let inner = &text[1..text.len() - 1];
                    Some(Type::LiteralStr(inner.to_string()))
                }
                SyntaxKind::KwTrue | SyntaxKind::KwFalse => Some(Type::Bool),
                SyntaxKind::KwNull => Some(Type::Null),
                _ => None,
            }
            _ => None,
        }
    }

    /// Apply narrowings to the current scope (for the then-branch).
    fn apply_narrowings(&mut self, narrowings: &[Narrowing]) {
        for n in narrowings {
            match n {
                Narrowing::Exists(name) => {
                    if let Some(ty) = self.lookup_var(name) {
                        self.set_var(name, ty.remove_none());
                    }
                }
                Narrowing::TypePredicate(name, predicate_type) => {
                    self.set_var(name, predicate_type.clone());
                }
                Narrowing::Equals(name, literal_type) => {
                    self.set_var(name, literal_type.clone());
                }
                Narrowing::Assigned(name, ty) => {
                    // Variable was assigned in condition — it exists with that type
                    self.set_var(name, ty.remove_none());
                }
            }
        }
    }

    /// Apply inverse narrowings to the current scope (for the else-branch).
    fn apply_narrowings_inverse(&mut self, narrowings: &[Narrowing]) {
        for n in narrowings {
            match n {
                Narrowing::Exists(name) => {
                    // In else branch, the variable is none
                    self.set_var(name, Type::None);
                }
                Narrowing::TypePredicate(_name, _) => {
                    // In else branch, the variable is NOT that type
                    // For simplicity, keep the original type minus the predicate type
                    // (full implementation would subtract the predicate type from the union)
                }
                Narrowing::Equals(_name, _) => {
                    // In else branch, the variable is not equal to the literal
                    // Keep original type (can't narrow further without more info)
                }
                Narrowing::Assigned(name, _) => {
                    // Assignment didn't produce a defined value — variable is none
                    self.set_var(name, Type::None);
                }
            }
        }
    }

    fn infer_block(&mut self, node: &SyntaxNode) -> Type {
        let mut last = Type::None;
        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    last = self.infer_node(&n);
                    // After processing a conditional, apply flow narrowing for subsequent code
                    if n.kind() == SyntaxKind::ConditionalExpr {
                        self.apply_flow_narrowing(&n);
                    }
                }
                rowan::NodeOrToken::Token(t) if !t.kind().is_trivia() => last = self.infer_token(&t),
                _ => {}
            }
        }
        last
    }

    /// If a conditional always returns/breaks in its body, apply inverse narrowing
    /// to the current scope for subsequent statements.
    fn apply_flow_narrowing(&mut self, node: &SyntaxNode) {
        let children: Vec<_> = non_trivia_children(node).collect();
        if children.is_empty() { return; }

        let is_when = as_token_kind(&children[0]) == Some(SyntaxKind::KwWhen);
        let is_unless = as_token_kind(&children[0]) == Some(SyntaxKind::KwUnless);
        if !is_when && !is_unless { return; }

        // Find `do` to get the condition
        let do_idx = children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::KwDo));
        let do_idx = match do_idx { Some(i) => i, None => return };

        // Check if the body always exits (contains return or break)
        let has_else = children.iter().any(|c| as_node(c).map_or(false, |n| n.kind() == SyntaxKind::ElseBranch));
        if has_else { return; } // if there's an else, flow continues regardless

        let body_always_exits = children[do_idx + 1..].iter().any(|c| {
            if let Some(n) = as_node(c) {
                if n.kind() == SyntaxKind::Block {
                    return self.block_always_exits(n);
                }
            }
            false
        });

        if !body_always_exits { return; }

        // The body always exits → after this statement, the condition's inverse holds
        let cond_children = &children[1..do_idx];
        let narrowings = self.extract_narrowings(cond_children);

        if is_when {
            // `when cond do return end` → after this, cond is false → apply inverse
            self.apply_narrowings_inverse(&narrowings);
        } else {
            // `unless cond do return end` → after this, cond is true → apply normal
            self.apply_narrowings(&narrowings);
        }
    }

    /// Check if a block always exits (contains a return statement).
    fn block_always_exits(&self, node: &SyntaxNode) -> bool {
        for child in node.children() {
            match child.kind() {
                SyntaxKind::ReturnExpr => return true,
                _ => {}
            }
        }
        false
    }

    fn infer_for(&mut self, node: &SyntaxNode) -> Type {
        let mut iterable_type = Type::unknown();
        let mut binding_names: Vec<String> = Vec::new();
        let mut binding_spans: Vec<std::ops::Range<usize>> = Vec::new();
        let mut binding_declared_types: Vec<Option<Type>> = Vec::new();
        let mut is_of = false;
        let mut body_type = Type::None;

        for child in node.children_with_tokens() {
            match &child {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KwOf => {
                    is_of = true;
                }
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::IterBinding => {
                    // Extract binding names, optional type annotations, and iterable
                    let mut past_keyword = false;
                    let mut expect_type = false;
                    for bc in non_trivia_children(n) {
                        match &bc {
                            rowan::NodeOrToken::Token(t) if matches!(t.kind(),
                                SyntaxKind::KwIn | SyntaxKind::KwOf) => {
                                if t.kind() == SyntaxKind::KwOf { is_of = true; }
                                past_keyword = true;
                            }
                            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Comma => {}
                            rowan::NodeOrToken::Token(t) if !past_keyword && t.kind() == SyntaxKind::Colon => {
                                expect_type = true;
                            }
                            _ if expect_type && !past_keyword => {
                                // Type annotation after ':'
                                let declared = interpret_type_child(&bc);
                                let declared = self.resolve_type(&declared);
                                if let Some(last) = binding_declared_types.last_mut() {
                                    *last = Some(declared);
                                }
                                expect_type = false;
                            }
                            _ if !past_keyword => {
                                // Before in/of — binding name
                                if let rowan::NodeOrToken::Token(t) = &bc {
                                    if t.kind() == SyntaxKind::Ident {
                                        binding_names.push(t.text().to_string());
                                        let range = t.text_range();
                                        binding_spans.push(range.start().into()..range.end().into());
                                        binding_declared_types.push(None);
                                    }
                                }
                            }
                            _ => {
                                // After in/of — iterable expression
                                iterable_type = self.infer_child(&bc);
                            }
                        }
                    }
                }
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::Block => {
                    self.push_scope();
                    // Set iteration variable types
                    let iterable = self.resolve_type(&iterable_type);
                    let is_object = matches!(&iterable, Type::Object { .. });
                    let elem_type = match &iterable {
                        Type::Array(elem) => (**elem).clone(),
                        Type::Object { fields, wildcard } => {
                            let mut types: Vec<Type> = fields.iter().map(|(_, t)| t.clone()).collect();
                            if let Some(w) = wildcard {
                                types.push((**w).clone());
                            }
                            if types.is_empty() {
                                Type::Some
                            } else {
                                Type::Union(types).simplify()
                            }
                        }
                        _ => Type::Some,
                    };
                    match binding_names.len() {
                        1 => {
                            let inferred = if is_of { Type::Str } else { elem_type };
                            let ty = binding_declared_types.first()
                                .and_then(|d| d.clone())
                                .unwrap_or(inferred);
                            self.set_var(&binding_names[0], ty.clone());
                            if let Some(span) = binding_spans.first() {
                                self.span_types.push((span.clone(), ty));
                            }
                        }
                        2 => {
                            let key_ty = binding_declared_types.first()
                                .and_then(|d| d.clone())
                                .unwrap_or(if is_object { Type::Str } else { Type::Int });
                            let val_ty = binding_declared_types.get(1)
                                .and_then(|d| d.clone())
                                .unwrap_or(elem_type);
                            self.set_var(&binding_names[0], key_ty.clone());
                            self.set_var(&binding_names[1], val_ty.clone());
                            if let Some(span) = binding_spans.first() {
                                self.span_types.push((span.clone(), key_ty));
                            }
                            if let Some(span) = binding_spans.get(1) {
                                self.span_types.push((span.clone(), val_ty));
                            }
                        }
                        _ => {}
                    }
                    body_type = self.infer_block(n);
                    self.pop_scope();
                }
                _ => {}
            }
        }
        body_type.add_none()
    }

    fn infer_while(&mut self, node: &SyntaxNode) -> Type {
        let mut body_type = Type::None;
        for child in node.children() {
            if child.kind() == SyntaxKind::Block {
                self.push_scope();
                body_type = self.infer_block(&child);
                self.pop_scope();
            }
        }
        body_type.add_none()
    }

    fn infer_array(&mut self, node: &SyntaxNode) -> Type {
        let mut elem_types = Vec::new();
        for child in node.children_with_tokens() {
            match &child {
                rowan::NodeOrToken::Token(t) if matches!(t.kind(),
                    SyntaxKind::LBracket | SyntaxKind::RBracket | SyntaxKind::Comma) => continue,
                rowan::NodeOrToken::Token(t) if t.kind().is_trivia() => continue,
                _ => {
                    elem_types.push(self.infer_child(&child));
                }
            }
        }
        if elem_types.is_empty() {
            Type::Array(Box::new(Type::unknown()))
        } else {
            // Unify element types
            let unified = Type::Union(elem_types).simplify();
            Type::Array(Box::new(unified))
        }
    }

    fn infer_array_comp(&mut self, node: &SyntaxNode) -> Type {
        // ArrayComprehension: [body_expr for binding in iterable]
        // Children: LBracket, body_expr, (KwFor), IterBinding, RBracket
        // Or: [body_expr for name in iterable]
        let children: Vec<_> = non_trivia_children(node).collect();

        // Skip brackets, find the body (first non-bracket child) and IterBinding
        let mut body_child = None;
        let mut iterable_type = Type::unknown();
        let mut binding_names: Vec<String> = Vec::new();
        let mut is_of = false;

        for child in &children {
            match child {
                rowan::NodeOrToken::Token(t) if matches!(t.kind(),
                    SyntaxKind::LBracket | SyntaxKind::RBracket) => continue,
                rowan::NodeOrToken::Token(t) if matches!(t.kind(),
                    SyntaxKind::KwFor | SyntaxKind::KwWhile) => continue,
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::IterBinding => {
                    let mut past_keyword = false;
                    for bc in non_trivia_children(n) {
                        match &bc {
                            rowan::NodeOrToken::Token(t) if matches!(t.kind(),
                                SyntaxKind::KwIn | SyntaxKind::KwOf) => {
                                if t.kind() == SyntaxKind::KwOf { is_of = true; }
                                past_keyword = true;
                            }
                            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Comma => {}
                            _ if !past_keyword => {
                                if let rowan::NodeOrToken::Token(t) = &bc {
                                    if t.kind() == SyntaxKind::Ident {
                                        binding_names.push(t.text().to_string());
                                    }
                                }
                            }
                            _ => {
                                iterable_type = self.infer_child(&bc);
                            }
                        }
                    }
                }
                _ => {
                    if body_child.is_none() {
                        body_child = Some(child);
                    }
                }
            }
        }

        // Set up scope with iteration variables
        self.push_scope();
        let iterable = self.resolve_type(&iterable_type);
        let elem_type = match &iterable {
            Type::Array(elem) => (**elem).clone(),
            _ => Type::Some,
        };
        match binding_names.len() {
            1 => {
                if is_of {
                    self.set_var(&binding_names[0], Type::Str);
                } else {
                    self.set_var(&binding_names[0], elem_type);
                }
            }
            2 => {
                self.set_var(&binding_names[0], Type::Int);
                self.set_var(&binding_names[1], elem_type);
            }
            _ => {}
        }

        // Infer body expression
        let body_type = if let Some(child) = body_child {
            self.infer_child(child)
        } else {
            Type::Some
        };
        self.pop_scope();

        // Comprehensions filter out none values, so strip none from element type
        let elem_type = strip_none(body_type);
        Type::Array(Box::new(elem_type))
    }

    fn infer_object(&mut self, node: &SyntaxNode) -> Type {
        let mut fields = Vec::new();
        for child in node.children() {
            if child.kind() == SyntaxKind::Pair {
                let pair_children: Vec<_> = non_trivia_children(&child).collect();
                let colon_idx = pair_children.iter().position(|c| as_token_kind(c) == Some(SyntaxKind::Colon));
                if let Some(ci) = colon_idx {
                    let key = extract_dotted_name(&pair_children[..ci]).unwrap_or_default();
                    let val_type = if ci + 1 < pair_children.len() {
                        self.infer_child(&pair_children[ci + 1])
                    } else {
                        Type::unknown()
                    };
                    // Record span for each key token → value type (for hover on object keys)
                    for key_child in &pair_children[..ci] {
                        if let rowan::NodeOrToken::Token(t) = key_child {
                            let range = t.text_range();
                            self.span_types.push((range.start().into()..range.end().into(), val_type.clone()));
                        }
                    }
                    fields.push((key, val_type));
                }
            }
        }
        Type::Object { fields, wildcard: None }
    }

    fn infer_object_comp(&mut self, node: &SyntaxNode) -> Type {
        // Same as array comprehension but returns {*: value_type}
        let children: Vec<_> = non_trivia_children(node).collect();

        let mut body_children = Vec::new();
        let mut iterable_type = Type::unknown();
        let mut binding_names: Vec<String> = Vec::new();
        let mut is_of = false;

        for child in &children {
            match child {
                rowan::NodeOrToken::Token(t) if matches!(t.kind(),
                    SyntaxKind::LBrace | SyntaxKind::RBrace |
                    SyntaxKind::KwFor | SyntaxKind::KwWhile) => continue,
                rowan::NodeOrToken::Node(n) if n.kind() == SyntaxKind::IterBinding => {
                    let mut past_keyword = false;
                    for bc in non_trivia_children(n) {
                        match &bc {
                            rowan::NodeOrToken::Token(t) if matches!(t.kind(),
                                SyntaxKind::KwIn | SyntaxKind::KwOf) => {
                                if t.kind() == SyntaxKind::KwOf { is_of = true; }
                                past_keyword = true;
                            }
                            rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::Comma => {}
                            _ if !past_keyword => {
                                if let rowan::NodeOrToken::Token(t) = &bc {
                                    if t.kind() == SyntaxKind::Ident {
                                        binding_names.push(t.text().to_string());
                                    }
                                }
                            }
                            _ => {
                                iterable_type = self.infer_child(&bc);
                            }
                        }
                    }
                }
                _ => {
                    body_children.push(child);
                }
            }
        }

        // Set up scope with iteration variables
        self.push_scope();
        let iterable = self.resolve_type(&iterable_type);
        let elem_type = match &iterable {
            Type::Array(elem) => (**elem).clone(),
            _ => Type::Some,
        };
        match binding_names.len() {
            1 => {
                if is_of {
                    self.set_var(&binding_names[0], Type::Str);
                } else {
                    self.set_var(&binding_names[0], elem_type);
                }
            }
            2 => {
                self.set_var(&binding_names[0], Type::Str);
                self.set_var(&binding_names[1], elem_type);
            }
            _ => {}
        }

        // Infer body — for object comprehensions, the body is a Pair (key: value)
        let mut val_type = Type::Some;
        for child in &body_children {
            if let rowan::NodeOrToken::Node(n) = child {
                if n.kind() == SyntaxKind::Pair {
                    // Visit all children of the pair to track variable reads
                    let pair_children: Vec<_> = non_trivia_children(n).collect();
                    let colon_idx = pair_children.iter()
                        .position(|c| as_token_kind(c) == Some(SyntaxKind::Colon));
                    if let Some(ci) = colon_idx {
                        // Infer key expression(s)
                        for kc in &pair_children[..ci] {
                            self.infer_child(kc);
                        }
                        // Infer value expression
                        if ci + 1 < pair_children.len() {
                            val_type = self.infer_child(&pair_children[ci + 1]);
                        }
                    }
                    continue;
                }
            }
            val_type = self.infer_child(child);
        }
        self.pop_scope();

        let val_type = strip_none(val_type);
        Type::Object { fields: vec![], wildcard: Some(Box::new(val_type)) }
    }

    fn infer_template(&mut self, node: &SyntaxNode) -> Type {
        // Infer child nodes and extract variable references from template tokens
        for child in node.children_with_tokens() {
            match &child {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::TemplateLiteral => {
                    // Extract variable names from ${...} interpolations
                    self.mark_template_vars(t.text());
                }
                rowan::NodeOrToken::Node(n) => { self.infer_node(n); }
                _ => {}
            }
        }
        Type::Str
    }

    /// Mark variables referenced inside `${...}` interpolations as read.
    fn mark_template_vars(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
                i += 2; // skip ${
                let start = i;
                let mut depth = 1u32;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 { i += 1; }
                }
                // Extract the expression between ${ and }
                let expr = &text[start..i];
                // Mark all identifier-like tokens as read
                for part in expr.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
                    if !part.is_empty() && part.chars().next().map_or(false, |c| c.is_alphabetic() || c == '_') {
                        self.var_reads.insert(part.to_string());
                    }
                }
                i += 1; // skip }
            } else if bytes[i] == b'\\' {
                i += 2; // skip escape
            } else {
                i += 1;
            }
        }
    }

    fn infer_return(&mut self, node: &SyntaxNode) -> Type {
        // Infer the return value if present
        for child in node.children_with_tokens() {
            match &child {
                rowan::NodeOrToken::Token(t) if t.kind() == SyntaxKind::KwReturn => continue,
                rowan::NodeOrToken::Token(t) if t.kind().is_trivia() => continue,
                _ => { self.infer_child(&child); }
            }
        }
        Type::Never
    }
}

impl PropertyResult {
    fn into_type(self) -> Type {
        match self {
            PropertyResult::Known(ty) | PropertyResult::Wildcard(ty) | PropertyResult::UnknownInBranch(ty) => ty,
            PropertyResult::Unknown => Type::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_type_alias_string() {
        let schema = parse_rexd("type Foo = str");
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
        let schema = parse_rexd("type Point = {x: int, y: int}");
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
        let schema = parse_rexd("type Headers = {*: str}");
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
        let schema = parse_rexd("type Names = [str]");
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
        let schema = parse_rexd("extern config: unknown");
        let g = schema.globals.get("config").unwrap();
        assert_eq!(g.ty, Type::unknown());
        assert!(!g.mutable);
    }

    #[test]
    fn parse_extern_mut() {
        let schema = parse_rexd("extern mut status: int");
        let g = schema.globals.get("status").unwrap();
        assert_eq!(g.ty, Type::Int);
        assert!(g.mutable);
    }

    #[test]
    fn parse_extern_object() {
        let schema = parse_rexd("extern req: {method: str, path: str}");
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
        let schema = parse_rexd("extern json.parse(text: str) -> some");
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
        let schema = parse_rexd("extern db.set(key: str, value: str) -> bool");
        let f = schema.functions.get("db.set").unwrap();
        assert_eq!(f.args.len(), 2);
        assert_eq!(f.args[0], ("key".into(), Type::Str));
        assert_eq!(f.args[1], ("value".into(), Type::Str));
        assert_eq!(f.returns, Type::Bool);
    }

    #[test]
    fn parse_doc_comments() {
        let schema = parse_rexd("// Parse a JSON str\nextern json.parse(text: str) -> some");
        let f = schema.functions.get("json.parse").unwrap();
        assert_eq!(f.doc.as_deref(), Some("Parse a JSON str"));
    }

    #[test]
    fn parse_doc_comments_multiline() {
        let schema = parse_rexd("// Line one\n// Line two\nextern config: unknown");
        let g = schema.globals.get("config").unwrap();
        assert_eq!(g.doc.as_deref(), Some("Line one\nLine two"));
    }

    #[test]
    fn blank_line_resets_doc() {
        let schema = parse_rexd("// Not attached\n\nextern config: unknown");
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

    // ── Assignability tests ───────────────────────────────────────────

    #[test]
    fn assignable_identity() {
        assert!(Type::Int.is_assignable_to(&Type::Int));
        assert!(Type::Str.is_assignable_to(&Type::Str));
    }

    #[test]
    fn assignable_int_to_number() {
        assert!(Type::Int.is_assignable_to(&Type::Num));
        assert!(!Type::Num.is_assignable_to(&Type::Int));
    }

    #[test]
    fn assignable_literal_str_to_str() {
        assert!(Type::LiteralStr("GET".into()).is_assignable_to(&Type::Str));
        assert!(!Type::Str.is_assignable_to(&Type::LiteralStr("GET".into())));
    }

    #[test]
    fn assignable_to_some() {
        assert!(Type::Int.is_assignable_to(&Type::Some));
        assert!(Type::Str.is_assignable_to(&Type::Some));
        assert!(!Type::None.is_assignable_to(&Type::Some));
    }

    #[test]
    fn assignable_to_unknown() {
        assert!(Type::Int.is_assignable_to(&Type::unknown()));
        assert!(Type::None.is_assignable_to(&Type::unknown()));
        assert!(Type::Some.is_assignable_to(&Type::unknown()));
    }

    #[test]
    fn assignable_never_to_anything() {
        assert!(Type::Never.is_assignable_to(&Type::Int));
        assert!(Type::Never.is_assignable_to(&Type::Str));
        assert!(Type::Never.is_assignable_to(&Type::None));
    }

    #[test]
    fn assignable_to_union() {
        let target = Type::Union(vec![Type::Str, Type::Int]);
        assert!(Type::Int.is_assignable_to(&target));
        assert!(Type::Str.is_assignable_to(&target));
        assert!(!Type::Bool.is_assignable_to(&target));
    }

    #[test]
    fn assignable_transitive() {
        // integer → number → some
        assert!(Type::Int.is_assignable_to(&Type::Some));
        // LiteralStr → string → some
        assert!(Type::LiteralStr("x".into()).is_assignable_to(&Type::Some));
    }

    #[test]
    fn assignable_object_structural() {
        let source = Type::Object {
            fields: vec![("a".into(), Type::Int), ("b".into(), Type::Str)],
            wildcard: None,
        };
        // Target with subset of fields
        let target = Type::Object {
            fields: vec![("a".into(), Type::Int)],
            wildcard: None,
        };
        assert!(source.is_assignable_to(&target));
    }

    #[test]
    fn assignable_object_to_map() {
        let source = Type::Object {
            fields: vec![("a".into(), Type::Int), ("b".into(), Type::Int)],
            wildcard: None,
        };
        // Rigid object with all-integer fields is assignable to integer map
        let target = Type::Object {
            fields: vec![],
            wildcard: Some(Box::new(Type::Int)),
        };
        assert!(source.is_assignable_to(&target));

        // But not if field types don't match the wildcard
        let source2 = Type::Object {
            fields: vec![("a".into(), Type::Int), ("b".into(), Type::Str)],
            wildcard: None,
        };
        assert!(!source2.is_assignable_to(&target));
    }

    // ── Property resolution tests ─────────────────────────────────────

    #[test]
    fn resolve_known_field() {
        let obj = Type::Object {
            fields: vec![("name".into(), Type::Str), ("age".into(), Type::Int)],
            wildcard: None,
        };
        assert_eq!(obj.resolve_property("name"), PropertyResult::Known(Type::Str));
        assert_eq!(obj.resolve_property("age"), PropertyResult::Known(Type::Int));
    }

    #[test]
    fn resolve_unknown_field() {
        let obj = Type::Object {
            fields: vec![("name".into(), Type::Str)],
            wildcard: None,
        };
        assert_eq!(obj.resolve_property("missing"), PropertyResult::Unknown);
    }

    #[test]
    fn resolve_wildcard_field() {
        let map = Type::Object {
            fields: vec![],
            wildcard: Some(Box::new(Type::Str)),
        };
        match map.resolve_property("anything") {
            PropertyResult::Wildcard(ty) => {
                // Should be string | none
                assert!(ty.contains_none());
            }
            other => panic!("expected Wildcard, got {other:?}"),
        }
    }

    #[test]
    fn resolve_known_field_over_wildcard() {
        let obj = Type::Object {
            fields: vec![("name".into(), Type::Str)],
            wildcard: Some(Box::new(Type::Int)),
        };
        // Known field returns exact type, not wildcard
        assert_eq!(obj.resolve_property("name"), PropertyResult::Known(Type::Str));
        // Unknown field falls through to wildcard
        match obj.resolve_property("other") {
            PropertyResult::Wildcard(ty) => assert!(ty.contains_none()),
            other => panic!("expected Wildcard, got {other:?}"),
        }
    }

    #[test]
    fn resolve_on_none() {
        assert_eq!(Type::None.resolve_property("x"), PropertyResult::Known(Type::None));
    }

    #[test]
    fn resolve_on_some() {
        let result = Type::Some.resolve_property("x");
        match result {
            PropertyResult::Known(ty) => {
                assert!(ty.contains_none());
            }
            other => panic!("expected Known(some | none), got {other:?}"),
        }
    }

    #[test]
    fn resolve_on_union() {
        // {a: number} | {*: string}
        let union = Type::Union(vec![
            Type::Object {
                fields: vec![("a".into(), Type::Num)],
                wildcard: None,
            },
            Type::Object {
                fields: vec![],
                wildcard: Some(Box::new(Type::Str)),
            },
        ]);
        // .a resolves on both branches: number from left, string|none from right
        match union.resolve_property("a") {
            PropertyResult::Known(ty) | PropertyResult::UnknownInBranch(ty) => {
                // Combined type should include number and string
                let display = ty.display();
                assert!(display.contains("num") || display.contains("str"),
                    "unexpected type: {display}");
            }
            other => panic!("expected Known or UnknownInBranch, got {other:?}"),
        }
    }

    #[test]
    fn resolve_array_size() {
        let arr = Type::Array(Box::new(Type::Str));
        assert_eq!(arr.resolve_property("size"), PropertyResult::Known(Type::Int));
    }

    // ── Simplify tests ────────────────────────────────────────────────

    #[test]
    fn simplify_nested_unions() {
        let ty = Type::Union(vec![
            Type::Union(vec![Type::Int, Type::Str]),
            Type::Bool,
        ]);
        let simplified = ty.simplify();
        match simplified {
            Type::Union(types) => {
                assert_eq!(types.len(), 3);
                assert!(types.contains(&Type::Int));
                assert!(types.contains(&Type::Str));
                assert!(types.contains(&Type::Bool));
            }
            _ => panic!("expected union, got {simplified:?}"),
        }
    }

    #[test]
    fn simplify_dedup() {
        let ty = Type::Union(vec![Type::Int, Type::Str, Type::Int]);
        let simplified = ty.simplify();
        match simplified {
            Type::Union(types) => assert_eq!(types.len(), 2),
            _ => panic!("expected union, got {simplified:?}"),
        }
    }

    #[test]
    fn simplify_single() {
        let ty = Type::Union(vec![Type::Int]);
        assert_eq!(ty.simplify(), Type::Int);
    }

    #[test]
    fn simplify_some_absorbs() {
        // some | string | number → some (some absorbs concrete types)
        let ty = Type::Union(vec![Type::Some, Type::Str, Type::Num]);
        let simplified = ty.simplify();
        assert_eq!(simplified, Type::Some);
    }

    #[test]
    fn simplify_some_keeps_none() {
        // some | none → some | none (unknown)
        let ty = Type::Union(vec![Type::Some, Type::None]);
        let simplified = ty.simplify();
        assert_eq!(simplified, Type::unknown());
    }

    // ── remove_none / add_none tests ──────────────────────────────────

    #[test]
    fn remove_none_from_union() {
        let ty = Type::Union(vec![Type::Str, Type::None]);
        assert_eq!(ty.remove_none(), Type::Str);
    }

    #[test]
    fn remove_none_from_bare_none() {
        assert_eq!(Type::None.remove_none(), Type::Never);
    }

    #[test]
    fn add_none_to_type() {
        let ty = Type::Str;
        let with_none = ty.add_none();
        assert!(with_none.contains_none());
    }

    #[test]
    fn add_none_idempotent() {
        let ty = Type::Union(vec![Type::Str, Type::None]);
        let with_none = ty.add_none();
        assert_eq!(with_none, ty);
    }

    // ── Inference tests ───────────────────────────────────────────────

    fn check(source: &str) -> Vec<Diagnostic> {
        check_source(source, &DomainSchema::default())
    }

    fn check_with(source: &str, rexd: &str) -> Vec<Diagnostic> {
        let schema = parse_rexd(rexd);
        check_source(source, &schema)
    }

    fn errors(diags: &[Diagnostic]) -> Vec<&Diagnostic> {
        diags.iter().filter(|d| d.kind == DiagnosticKind::Error).collect()
    }

    fn has_error(diags: &[Diagnostic], substring: &str) -> bool {
        diags.iter().any(|d| d.kind == DiagnosticKind::Error && d.message.contains(substring))
    }

    fn has_warning(diags: &[Diagnostic], substring: &str) -> bool {
        diags.iter().any(|d| d.kind == DiagnosticKind::Warning && d.message.contains(substring))
    }

    #[test]
    fn infer_integer_literal() {
        let diags = check("42");
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_string_concat() {
        let diags = check(r#""hello" + " world""#);
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_mixed_add_error() {
        let diags = check(r#""hello" + 1"#);
        assert!(has_error(&diags, "cannot add"));
    }

    #[test]
    fn infer_variable_assignment() {
        let diags = check("x = 42\nx + 1");
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_typed_assignment() {
        let diags = check("lookup: {*: int} = {a: 1, b: 2}");
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
    }

    #[test]
    fn infer_typed_assignment_mismatch() {
        let diags = check(r#"x: int = "hello""#);
        assert!(has_error(&diags, "not assignable"));
    }

    #[test]
    fn infer_comparison_type() {
        // Comparisons return lhs | none — y is unused (warning) but no errors
        let diags = check("x = 42\ny = x > 10");
        assert!(errors(&diags).is_empty(), "unexpected errors: {:?}", errors(&diags));
    }

    #[test]
    fn infer_when_else() {
        let diags = check("when 1 == 1 do 42 else 0 end");
        assert!(diags.is_empty());
    }

    #[test]
    #[test]
    fn infer_template_literal() {
        // x is used inside the template interpolation — no errors
        // Note: unused variable warning may appear since template interpolation
        // tracking is not yet implemented
        let diags = check(r#"x = 42
`the answer is ${x}`"#);
        assert!(errors(&diags).is_empty(), "unexpected errors: {:?}", errors(&diags));
    }

    #[test]
    fn infer_return_is_never() {
        let diags = check("return 42");
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_domain_global() {
        let diags = check_with(
            "method",
            r#"type HttpMethod = "GET" | "POST"
extern method: HttpMethod"#,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_domain_function_call() {
        let diags = check_with(
            r#"json.parse("hello")"#,
            "extern json.parse(text: str) -> some",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_domain_function_wrong_arg_type() {
        let diags = check_with(
            "json.parse(42)",
            "extern json.parse(text: str) -> some",
        );
        assert!(has_error(&diags, "expected str"));
    }

    #[test]
    fn infer_domain_function_wrong_arg_count() {
        let diags = check_with(
            r#"json.parse("a", "b")"#,
            "extern json.parse(text: str) -> some",
        );
        assert!(has_error(&diags, "expects 1 argument"));
    }

    #[test]
    fn infer_nav_on_object() {
        let diags = check_with(
            "req.method",
            "extern req: {method: str, path: str}",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_nav_unknown_property() {
        let diags = check_with(
            "req.headrs",
            "extern req: {method: str, headers: str}",
        );
        assert!(has_warning(&diags, "unknown property"));
    }

    #[test]
    fn infer_object_literal() {
        let diags = check("{a: 1, b: 2}");
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_array_literal() {
        let diags = check("[1, 2, 3]");
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_for_loop() {
        let diags = check("for v in [1, 2, 3] do v + 1 end");
        assert!(diags.is_empty());
    }

    #[test]
    fn infer_for_over_object() {
        // Iterating over an object with two bindings should give string keys
        // and the union of field value types.
        // v + 1 should not error — v should be inferred as Int, not Some
        let source = "obj = {a: 1, b: 2, c: 3}\nfor key, val in obj do\n  val + 1\nend";
        let diags = check(source);
        assert!(diags.is_empty(), "unexpected diagnostics: {:?}", diags);
    }

    #[test]
    fn infer_knowledge_base() {
        // Smoke test: parse all .rex files with the real domain schema.
        // Some false positives are expected until narrowing and iteration
        // variable types are implemented.
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/knowledge-base");
        let rexd = std::fs::read_to_string(base.join("rex-serve.rexd")).unwrap();
        let schema = parse_rexd(&rexd);

        fn visit_dir(dir: &std::path::Path, schema: &DomainSchema) -> usize {
            let mut count = 0;
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    count += visit_dir(&path, schema);
                } else if path.extension().map_or(false, |e| e == "rex") {
                    let source = std::fs::read_to_string(&path).unwrap();
                    let _diags = check_source(&source, schema);
                    count += 1;
                    // Don't assert zero errors yet — narrowing and loop variable
                    // types are not implemented, causing false positives.
                }
            }
            count
        }

        let count = visit_dir(&base.join("routes"), &schema);
        assert!(count > 0, "no .rex files found");
    }

    // ── Narrowing tests ───────────────────────────────────────────────

    #[test]
    fn narrow_existence() {
        // when x do → x has none removed
        let diags = check_with(
            "when name do\n  name + \" suffix\"\nend",
            "extern name: str | none",
        );
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
    }

    #[test]
    fn narrow_type_predicate() {
        // when isNumber(x) do → x is num
        let diags = check_with(
            "when isNumber(value) do\n  value + 1\nend",
            "extern value: unknown",
        );
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
    }

    #[test]
    fn narrow_equality() {
        // when method == "GET" do → method is "GET"
        let diags = check_with(
            r#"when method == "GET" do
  method
end"#,
            r#"type HttpMethod = "GET" | "POST"
extern method: HttpMethod"#,
        );
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
    }

    #[test]
    fn narrow_and_chain() {
        // when input and input.slug do → both exist
        let diags = check_with(
            "when input and input.slug do\n  input.slug + \"-suffix\"\nend",
            "extern input: {slug: str | none} | none",
        );
        // After narrowing: input is {slug: string | none} (none removed)
        // input.slug after `and` narrowing: string (none removed)
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
    }

    #[test]
    fn for_loop_variable_type() {
        // for v in items → v gets element type
        let diags = check_with(
            "for a in articles do\n  a.value + \"-suffix\"\nend",
            "type DbEntry = {key: str, value: str}\nextern articles: [DbEntry]",
        );
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
    }

    #[test]
    fn for_loop_key_value() {
        // for k, v in items → k is integer, v is element type
        let diags = check("for k, v in [1, 2, 3] do\n  k + v\nend");
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
    }

    // ── Intersection type tests ───────────────────────────────────────

    #[test]
    fn parse_intersection_type() {
        let schema = parse_rexd("type HeaderValue = str & [str]");
        let ty = schema.type_aliases.get("HeaderValue").unwrap();
        match ty {
            Type::Intersection(types) => {
                assert_eq!(types.len(), 2);
                assert!(types.contains(&Type::Str));
                assert!(types.contains(&Type::Array(Box::new(Type::Str))));
            }
            _ => panic!("expected intersection, got {ty:?}"),
        }
    }

    #[test]
    fn intersection_is_string() {
        let ty = Type::Intersection(vec![Type::Str, Type::Array(Box::new(Type::Str))]);
        assert!(ty.is_string());
    }

    #[test]
    fn intersection_string_concat() {
        // string & [string] can be used with +
        let diags = check_with(
            r#"h + "-suffix""#,
            "extern h: str & [str]",
        );
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
    }

    #[test]
    fn intersection_assignable_to_member() {
        // string & [string] is assignable to string
        let ty = Type::Intersection(vec![Type::Str, Type::Array(Box::new(Type::Str))]);
        assert!(ty.is_assignable_to(&Type::Str));
        assert!(ty.is_assignable_to(&Type::Array(Box::new(Type::Str))));
    }

    #[test]
    fn intersection_property_resolution() {
        // string & [string] — .size works (from both), .0 works (from both)
        let ty = Type::Intersection(vec![Type::Str, Type::Array(Box::new(Type::Str))]);
        match ty.resolve_property("size") {
            PropertyResult::Known(Type::Int) => {}
            other => panic!("expected Known(Int), got {other:?}"),
        }
    }

    #[test]
    fn intersection_in_map() {
        // {*: string & [string]} — map values implement both interfaces
        let diags = check_with(
            r#"headers.host + "/path""#,
            "extern headers: {*: str & [str]}",
        );
        assert!(diags.is_empty(), "unexpected errors: {:?}", diags);
    }

    #[test]
    fn intersection_display() {
        let ty = Type::Intersection(vec![Type::Str, Type::Array(Box::new(Type::Str))]);
        assert_eq!(ty.display(), "str & [str]");
    }

    // ── Unused variable tests ─────────────────────────────────────────

    #[test]
    fn warn_unused_variable() {
        let diags = check("x = 42");
        assert!(has_warning(&diags, "variable 'x' is assigned but never used"));
    }

    #[test]
    fn no_warn_used_variable() {
        let diags = check("x = 42\nx + 1");
        assert!(!has_warning(&diags, "unused"));
    }

    #[test]
    fn no_warn_compound_assignment() {
        // Compound assignment reads the variable
        let diags = check("x = 0\nx += 1\nx");
        assert!(!has_warning(&diags, "unused"));
    }

    // ── Did you mean tests ────────────────────────────────────────────

    #[test]
    fn suggest_similar_property() {
        let diags = check_with(
            "req.headrs",
            "extern req: {method: str, headers: str}",
        );
        assert!(has_warning(&diags, "Did you mean 'headers'"));
    }

    #[test]
    fn no_suggestion_for_unrelated() {
        let diags = check_with(
            "req.xyz",
            "extern req: {method: str, headers: str}",
        );
        assert!(has_warning(&diags, "unknown property"));
        assert!(!has_warning(&diags, "Did you mean"));
    }

    // ── Mutability tests ──────────────────────────────────────────────

    #[test]
    fn mut_field_write_allowed() {
        let diags = check_with(
            "res.status = 404",
            "extern res: {mut status: int, body: str}",
        );
        assert!(errors(&diags).is_empty(), "unexpected errors: {:?}", errors(&diags));
    }

    #[test]
    fn readonly_field_write_error() {
        let diags = check_with(
            "req.method = \"POST\"",
            "extern req: {method: str}",
        );
        assert!(has_error(&diags, "read-only"));
    }

    #[test]
    fn mut_binding_write_allowed() {
        let diags = check_with(
            "status = 404",
            "extern mut status: int",
        );
        assert!(errors(&diags).is_empty(), "unexpected errors: {:?}", errors(&diags));
    }
}
