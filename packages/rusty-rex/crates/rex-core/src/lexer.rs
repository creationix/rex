use logos::Logos;
use std::ops::Range;

/// Every token kind the Rex lexer can produce.
///
/// Logos picks the longest match; keywords use a word-boundary callback to
/// prevent `"trueish"` from lexing as `true` + `ish`.
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[logos()]
pub enum TokenKind {
    // ── Keywords (alphabetical) ─────────────────────────────────────
    #[token("and", word_boundary)]
    KwAnd,
    #[token("array", word_boundary)]
    KwArray,
    #[token("boolean", word_boundary)]
    KwBoolean,
    #[token("break", word_boundary)]
    KwBreak,
    #[token("continue", word_boundary)]
    KwContinue,
    #[token("delete", word_boundary)]
    KwDelete,
    #[token("do", word_boundary)]
    KwDo,
    #[token("else", word_boundary)]
    KwElse,
    #[token("end", word_boundary)]
    KwEnd,
    #[token("extern", word_boundary)]
    KwExtern,
    #[token("false", word_boundary)]
    KwFalse,
    #[token("for", word_boundary)]
    KwFor,
    #[token("in", word_boundary)]
    KwIn,
    #[token("inf", word_boundary)]
    KwInf,
    #[token("nan", word_boundary)]
    KwNan,
    #[token("not", word_boundary)]
    KwNot,
    #[token("null", word_boundary)]
    KwNull,
    #[token("number", word_boundary)]
    KwNumber,
    #[token("object", word_boundary)]
    KwObject,
    #[token("of", word_boundary)]
    KwOf,
    #[token("or", word_boundary)]
    KwOr,
    #[token("return", word_boundary)]
    KwReturn,
    #[token("string", word_boundary)]
    KwString,
    #[token("true", word_boundary)]
    KwTrue,
    #[token("type", word_boundary)]
    KwType,
    #[token("none", word_boundary)]
    KwNone,
    #[token("unless", word_boundary)]
    KwUnless,
    #[token("when", word_boundary)]
    KwWhen,
    #[token("while", word_boundary)]
    KwWhile,

    // ── Identifiers ─────────────────────────────────────────────────
    // Must come after keywords so logos tries keyword tokens first.
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_-]*")]
    Ident,

    // ── Number literals ─────────────────────────────────────────────
    #[regex(r"-?0x[0-9a-fA-F]+")]
    HexNumber,
    #[regex(r"-?0b[01]+")]
    BinaryNumber,
    #[regex(r"-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?")]
    DecimalNumber,

    // ── String literals ─────────────────────────────────────────────
    #[regex(r#""([^"\\]|\\.)*""#)]
    DoubleString,
    #[regex(r"'([^'\\]|\\.)*'")]
    SingleString,
    #[token("`", lex_template_literal)]
    TemplateLiteral,

    // ── Multi-char operators (longest-match first) ───────────────────
    #[token("->")]
    Arrow,
    #[token(":=")]
    ColonEq,
    #[token("==")]
    EqEq,
    #[token("!=")]
    BangEq,
    #[token(">=")]
    GtEq,
    #[token("<=")]
    LtEq,
    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    #[token("%=")]
    PercentEq,
    #[token("&=")]
    AmpEq,
    #[token("|=")]
    PipeEq,
    #[token("^=")]
    CaretEq,
    #[token("..")]
    DotDot,
    #[token(".(")]
    DotParen,

    // ── Single-char operators & delimiters ───────────────────────────
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("&")]
    Amp,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("~")]
    Tilde,
    #[token("=")]
    Eq,
    #[token(">")]
    Gt,
    #[token("<")]
    Lt,
    #[token(".")]
    Dot,
    #[token(",")]
    Comma,
    #[token(":")]
    Colon,
    #[token("@")]
    At,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,

    // ── Comments ────────────────────────────────────────────────────
    #[regex(r"//[^\n]*(\n)?", allow_greedy = true)]
    LineComment,
    #[regex(r"/\*([^*]|\*[^/])*\*/")]
    BlockComment,

    // ── Trivia ──────────────────────────────────────────────────────
    #[regex(r"[ \t\r\n]+")]
    Whitespace,

    // ── Error ───────────────────────────────────────────────────────
    Error,
}

