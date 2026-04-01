use crate::lexer::TokenKind;

/// Unified kind for both leaf tokens and composite CST nodes.
///
/// The first block mirrors `TokenKind` 1:1 (so conversion is a cast).
/// The second block adds composite node kinds for the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // ── Leaf tokens (must stay in sync with TokenKind) ──────────────
    KwAnd = 0,
    KwBreak,
    KwContinue,
    KwDelete,
    KwDo,
    KwElse,
    KwEnd,
    KwExtern,
    KwFalse,
    KwFor,
    KwIn,
    KwInf,
    KwNan,
    KwNot,
    KwNull,
    KwOf,
    KwOr,
    KwReturn,
    KwTrue,
    KwType,
    KwNone,
    KwUnless,
    KwWhen,
    KwWhile,
    Ident,
    HexNumber,
    BinaryNumber,
    DecimalNumber,
    DoubleString,
    SingleString,
    TemplateLiteral,
    Arrow,
    ColonEq,
    EqEq,
    BangEq,
    GtEq,
    LtEq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    DotDot,
    DotParen,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Eq,
    Gt,
    Lt,
    Dot,
    Comma,
    Colon,
    At,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    LineComment,
    BlockComment,
    Whitespace,
    Error,

    // ── Composite nodes ─────────────────────────────────────────────
    Program,
    BinaryExpr,
    UnaryExpr,
    AssignExpr,
    RangeExpr,
    CallExpr,
    NavExpr,
    GroupExpr,
    SelfExpr,
    ConditionalExpr,
    ElseBranch,
    ForExpr,
    WhileExpr,
    Block,
    ArrayExpr,
    ArrayComprehension,
    ObjectExpr,
    ObjectComprehension,
    Pair,
    IterBinding,
    TemplateExpr,
    TypeDecl,
    ExternDecl,
    ReturnExpr,

    /// Rowan requires a dedicated root kind.
    Root,
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::Whitespace | SyntaxKind::LineComment | SyntaxKind::BlockComment
        )
    }

    /// Returns true for keyword tokens that can appear as property names after `.`.
    /// All keywords are valid property names in dot-access position (e.g. `db.delete`).
    pub fn is_keyword(self) -> bool {
        matches!(
            self,
            SyntaxKind::KwAnd
            | SyntaxKind::KwBreak | SyntaxKind::KwContinue | SyntaxKind::KwDelete
            | SyntaxKind::KwDo | SyntaxKind::KwElse | SyntaxKind::KwEnd
            | SyntaxKind::KwExtern | SyntaxKind::KwFalse | SyntaxKind::KwFor
            | SyntaxKind::KwIn | SyntaxKind::KwInf | SyntaxKind::KwNan
            | SyntaxKind::KwNot | SyntaxKind::KwNull
            | SyntaxKind::KwOf | SyntaxKind::KwOr
            | SyntaxKind::KwReturn | SyntaxKind::KwTrue
            | SyntaxKind::KwType | SyntaxKind::KwNone | SyntaxKind::KwUnless
            | SyntaxKind::KwWhen | SyntaxKind::KwWhile
        )
    }
}

impl From<TokenKind> for SyntaxKind {
    fn from(kind: TokenKind) -> Self {
        // SAFETY: the first N variants of SyntaxKind mirror TokenKind exactly,
        // so a discriminant cast is correct.
        let disc = kind as u16;
        // This is safe because we keep the enums in sync.
        unsafe { std::mem::transmute(disc) }
    }
}

/// Marker type for `rowan::Language`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RexLang {}

impl rowan::Language for RexLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        // SAFETY: we only put valid SyntaxKind discriminants into rowan.
        unsafe { std::mem::transmute(raw.0) }
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<RexLang>;
pub type SyntaxToken = rowan::SyntaxToken<RexLang>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_kind_to_syntax_kind_roundtrip() {
        // Spot-check a few conversions
        assert_eq!(SyntaxKind::from(TokenKind::KwAnd), SyntaxKind::KwAnd);
        assert_eq!(SyntaxKind::from(TokenKind::Ident), SyntaxKind::Ident);
        assert_eq!(SyntaxKind::from(TokenKind::Plus), SyntaxKind::Plus);
        assert_eq!(SyntaxKind::from(TokenKind::Error), SyntaxKind::Error);
        assert_eq!(
            SyntaxKind::from(TokenKind::Whitespace),
            SyntaxKind::Whitespace
        );
    }

    #[test]
    fn new_keywords_convert() {
        assert_eq!(SyntaxKind::from(TokenKind::KwType), SyntaxKind::KwType);
        assert_eq!(SyntaxKind::from(TokenKind::KwExtern), SyntaxKind::KwExtern);
    }

    #[test]
    fn trivia_classification() {
        assert!(SyntaxKind::Whitespace.is_trivia());
        assert!(SyntaxKind::LineComment.is_trivia());
        assert!(SyntaxKind::BlockComment.is_trivia());
        assert!(!SyntaxKind::Ident.is_trivia());
        assert!(!SyntaxKind::Plus.is_trivia());
    }
}
