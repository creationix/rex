//! Lower a rowan CST into bytecode [`Value`]s.
//!
//! Walks the untyped syntax tree and produces a `Value` tree suitable for
//! encoding to the bytecode format.

use crate::bytecode::Value;
use crate::syntax::{SyntaxKind, SyntaxNode};

/// Lower a parsed CST root into a bytecode `Value`.
///
/// A program with a single expression returns that expression's value.
/// A program with multiple expressions returns a `Block`.
pub fn lower(root: &SyntaxNode) -> Value {
    assert_eq!(root.kind(), SyntaxKind::Root);

    let exprs: Vec<Value> = root
        .children_with_tokens()
        .filter(|c| c.as_token().map_or(true, |t| !t.kind().is_trivia()))
        .filter_map(|child| lower_child(child))
        .collect();

    match exprs.len() {
        0 => Value::Ref("no".into()), // empty program → undefined
        1 => exprs.into_iter().next().unwrap(),
        _ => Value::Block(exprs),
    }
}

fn lower_node(node: &SyntaxNode) -> Option<Value> {
    match node.kind() {
        SyntaxKind::BinaryExpr => Some(lower_binary(node)),
        SyntaxKind::UnaryExpr => Some(lower_unary(node)),
        SyntaxKind::AssignExpr => Some(lower_assign(node)),
        SyntaxKind::RangeExpr => Some(lower_range(node)),
        SyntaxKind::CallExpr => Some(lower_call(node)),
        SyntaxKind::NavExpr => Some(lower_nav(node)),
        SyntaxKind::GroupExpr => lower_group(node),
        SyntaxKind::SelfExpr => Some(lower_self_expr(node)),
        SyntaxKind::ConditionalExpr => Some(lower_conditional(node)),
        SyntaxKind::ForExpr => Some(lower_for(node)),
        SyntaxKind::WhileExpr => Some(lower_while(node)),
        SyntaxKind::ArrayExpr => Some(lower_array(node)),
        SyntaxKind::ArrayComprehension => Some(lower_array_comprehension(node)),
        SyntaxKind::ObjectExpr => Some(lower_object(node)),
        SyntaxKind::ObjectComprehension => Some(lower_object_comprehension(node)),
        SyntaxKind::TemplateExpr => Some(lower_template_expr(node)),
        SyntaxKind::Error => None, // skip error nodes
        _ => None,
    }
}

/// Lower any child — could be a composite node or a bare token.
fn lower_child(child: rowan::NodeOrToken<SyntaxNode, crate::syntax::SyntaxToken>) -> Option<Value> {
    match child {
        rowan::NodeOrToken::Node(n) => lower_node(&n),
        rowan::NodeOrToken::Token(t) => lower_token(&t),
    }
}

fn lower_token(token: &crate::syntax::SyntaxToken) -> Option<Value> {
    match token.kind() {
        SyntaxKind::DecimalNumber => {
            let text = token.text();
            if let Some(value) = parse_decimal_number(text) {
                Some(value)
            } else {
                Some(Value::Integer(0))
            }
        }
        SyntaxKind::HexNumber => {
            let text = token.text().trim_start_matches('-');
            let neg = token.text().starts_with('-');
            let n = i64::from_str_radix(text.trim_start_matches("0x"), 16).unwrap_or(0);
            Some(Value::Integer(if neg { -n } else { n }))
        }
        SyntaxKind::BinaryNumber => {
            let text = token.text().trim_start_matches('-');
            let neg = token.text().starts_with('-');
            let n = i64::from_str_radix(text.trim_start_matches("0b"), 2).unwrap_or(0);
            Some(Value::Integer(if neg { -n } else { n }))
        }
        SyntaxKind::DoubleString => {
            let text = token.text();
            // Strip surrounding quotes
            let inner = &text[1..text.len() - 1];
            Some(Value::String(unescape(inner)))
        }
        SyntaxKind::SingleString => {
            let text = token.text();
            let inner = &text[1..text.len() - 1];
            Some(Value::String(unescape(inner)))
        }
        SyntaxKind::Ident => Some(Value::Variable(token.text().to_string())),
        SyntaxKind::KwTrue => Some(Value::Ref("t".into())),
        SyntaxKind::KwFalse => Some(Value::Ref("f".into())),
        SyntaxKind::KwNull => Some(Value::Ref("n".into())),
        SyntaxKind::KwNone => Some(Value::Ref("no".into())),
        SyntaxKind::KwNan => Some(Value::Ref("nan".into())),
        SyntaxKind::KwInf => Some(Value::Ref("inf".into())),
        SyntaxKind::KwSelf => Some(Value::SelfRef(0)),
        SyntaxKind::KwBreak => Some(Value::BreakCont(0)),
        SyntaxKind::KwContinue => Some(Value::BreakCont(1)),
        // Type predicates as opcodes
        SyntaxKind::KwString => Some(Value::Opcode("st".into())),
        SyntaxKind::KwNumber => Some(Value::Opcode("nm".into())),
        SyntaxKind::KwObject => Some(Value::Opcode("ob".into())),
        SyntaxKind::KwArray => Some(Value::Opcode("ar".into())),
        SyntaxKind::KwBoolean => Some(Value::Opcode("bt".into())),
        _ => None,
    }
}

