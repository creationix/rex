//! Fast path: lex JSON directly to bytecode `Value`, skipping CST entirely.
//!
//! For pure JSON input this is much faster than lex → parse → CST → lower
//! because it avoids rowan's Arc allocations and the CST tree walk.

use crate::bytecode::Value;
use crate::lexer::{Token, TokenKind};

/// Try to parse the token stream as pure JSON data. Returns `None` if the
/// tokens contain Rex-specific constructs (variables, keywords, operators).
pub fn try_json_to_value(source: &str, tokens: &[Token]) -> Option<Value> {
    let mut p = JsonParser { source, tokens, pos: 0 };
    let val = p.parse_value()?;
    // Must have consumed all non-trivia tokens
    p.skip_trivia();
    if p.pos < p.tokens.len() {
        return None;
    }
    Some(val)
}

struct JsonParser<'s> {
    source: &'s str,
    tokens: &'s [Token],
    pos: usize,
}

impl<'s> JsonParser<'s> {
    fn skip_trivia(&mut self) {
        while self.pos < self.tokens.len() {
            match self.tokens[self.pos].kind {
                TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment => {
                    self.pos += 1;
                }
                _ => break,
            }
        }
    }

    fn current(&mut self) -> Option<TokenKind> {
        self.skip_trivia();
        self.tokens.get(self.pos).map(|t| t.kind)
    }

    fn bump(&mut self) -> &'s str {
        self.skip_trivia();
        let t = &self.tokens[self.pos];
        self.pos += 1;
        &self.source[t.span.clone()]
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.current() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_value(&mut self) -> Option<Value> {
        match self.current()? {
            TokenKind::LBrace => self.parse_object(),
            TokenKind::LBracket => self.parse_array(),
            TokenKind::DoubleString => self.parse_string(),
            TokenKind::SingleString => self.parse_string(),
            TokenKind::DecimalNumber => self.parse_number(),
            TokenKind::HexNumber => self.parse_hex_number(),
            TokenKind::BinaryNumber => self.parse_bin_number(),
            TokenKind::KwTrue => { self.bump(); Some(Value::Ref("t".into())) }
            TokenKind::KwFalse => { self.bump(); Some(Value::Ref("f".into())) }
            TokenKind::KwNull => { self.bump(); Some(Value::Ref("n".into())) }
            TokenKind::KwNone => { self.bump(); Some(Value::Ref("no".into())) }
            TokenKind::KwNan => { self.bump(); Some(Value::Ref("nan".into())) }
            TokenKind::KwInf => { self.bump(); Some(Value::Ref("inf".into())) }
            // Bare identifiers as keys in Rex objects (no quotes)
            TokenKind::Ident => {
                // In JSON context, bare idents only appear as object keys
                // which are handled by parse_object. If we get here, it's
                // a Rex variable — bail.
                None
            }
            // Any Rex-specific token → not pure JSON
            _ => None,
        }
    }

    fn parse_object(&mut self) -> Option<Value> {
        self.eat(TokenKind::LBrace);
        let mut pairs = Vec::new();

        if self.current() == Some(TokenKind::RBrace) {
            self.bump();
            return Some(Value::Object(pairs));
        }

        loop {
            let key = match self.current()? {
                TokenKind::DoubleString | TokenKind::SingleString => {
                    self.parse_string()?
                }
                // Rex allows bare identifier keys
                TokenKind::Ident => {
                    let text = self.bump();
                    Value::String(text.to_string())
                }
                TokenKind::DecimalNumber => {
                    let text = self.bump();
                    Value::String(text.to_string())
                }
                _ => return None,
            };
            // Colon separator (Rex also allows omitting it, but JSON requires it)
            self.eat(TokenKind::Colon);
            let val = self.parse_value()?;
            pairs.push((key, val));

            // Comma or closing brace
            if !self.eat(TokenKind::Comma) {
                break;
            }
            // Trailing comma
            if self.current() == Some(TokenKind::RBrace) {
                break;
            }
        }

        if !self.eat(TokenKind::RBrace) {
            return None;
        }
        Some(Value::Object(pairs))
    }

    fn parse_array(&mut self) -> Option<Value> {
        self.eat(TokenKind::LBracket);
        let mut items = Vec::new();

        if self.current() == Some(TokenKind::RBracket) {
            self.bump();
            return Some(Value::Array(items));
        }

        loop {
            let val = self.parse_value()?;
            items.push(val);

            if !self.eat(TokenKind::Comma) {
                // Rex allows space-separated array elements
                if self.current() == Some(TokenKind::RBracket) {
                    break;
                }
                // Try without comma (Rex-style)
                match self.current() {
                    Some(TokenKind::RBracket) => break,
                    Some(TokenKind::LBrace) | Some(TokenKind::LBracket)
                    | Some(TokenKind::DoubleString) | Some(TokenKind::SingleString)
                    | Some(TokenKind::DecimalNumber) | Some(TokenKind::HexNumber)
                    | Some(TokenKind::BinaryNumber)
                    | Some(TokenKind::KwTrue) | Some(TokenKind::KwFalse)
                    | Some(TokenKind::KwNull) | Some(TokenKind::KwNone) => {
                        continue; // next value without comma
                    }
                    _ => break,
                }
            }
            // Trailing comma
            if self.current() == Some(TokenKind::RBracket) {
                break;
            }
        }

        if !self.eat(TokenKind::RBracket) {
            return None;
        }
        Some(Value::Array(items))
    }