/// Check that the character after a keyword match is not a valid identifier
/// continuation character (`[a-zA-Z0-9_-]`). This mirrors the Ohm `~nameTail`
/// guard on every keyword token.
/// Manually scan a template literal, tracking `${...}` brace depth so that
/// nested template literals inside interpolations are handled correctly.
/// Called after the opening backtick has been consumed.
fn lex_template_literal(lex: &mut logos::Lexer<TokenKind>) -> bool {
    let rest = lex.remainder().as_bytes();
    let mut i = 0;
    while i < rest.len() {
        match rest[i] {
            b'`' => {
                // Closing backtick — consume it and succeed
                lex.bump(i + 1);
                return true;
            }
            b'\\' => {
                // Escape sequence — skip next byte
                i += 2;
            }
            b'$' if i + 1 < rest.len() && rest[i + 1] == b'{' => {
                // Interpolation start — track brace depth
                i += 2;
                let mut depth: u32 = 1;
                while i < rest.len() && depth > 0 {
                    match rest[i] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        b'`' => {
                            // Nested template literal — recurse by scanning it
                            i += 1;
                            let mut nested_done = false;
                            while i < rest.len() && !nested_done {
                                match rest[i] {
                                    b'`' => { i += 1; nested_done = true; }
                                    b'\\' => { i += 2; }
                                    b'$' if i + 1 < rest.len() && rest[i + 1] == b'{' => {
                                        // Nested interpolation in nested template
                                        i += 2;
                                        let mut d2: u32 = 1;
                                        while i < rest.len() && d2 > 0 {
                                            match rest[i] {
                                                b'{' => d2 += 1,
                                                b'}' => d2 -= 1,
                                                b'\\' => { i += 1; }
                                                _ => {}
                                            }
                                            i += 1;
                                        }
                                    }
                                    _ => { i += 1; }
                                }
                            }
                            continue;
                        }
                        b'\\' => { i += 1; }
                        b'\'' => {
                            // Skip single-quoted string inside interpolation
                            i += 1;
                            while i < rest.len() && rest[i] != b'\'' {
                                if rest[i] == b'\\' { i += 1; }
                                i += 1;
                            }
                        }
                        b'"' => {
                            // Skip double-quoted string inside interpolation
                            i += 1;
                            while i < rest.len() && rest[i] != b'"' {
                                if rest[i] == b'\\' { i += 1; }
                                i += 1;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    false // unterminated template literal
}

fn word_boundary(lex: &logos::Lexer<TokenKind>) -> bool {
    lex.remainder()
        .chars()
        .next()
        .map_or(true, |c| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
}

/// A token with its kind and byte-offset span into the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
}

/// Lex the full source into a flat token vector, including trivia
/// (whitespace and comments). Never fails — unrecognised bytes produce
/// `TokenKind::Error` tokens.
pub fn lex(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut lexer = TokenKind::lexer(source);
    while let Some(result) = lexer.next() {
        let kind = result.unwrap_or(TokenKind::Error);
        let span = lexer.span();
        tokens.push(Token { kind, span });
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source).into_iter().map(|t| t.kind).collect()
    }

    fn non_trivia(source: &str) -> Vec<TokenKind> {
        lex(source)
            .into_iter()
            .filter(|t| {
                !matches!(
                    t.kind,
                    TokenKind::Whitespace | TokenKind::LineComment | TokenKind::BlockComment
                )
            })
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn keywords_vs_identifiers() {
        assert_eq!(non_trivia("true"), vec![TokenKind::KwTrue]);
        assert_eq!(non_trivia("trueish"), vec![TokenKind::Ident]);
        assert_eq!(non_trivia("in"), vec![TokenKind::KwIn]);
        assert_eq!(non_trivia("info"), vec![TokenKind::Ident]);
        assert_eq!(non_trivia("infinity"), vec![TokenKind::Ident]);
        assert_eq!(non_trivia("inf"), vec![TokenKind::KwInf]);
        assert_eq!(non_trivia("self"), vec![TokenKind::Ident]);
        assert_eq!(non_trivia("selfish"), vec![TokenKind::Ident]);
    }

    #[test]
    fn number_literals() {
        assert_eq!(non_trivia("42"), vec![TokenKind::DecimalNumber]);
        assert_eq!(non_trivia("3.14"), vec![TokenKind::DecimalNumber]);
        assert_eq!(non_trivia("1e10"), vec![TokenKind::DecimalNumber]);
        assert_eq!(non_trivia("0xff"), vec![TokenKind::HexNumber]);
        assert_eq!(non_trivia("0b1010"), vec![TokenKind::BinaryNumber]);
    }

    #[test]
    fn string_literals() {
        assert_eq!(non_trivia(r#""hello""#), vec![TokenKind::DoubleString]);
        assert_eq!(non_trivia(r#"'world'"#), vec![TokenKind::SingleString]);
        assert_eq!(
            non_trivia(r#""escaped\"quote""#),
            vec![TokenKind::DoubleString]
        );
    }

    #[test]
    fn operators_and_delimiters() {
        assert_eq!(
            non_trivia(":= == != >= <= .."),
            vec![
                TokenKind::ColonEq,
                TokenKind::EqEq,
                TokenKind::BangEq,
                TokenKind::GtEq,
                TokenKind::LtEq,
                TokenKind::DotDot,
            ]
        );
    }

    #[test]
    fn dot_paren_vs_dot_lparen() {
        // `.(` should lex as a single DotParen token
        assert_eq!(non_trivia(".("), vec![TokenKind::DotParen]);
        // `. (` with space should be Dot then LParen
        assert_eq!(
            non_trivia(". ("),
            vec![TokenKind::Dot, TokenKind::LParen]
        );
    }

    #[test]
    fn comments() {
        assert_eq!(
            kinds("// line\n"),
            vec![TokenKind::LineComment]
        );
        assert_eq!(
            kinds("/* block */"),
            vec![TokenKind::BlockComment]
        );
    }

    #[test]
    fn simple_expression() {
        assert_eq!(
            non_trivia("x + 1"),
            vec![TokenKind::Ident, TokenKind::Plus, TokenKind::DecimalNumber]
        );
    }

    #[test]
    fn conditional() {
        assert_eq!(
            non_trivia("when x do y end"),
            vec![
                TokenKind::KwWhen,
                TokenKind::Ident,
                TokenKind::KwDo,
                TokenKind::Ident,
                TokenKind::KwEnd,
            ]
        );
    }


    #[test]
    fn trivia_preserved() {
        let tokens = lex("x + y");
        assert_eq!(tokens.len(), 5); // x, ws, +, ws, y
        assert_eq!(tokens[1].kind, TokenKind::Whitespace);
        assert_eq!(tokens[3].kind, TokenKind::Whitespace);
    }

    #[test]
    fn dashed_identifier() {
        assert_eq!(non_trivia("my-var"), vec![TokenKind::Ident]);
    }

    #[test]
    fn template_literal() {
        assert_eq!(non_trivia("`hello`"), vec![TokenKind::TemplateLiteral]);
        assert_eq!(non_trivia(r"`hello ${name}`"), vec![TokenKind::TemplateLiteral]);
        assert_eq!(non_trivia(r"`escaped \` backtick`"), vec![TokenKind::TemplateLiteral]);
        // Tagged template: identifier followed by template
        assert_eq!(
            non_trivia(r"html`<p>${text}</p>`"),
            vec![TokenKind::Ident, TokenKind::TemplateLiteral]
        );
    }

    #[test]
    fn type_and_extern_keywords() {
        assert_eq!(non_trivia("type"), vec![TokenKind::KwType]);
        assert_eq!(non_trivia("extern"), vec![TokenKind::KwExtern]);
        // Not keywords when part of longer identifier
        assert_eq!(non_trivia("typedef"), vec![TokenKind::Ident]);
        assert_eq!(non_trivia("external"), vec![TokenKind::Ident]);
        // mut is NOT a keyword — always an identifier
        assert_eq!(non_trivia("mut"), vec![TokenKind::Ident]);
    }

    #[test]
    fn assign_ops() {
        assert_eq!(
            non_trivia("+= -= *= /= %= &= |= ^="),
            vec![
                TokenKind::PlusEq,
                TokenKind::MinusEq,
                TokenKind::StarEq,
                TokenKind::SlashEq,
                TokenKind::PercentEq,
                TokenKind::AmpEq,
                TokenKind::PipeEq,
                TokenKind::CaretEq,
            ]
        );
    }
}