fn parse_decimal_number(text: &str) -> Option<Value> {
    let neg = text.starts_with('-');
    let text = text.trim_start_matches('-');

    // Check for decimal point or exponent
    if text.contains('.') || text.contains('e') || text.contains('E') {
        // Parse as decimal: significand × 10^exponent
        let (int_part, frac_part, exp_part) = split_decimal(text);
        let sig_str = format!("{}{}", int_part, frac_part);
        let sig: i64 = sig_str.parse().unwrap_or(0);
        let exp = exp_part - frac_part.len() as i64;
        let sig = if neg { -sig } else { sig };
        Some(Value::Decimal { sig, exp })
    } else {
        let n: i64 = text.parse().ok()?;
        Some(Value::Integer(if neg { -n } else { n }))
    }
}

fn split_decimal(text: &str) -> (&str, &str, i64) {
    let (main, exp) = if let Some(pos) = text.find(['e', 'E']) {
        let exp_val: i64 = text[pos + 1..].parse().unwrap_or(0);
        (&text[..pos], exp_val)
    } else {
        (text, 0)
    };

    if let Some(dot) = main.find('.') {
        (&main[..dot], &main[dot + 1..], exp)
    } else {
        (main, "", exp)
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('\'') => out.push('\''),
                Some('/') => out.push('/'),
                Some('b') => out.push('\u{08}'),
                Some('f') => out.push('\u{0C}'),
                Some('0') => out.push('\0'),
                Some('x') => {
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(n) = u32::from_str_radix(&hex, 16) {
                        if let Some(c) = char::from_u32(n) {
                            out.push(c);
                        }
                    }
                }
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(n) = u32::from_str_radix(&hex, 16) {
                        if let Some(c) = char::from_u32(n) {
                            out.push(c);
                        }
                    }
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ── Expression lowering ─────────────────────────────────────────────────

fn non_trivia_children(
    node: &SyntaxNode,
) -> impl Iterator<Item = rowan::NodeOrToken<SyntaxNode, crate::syntax::SyntaxToken>> {
    node.children_with_tokens()
        .filter(|c| c.as_token().map_or(true, |t| !t.kind().is_trivia()))
}

/// Lower a Block node's children into a vec of values, or a single value.
fn lower_block_body(node: &SyntaxNode) -> Value {
    let items: Vec<Value> = non_trivia_children(node)
        .filter_map(|c| lower_child(c))
        .collect();
    match items.len() {
        0 => Value::Ref("no".into()),
        1 => items.into_iter().next().unwrap(),
        _ => Value::Block(items),
    }
}

fn lower_binary(node: &SyntaxNode) -> Value {
    let mut children = non_trivia_children(node);

    let lhs = children
        .next()
        .and_then(|c| lower_child(c))
        .unwrap_or(Value::Ref("no".into()));

    let op_token = children.next();
    let op = op_token
        .as_ref()
        .and_then(|c| c.as_token())
        .map(|t| t.kind());

    let rhs = children
        .next()
        .and_then(|c| lower_child(c))
        .unwrap_or(Value::Ref("no".into()));

    let opcode = match op {
        Some(SyntaxKind::Plus) => "ad",
        Some(SyntaxKind::Minus) => "sb",
        Some(SyntaxKind::Star) => "ml",
        Some(SyntaxKind::Slash) => "dv",
        Some(SyntaxKind::Percent) => "md",
        Some(SyntaxKind::Amp) => "an",
        Some(SyntaxKind::Pipe) => "or",
        Some(SyntaxKind::Caret) => "xr",
        Some(SyntaxKind::EqEq) => "eq",
        Some(SyntaxKind::BangEq) => "nq",
        Some(SyntaxKind::Gt) => "gt",
        Some(SyntaxKind::GtEq) => "ge",
        Some(SyntaxKind::Lt) => "lt",
        Some(SyntaxKind::LtEq) => "le",
        Some(SyntaxKind::KwAnd) => return Value::And(vec![lhs, rhs]),
        Some(SyntaxKind::KwOr) => return Value::Or(vec![lhs, rhs]),
        Some(SyntaxKind::KwNor) => return Value::Unless(vec![lhs, rhs]),
        _ => "ad", // fallback
    };

    Value::Call(vec![Value::Opcode(opcode.into()), lhs, rhs])
}

fn lower_unary(node: &SyntaxNode) -> Value {
    let mut children = non_trivia_children(node);

    let op_token = children.next();
    let op = op_token
        .as_ref()
        .and_then(|c| c.as_token())
        .map(|t| t.kind());

    let operand = children
        .next()
        .and_then(|c| lower_child(c))
        .unwrap_or(Value::Ref("no".into()));

    match op {
        Some(SyntaxKind::Minus) => {
            Value::Call(vec![Value::Opcode("ng".into()), operand])
        }
        Some(SyntaxKind::Tilde) => {
            Value::Call(vec![Value::Opcode("nt".into()), operand])
        }
        Some(SyntaxKind::KwNot) => {
            Value::Call(vec![Value::Opcode("nt".into()), operand])
        }
        Some(SyntaxKind::KwDelete) => Value::Delete(Box::new(operand)),
        _ => operand,
    }
}

fn lower_assign(node: &SyntaxNode) -> Value {
    let mut children = non_trivia_children(node);

    let place = children
        .next()
        .and_then(|c| lower_child(c))
        .unwrap_or(Value::Ref("no".into()));

    let op_token = children.next();
    let op = op_token
        .as_ref()
        .and_then(|c| c.as_token())
        .map(|t| t.kind());

    let value = children
        .next()
        .and_then(|c| lower_child(c))
        .unwrap_or(Value::Ref("no".into()));

    // Compound assignments desugar: x += e → x = add(x, e)
    let value = match op {
        Some(SyntaxKind::PlusEq) => {
            Value::Call(vec![Value::Opcode("ad".into()), place.clone(), value])
        }
        Some(SyntaxKind::MinusEq) => {
            Value::Call(vec![Value::Opcode("sb".into()), place.clone(), value])
        }
        Some(SyntaxKind::StarEq) => {
            Value::Call(vec![Value::Opcode("ml".into()), place.clone(), value])
        }
        Some(SyntaxKind::SlashEq) => {
            Value::Call(vec![Value::Opcode("dv".into()), place.clone(), value])
        }
        Some(SyntaxKind::PercentEq) => {
            Value::Call(vec![Value::Opcode("md".into()), place.clone(), value])
        }
        Some(SyntaxKind::AmpEq) => {
            Value::Call(vec![Value::Opcode("an".into()), place.clone(), value])
        }
        Some(SyntaxKind::PipeEq) => {
            Value::Call(vec![Value::Opcode("or".into()), place.clone(), value])
        }
        Some(SyntaxKind::CaretEq) => {
            Value::Call(vec![Value::Opcode("xr".into()), place.clone(), value])
        }
        Some(SyntaxKind::ColonEq) => return Value::Swap(Box::new(place), Box::new(value)),
        _ => value, // plain =
    };

    Value::Set(Box::new(place), Box::new(value))
}

fn lower_range(node: &SyntaxNode) -> Value {
    let mut children = non_trivia_children(node);

    let from = children
        .next()
        .and_then(|c| lower_child(c))
        .unwrap_or(Value::Integer(0));

    // skip DotDot token
    children.next();

    let to = children
        .next()
        .and_then(|c| lower_child(c))
        .unwrap_or(Value::Integer(0));

    Value::Call(vec![Value::Opcode("rn".into()), from, to])
}

fn lower_call(node: &SyntaxNode) -> Value {
    let mut items = Vec::new();
    for child in non_trivia_children(node) {
        // Skip parens and commas
        if let Some(t) = child.as_token() {
            match t.kind() {
                SyntaxKind::LParen | SyntaxKind::RParen | SyntaxKind::Comma => continue,
                _ => {}
            }
        }
        if let Some(v) = lower_child(child) {
            items.push(v);
        }
    }
    Value::Call(items)
}

fn lower_nav(node: &SyntaxNode) -> Value {
    // NavExpr: base . key | base .( expr )
    // If base is a NavExpr, flatten into a single Call.
    // The first child is the base (Variable if ident), subsequent idents
    // after Dot are nav keys (String).
    let mut items = Vec::new();
    let mut seen_dot = false;

    for child in non_trivia_children(node) {
        match child {
            rowan::NodeOrToken::Node(n) => {
                if n.kind() == SyntaxKind::NavExpr {
                    let inner = lower_nav(&n);
                    if let Value::Call(inner_items) = inner {
                        items.extend(inner_items);
                    } else {
                        items.push(inner);
                    }
                } else if let Some(v) = lower_node(&n) {
                    items.push(v);
                }
            }
            rowan::NodeOrToken::Token(t) => {
                match t.kind() {
                    SyntaxKind::Dot | SyntaxKind::DotParen => {
                        seen_dot = true;
                    }
                    SyntaxKind::RParen => {}
                    SyntaxKind::Ident if !seen_dot => {
                        // Base identifier → Variable
                        items.push(Value::Variable(t.text().to_string()));
                    }
                    SyntaxKind::Ident => {
                        // Nav key after dot → String
                        items.push(Value::String(t.text().to_string()));
                        seen_dot = false;
                    }
                    SyntaxKind::DecimalNumber if seen_dot => {
                        items.push(Value::String(t.text().to_string()));
                        seen_dot = false;
                    }
                    _ => {
                        if let Some(v) = lower_token(&t) {
                            items.push(v);
                        }
                    }
                }
            }
        }
    }

    Value::Call(items)
}

fn lower_group(node: &SyntaxNode) -> Option<Value> {
    // GroupExpr: ( expr ) — just unwrap
    non_trivia_children(node)
        .find_map(|child| lower_child(child))
}

fn lower_self_expr(node: &SyntaxNode) -> Value {
    // SelfExpr: self @ number
    let mut depth: u32 = 0;
    for child in non_trivia_children(node) {
        if let Some(t) = child.as_token() {
            if t.kind() == SyntaxKind::DecimalNumber {
                depth = t.text().parse().unwrap_or(0);
            }
        }
    }
    Value::SelfRef(depth)
}

fn lower_conditional(node: &SyntaxNode) -> Value {
    let mut items = Vec::new();
    let mut is_unless = false;

    for child in non_trivia_children(node) {
        match child {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::KwUnless => is_unless = true,
                SyntaxKind::KwWhen | SyntaxKind::KwDo | SyntaxKind::KwEnd => {}
                _ => {
                    if let Some(v) = lower_token(&t) {
                        items.push(v);
                    }
                }
            },
            rowan::NodeOrToken::Node(n) => match n.kind() {
                SyntaxKind::Block => {
                    items.push(lower_block_body(&n));
                }
                SyntaxKind::ElseBranch => {
                    items.push(lower_else_branch(&n));
                }
                _ => {
                    if let Some(v) = lower_node(&n) {
                        items.push(v);
                    }
                }
            },
        }
    }

    if is_unless {
        Value::Unless(items)
    } else {
        Value::When(items)
    }
}

fn lower_else_branch(node: &SyntaxNode) -> Value {
    // ElseBranch can contain: else block, or else when/unless ... (nested conditional)
    let mut has_when = false;
    let mut is_unless = false;
    let mut items = Vec::new();

    for child in non_trivia_children(node) {
        match child {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::KwElse => {}
                SyntaxKind::KwWhen => has_when = true,
                SyntaxKind::KwUnless => {
                    has_when = true;
                    is_unless = true;
                }
                SyntaxKind::KwDo => {}
                _ => {
                    if let Some(v) = lower_token(&t) {
                        items.push(v);
                    }
                }
            },
            rowan::NodeOrToken::Node(n) => match n.kind() {
                SyntaxKind::Block => {
                    items.push(lower_block_body(&n));
                }
                SyntaxKind::ElseBranch => {
                    items.push(lower_else_branch(&n));
                }
                _ => {
                    if let Some(v) = lower_node(&n) {
                        items.push(v);
                    }
                }
            },
        }
    }

    if has_when {
        if is_unless {
            Value::Unless(items)
        } else {
            Value::When(items)
        }
    } else {
        // Plain else block
        match items.len() {
            0 => Value::Ref("no".into()),
            1 => items.into_iter().next().unwrap(),
            _ => Value::Block(items),
        }
    }
}