    fn parse_string(&mut self) -> Option<Value> {
        let text = self.bump();
        // Strip quotes
        let inner = &text[1..text.len() - 1];
        // Fast path: no escapes
        if !inner.contains('\\') {
            return Some(Value::String(inner.to_string()));
        }
        Some(Value::String(unescape(inner)))
    }

    fn parse_number(&mut self) -> Option<Value> {
        let text = self.bump();
        let neg = text.starts_with('-');
        let body = text.trim_start_matches('-');

        if body.contains('.') || body.contains('e') || body.contains('E') {
            // Decimal
            let (int_part, frac_part, exp) = split_decimal(body);
            let sig_str = format!("{}{}", int_part, frac_part);
            let sig: i64 = sig_str.parse().ok()?;
            let exp = exp - frac_part.len() as i64;
            let sig = if neg { -sig } else { sig };
            Some(Value::Decimal { sig, exp })
        } else {
            let n: i64 = body.parse().ok()?;
            Some(Value::Integer(if neg { -n } else { n }))
        }
    }

    fn parse_hex_number(&mut self) -> Option<Value> {
        let text = self.bump();
        let neg = text.starts_with('-');
        let body = text.trim_start_matches('-').trim_start_matches("0x");
        let n = i64::from_str_radix(body, 16).ok()?;
        Some(Value::Integer(if neg { -n } else { n }))
    }

    fn parse_bin_number(&mut self) -> Option<Value> {
        let text = self.bump();
        let neg = text.starts_with('-');
        let body = text.trim_start_matches('-').trim_start_matches("0b");
        let n = i64::from_str_radix(body, 2).ok()?;
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
                        if let Some(c) = char::from_u32(n) { out.push(c); }
                    }
                }
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(n) = u32::from_str_radix(&hex, 16) {
                        if let Some(c) = char::from_u32(n) { out.push(c); }
                    }
                }
                Some(other) => { out.push('\\'); out.push(other); }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;

    fn parse_json(source: &str) -> Value {
        let tokens = lexer::lex(source);
        try_json_to_value(source, &tokens)
            .unwrap_or_else(|| panic!("failed to parse as JSON: {source:?}"))
    }

    #[test]
    fn json_integers() {
        assert_eq!(parse_json("42"), Value::Integer(42));
        assert_eq!(parse_json("-1"), Value::Integer(-1));
        assert_eq!(parse_json("0"), Value::Integer(0));
    }

    #[test]
    fn json_strings() {
        assert_eq!(parse_json(r#""hello""#), Value::String("hello".into()));
        assert_eq!(parse_json(r#""a\"b""#), Value::String("a\"b".into()));
    }

    #[test]
    fn json_booleans_null() {
        assert_eq!(parse_json("true"), Value::Ref("t".into()));
        assert_eq!(parse_json("false"), Value::Ref("f".into()));
        assert_eq!(parse_json("null"), Value::Ref("n".into()));
    }

    #[test]
    fn json_array() {
        assert_eq!(
            parse_json("[1, 2, 3]"),
            Value::Array(vec![Value::Integer(1), Value::Integer(2), Value::Integer(3)])
        );
    }

    #[test]
    fn json_object() {
        let v = parse_json(r#"{"a": 1, "b": 2}"#);
        assert_eq!(
            v,
            Value::Object(vec![
                (Value::String("a".into()), Value::Integer(1)),
                (Value::String("b".into()), Value::Integer(2)),
            ])
        );
    }

    #[test]
    fn json_nested() {
        let v = parse_json(r#"{"items": [1, {"x": true}]}"#);
        assert_eq!(
            v,
            Value::Object(vec![(
                Value::String("items".into()),
                Value::Array(vec![
                    Value::Integer(1),
                    Value::Object(vec![(Value::String("x".into()), Value::Ref("t".into()))]),
                ]),
            )])
        );
    }

    #[test]
    fn json_empty() {
        assert_eq!(parse_json("{}"), Value::Object(vec![]));
        assert_eq!(parse_json("[]"), Value::Array(vec![]));
    }

    #[test]
    fn rejects_rex_code() {
        let tokens = lexer::lex("x = 1 + 2");
        assert!(try_json_to_value("x = 1 + 2", &tokens).is_none());
    }
}