fn lower_for(node: &SyntaxNode) -> Value {
    let mut items = Vec::new();
    let mut is_of = false;

    for child in non_trivia_children(node) {
        match child {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::KwFor | SyntaxKind::KwDo | SyntaxKind::KwEnd => {}
                _ => {}
            },
            rowan::NodeOrToken::Node(n) => match n.kind() {
                SyntaxKind::IterBinding => {
                    let (binding_items, binding_is_of) = lower_iter_binding(&n);
                    is_of = binding_is_of;
                    items.extend(binding_items);
                }
                SyntaxKind::Block => {
                    items.push(lower_block_body(&n));
                }
                _ => {
                    if let Some(v) = lower_node(&n) {
                        items.push(v);
                    }
                }
            },
        }
    }

    if is_of {
        Value::ForOf(items)
    } else {
        Value::ForIn(items)
    }
}

fn lower_while(node: &SyntaxNode) -> Value {
    let mut items = Vec::new();

    for child in non_trivia_children(node) {
        match child {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::KwWhile | SyntaxKind::KwDo | SyntaxKind::KwEnd => {}
                _ => {
                    if let Some(v) = lower_token(&t) {
                        items.push(v);
                    }
                }
            },
            rowan::NodeOrToken::Node(n) => match n.kind() {
                SyntaxKind::Block => {
                    items.push(lower_block_body(&n));
                }
                _ => {
                    if let Some(v) = lower_node(&n) {
                        items.push(v);
                    }
                }
            },
        }
    }

    Value::While(items)
}

/// Returns (items, is_of).
/// Items: iterable first, then any $bindings.
fn lower_iter_binding(node: &SyntaxNode) -> (Vec<Value>, bool) {
    let mut bindings = Vec::new();
    let mut is_of = false;
    let mut source: Option<Value> = None;
    let mut seen_keyword = false;

    for child in non_trivia_children(node) {
        match child {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::Ident if !seen_keyword => {
                    bindings.push(Value::Variable(t.text().to_string()));
                }
                SyntaxKind::Ident if seen_keyword => {
                    // Identifier after in/of is the source expression
                    source = Some(Value::Variable(t.text().to_string()));
                }
                SyntaxKind::KwIn => { seen_keyword = true; }
                SyntaxKind::KwOf => { seen_keyword = true; is_of = true; }
                SyntaxKind::Comma => {}
                _ => {
                    if seen_keyword {
                        if let Some(v) = lower_token(&t) {
                            source = Some(v);
                        }
                    }
                }
            },
            rowan::NodeOrToken::Node(n) => {
                if let Some(v) = lower_node(&n) {
                    source = Some(v);
                }
            },
        }
    }

    // Order: iterable first, then bindings
    let mut items = Vec::new();
    if let Some(src) = source {
        items.push(src);
    }
    items.extend(bindings);

    (items, is_of)
}

fn lower_array(node: &SyntaxNode) -> Value {
    let mut items = Vec::new();
    for child in non_trivia_children(node) {
        if let Some(t) = child.as_token() {
            match t.kind() {
                SyntaxKind::LBracket | SyntaxKind::RBracket | SyntaxKind::Comma => continue,
                _ => {}
            }
        }
        if let Some(v) = lower_child(child) {
            items.push(v);
        }
    }

    // Pure data (all literals) → lazy list, otherwise eager array
    if items.iter().all(is_data) {
        Value::List(items)
    } else {
        Value::Array(items)
    }
}

fn lower_array_comprehension(node: &SyntaxNode) -> Value {
    let mut body_items = Vec::new(); // items before the keyword
    let mut source_items = Vec::new(); // items after the keyword
    let mut comp_kind = None;
    let mut seen_keyword = false;

    for child in non_trivia_children(node) {
        match child {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::LBracket | SyntaxKind::RBracket | SyntaxKind::Comma => {}
                SyntaxKind::KwFor => { seen_keyword = true; }
                SyntaxKind::KwWhile => { seen_keyword = true; comp_kind = Some('#'); }
                SyntaxKind::KwIn => { seen_keyword = true; if comp_kind.is_none() { comp_kind = Some('>'); } }
                SyntaxKind::KwOf => { seen_keyword = true; comp_kind = Some('<'); }
                _ => {
                    if let Some(v) = lower_token(&t) {
                        if seen_keyword { source_items.push(v); } else { body_items.push(v); }
                    }
                }
            },
            rowan::NodeOrToken::Node(n) => match n.kind() {
                SyntaxKind::IterBinding => {
                    seen_keyword = true;
                    let (binding_items, binding_is_of) = lower_iter_binding(&n);
                    if binding_is_of { comp_kind = Some('<'); }
                    else if comp_kind.is_none() { comp_kind = Some('>'); }
                    source_items.extend(binding_items);
                }
                _ => {
                    if let Some(v) = lower_node(&n) {
                        if seen_keyword { source_items.push(v); } else { body_items.push(v); }
                    }
                }
            },
        }
    }

    // Order: iterable/bindings first, then body expression
    let mut items = source_items;
    items.extend(body_items);

    match comp_kind {
        Some('<') => Value::ListCompOf(items),
        Some('#') => Value::ListCompWhile(items),
        _ => Value::ListCompIn(items),
    }
}

fn lower_object(node: &SyntaxNode) -> Value {
    let mut pairs = Vec::new();
    for child in node.children_with_tokens().filter_map(|c| c.into_node()) {
        if child.kind() == SyntaxKind::Pair {
            let (key, val) = lower_pair(&child);
            pairs.push((key, val));
        }
    }

    // Pure data → lazy map, otherwise can't use map (has expressions)
    if pairs.iter().all(|(k, v)| is_data(k) && is_data(v)) {
        Value::Map(pairs)
    } else {
        // For non-data objects, emit as a block that builds the object
        // For now, use Map anyway (the encoder handles it)
        Value::Map(pairs)
    }
}

fn lower_pair(node: &SyntaxNode) -> (Value, Value) {
    let mut key = Value::Ref("no".into());
    let mut val = Value::Ref("no".into());
    let mut seen_colon = false;

    for child in non_trivia_children(node) {
        match child {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::Colon => {
                    seen_colon = true;
                }
                SyntaxKind::Ident if !seen_colon => {
                    key = Value::String(t.text().to_string());
                }
                _ => {
                    if let Some(v) = lower_token(&t) {
                        if !seen_colon {
                            key = v;
                        } else {
                            val = v;
                        }
                    }
                }
            },
            rowan::NodeOrToken::Node(n) => {
                if let Some(v) = lower_node(&n) {
                    if !seen_colon {
                        key = v;
                    } else {
                        val = v;
                    }
                }
            },
        }
    }

    (key, val)
}

fn lower_object_comprehension(node: &SyntaxNode) -> Value {
    let mut body_items = Vec::new();
    let mut source_items = Vec::new();
    let mut comp_kind = None;
    let mut seen_keyword = false;

    for child in non_trivia_children(node) {
        match child {
            rowan::NodeOrToken::Token(t) => match t.kind() {
                SyntaxKind::LBrace | SyntaxKind::RBrace | SyntaxKind::Comma | SyntaxKind::Colon => {}
                SyntaxKind::KwFor => { seen_keyword = true; }
                SyntaxKind::KwWhile => { seen_keyword = true; comp_kind = Some('#'); }
                SyntaxKind::KwIn => { seen_keyword = true; if comp_kind.is_none() { comp_kind = Some('>'); } }
                SyntaxKind::KwOf => { seen_keyword = true; comp_kind = Some('<'); }
                _ => {
                    if let Some(v) = lower_token(&t) {
                        if seen_keyword { source_items.push(v); } else { body_items.push(v); }
                    }
                }
            },
            rowan::NodeOrToken::Node(n) => match n.kind() {
                SyntaxKind::IterBinding => {
                    seen_keyword = true;
                    let (binding_items, binding_is_of) = lower_iter_binding(&n);
                    if binding_is_of { comp_kind = Some('<'); }
                    else if comp_kind.is_none() { comp_kind = Some('>'); }
                    source_items.extend(binding_items);
                }
                _ => {
                    if let Some(v) = lower_node(&n) {
                        if seen_keyword { source_items.push(v); } else { body_items.push(v); }
                    }
                }
            },
        }
    }

    let mut items = source_items;
    items.extend(body_items);

    match comp_kind {
        Some('<') => Value::MapCompOf(items),
        Some('#') => Value::MapCompWhile(items),
        _ => Value::MapCompIn(items),
    }
}

// ── Template literal lowering ──────────────────────────────────────────

fn lower_template_expr(node: &SyntaxNode) -> Value {
    let mut tag_name: Option<String> = None;
    let mut template_text: Option<String> = None;

    for child in non_trivia_children(node) {
        if let Some(t) = child.as_token() {
            match t.kind() {
                SyntaxKind::Ident => {
                    tag_name = Some(t.text().to_string());
                }
                SyntaxKind::TemplateLiteral => {
                    let text = t.text();
                    // Strip surrounding backticks
                    template_text = Some(text[1..text.len() - 1].to_string());
                }
                _ => {}
            }
        }
    }

    let content = template_text.unwrap_or_default();
    let parts = parse_template_parts(&content);

    match tag_name {
        Some(tag) => lower_tagged_template(&tag, &parts),
        None => lower_untagged_template(&parts),
    }
}

/// A segment of a template literal: either a static string or an interpolation.
enum TemplatePart {
    Static(String),
    Interpolation(String), // raw Rex source inside ${...}
}

/// Parse template content (without backticks) into static and interpolation parts.
fn parse_template_parts(content: &str) -> Vec<TemplatePart> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Escaped character
            match chars.peek() {
                Some('`') => { current.push('`'); chars.next(); }
                Some('$') => { current.push('$'); chars.next(); }
                Some('\\') => { current.push('\\'); chars.next(); }
                Some('n') => { current.push('\n'); chars.next(); }
                Some('t') => { current.push('\t'); chars.next(); }
                Some('r') => { current.push('\r'); chars.next(); }
                _ => { current.push('\\'); }
            }
        } else if c == '$' && chars.peek() == Some(&'{') {
            // Start of interpolation
            chars.next(); // consume '{'
            if !current.is_empty() {
                parts.push(TemplatePart::Static(std::mem::take(&mut current)));
            }
            // Read until matching '}'
            let mut depth = 1;
            let mut expr = String::new();
            while let Some(ch) = chars.next() {
                if ch == '{' {
                    depth += 1;
                    expr.push(ch);
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    expr.push(ch);
                } else {
                    expr.push(ch);
                }
            }
            parts.push(TemplatePart::Interpolation(expr));
        } else {
            current.push(c);
        }
    }

    if !current.is_empty() {
        parts.push(TemplatePart::Static(current));
    }

    parts
}

/// Lower an untagged template: no interpolations → plain string,
/// with interpolations → chain.
fn lower_untagged_template(parts: &[TemplatePart]) -> Value {
    // Check if there are any interpolations
    let has_interpolations = parts.iter().any(|p| matches!(p, TemplatePart::Interpolation(_)));

    if !has_interpolations {
        // No interpolations → plain string
        let s: String = parts.iter().map(|p| match p {
            TemplatePart::Static(s) => s.as_str(),
            _ => "",
        }).collect();
        return Value::String(s);
    }

    // Build chain segments: interleaved string literals and expressions
    let mut segments = Vec::new();
    for part in parts {
        match part {
            TemplatePart::Static(s) => {
                segments.push(Value::String(s.clone()));
            }
            TemplatePart::Interpolation(expr_src) => {
                // Recursively compile the interpolated expression
                let tokens = crate::lexer::lex(expr_src);
                let (green, _errors) = crate::parser::parse(expr_src, &tokens);
                let root = crate::syntax::SyntaxNode::new_root(green);
                let value = lower(&root);
                segments.push(value);
            }
        }
    }

    Value::Chain(segments)
}

/// Lower a tagged template: tag function receives (string_parts_array, ...exprs).
fn lower_tagged_template(tag: &str, parts: &[TemplatePart]) -> Value {
    let mut string_parts = Vec::new();
    let mut exprs = Vec::new();

    for part in parts {
        match part {
            TemplatePart::Static(s) => {
                string_parts.push(Value::String(s.clone()));
            }
            TemplatePart::Interpolation(expr_src) => {
                let tokens = crate::lexer::lex(expr_src);
                let (green, _errors) = crate::parser::parse(expr_src, &tokens);
                let root = crate::syntax::SyntaxNode::new_root(green);
                let value = lower(&root);
                exprs.push(value);
            }
        }
    }

    // If there are interpolations, we need one more static part at the end
    // (like JS: `a${x}b` → ["a", "b"], [x])
    // The parts already alternate correctly, but ensure string_parts bookend properly.
    // Actually, the parser already handles this: static parts appear between/around interpolations.

    // Build: call(tag, [string_parts...], expr1, expr2, ...)
    let mut call_items = Vec::new();
    // Tag as opcode (built-in) — use Variable for user-defined tags
    call_items.push(Value::Variable(tag.to_string()));
    call_items.push(Value::List(string_parts));
    call_items.extend(exprs);

    Value::Call(call_items)
}

/// Returns true if the value is pure data (no computation needed).
fn is_data(v: &Value) -> bool {
    match v {
        Value::Integer(_) | Value::Decimal { .. } | Value::String(_) | Value::Ref(_) => true,
        Value::List(items) => items.iter().all(is_data),
        Value::Map(pairs) => pairs.iter().all(|(k, v)| is_data(k) && is_data(v)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{bytecode, lexer, parser};

    fn compile(source: &str) -> String {
        let tokens = lexer::lex(source);
        let (green, errors) = parser::parse(source, &tokens);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let root = SyntaxNode::new_root(green);
        let value = lower(&root);
        bytecode::encode(&value)
    }

    #[test]
    fn compile_integer() {
        assert_eq!(compile("42"), "1k+");
    }

    #[test]
    fn compile_string() {
        assert_eq!(compile(r#""hello""#), "5,hello");
    }

    #[test]
    fn compile_true() {
        assert_eq!(compile("true"), "t'");
    }

    #[test]
    fn compile_variable() {
        assert_eq!(compile("x"), "x$");
    }

    #[test]
    fn compile_addition() {
        assert_eq!(compile("1 + 2"), "(ad%2+4+)");
    }

    #[test]
    fn compile_assignment() {
        assert_eq!(compile("x = 42"), "=x$1k+");
    }

    #[test]
    fn compile_self() {
        assert_eq!(compile("self"), "@");
    }

    #[test]
    fn compile_array_data() {
        // Pure data array → lazy list
        assert_eq!(compile("[1, 2, 3]"), "6;2+4+6+");
    }

    #[test]
    fn compile_when() {
        let bc = compile("when x do y end");
        assert_eq!(bc, "?(x$y$)");
    }

    #[test]
    fn compile_or() {
        let bc = compile("a or b");
        assert_eq!(bc, "|(a$b$)");
    }

    #[test]
    fn compile_compound_assign() {
        // x += 1 → set(x, add(x, 1))
        let bc = compile("x += 1");
        assert_eq!(bc, "=x$(ad%x$2+)");
    }

    #[test]
    fn compile_break() {
        assert_eq!(compile("break"), "\\");
    }

    #[test]
    fn compile_continue() {
        assert_eq!(compile("continue"), "1\\");
    }

    #[test]
    fn compile_template_no_interpolation() {
        // `hello` → plain string
        assert_eq!(compile("`hello`"), "5,hello");
    }

    #[test]
    fn compile_template_with_interpolation() {
        // `hello ${name}` → chain
        let bc = compile(r"`hello ${name}`");
        // Should be a chain containing string "hello " and variable name
        assert!(bc.contains('.'), "expected chain (.) in bytecode: {bc}");
        assert!(bc.contains("name$"), "expected variable name in bytecode: {bc}");
    }

    #[test]
    fn compile_template_only_interpolation() {
        // `${x}` → chain with just the variable
        let bc = compile(r"`${x}`");
        assert!(bc.contains("x$"), "expected variable x in bytecode: {bc}");
    }

    #[test]
    fn compile_template_multiple_interpolations() {
        let bc = compile(r"`${a} and ${b}`");
        assert!(bc.contains("a$"), "expected variable a: {bc}");
        assert!(bc.contains("b$"), "expected variable b: {bc}");
        assert!(bc.contains('.'), "expected chain: {bc}");
    }

    #[test]
    fn compile_tagged_template() {
        // html`<p>${text}</p>` → call(html, [...strings], text)
        let bc = compile(r"html`<p>${text}</p>`");
        assert!(bc.contains("html$"), "expected html variable: {bc}");
        assert!(bc.contains("text$"), "expected text variable: {bc}");
        assert!(bc.contains('('), "expected call: {bc}");
    }

    #[test]
    fn compile_template_escaped_dollar() {
        // `\${not interpolated}` → plain string
        assert_eq!(compile(r"`\${not interpolated}`"), "j,${not interpolated}");
    }

    #[test]
    fn compile_template_empty() {
        // `` → empty string
        assert_eq!(compile("``"), ",");
    }

    #[test]
    fn compile_roundtrip() {
        // Compile to bytecode, decode back, re-encode, should match
        let sources = [
            "42",
            "true",
            "x",
            "1 + 2",
            "x = 42",
            "[1, 2, 3]",
            "when x do y end",
            "a or b",
            "a and b",
        ];
        for source in sources {
            let bc = compile(source);
            let decoded = bytecode::decode(&bc)
                .unwrap_or_else(|e| panic!("decode failed for {source:?} → {bc:?}: {e}"));
            let re_encoded = bytecode::encode(&decoded);
            assert_eq!(bc, re_encoded, "roundtrip failed for {source:?}");
        }
    }
}
