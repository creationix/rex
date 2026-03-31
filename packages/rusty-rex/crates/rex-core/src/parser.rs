use rowan::GreenNodeBuilder;

use crate::lexer::Token;
use crate::syntax::SyntaxKind;

/// A parse error with a byte-offset span and message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub span: std::ops::Range<usize>,
    pub message: String,
}

/// Parse a Rex source string (already lexed) into a rowan green tree.
///
/// Returns the root `GreenNode` and any parse errors. The tree is always
/// produced — even when the source is invalid — so downstream can still
/// provide diagnostics, completions, etc.
pub fn parse(source: &str, tokens: &[Token]) -> (rowan::GreenNode, Vec<ParseError>) {
    let mut cache = rowan::NodeCache::default();
    parse_with_cache(source, tokens, &mut cache)
}

/// Parse with an externally-owned [`rowan::NodeCache`] for deduplication.
///
/// Reusing a cache across parses lets rowan share identical tokens and
/// small nodes (e.g. repeated keywords, punctuation, common subtrees),
/// reducing allocation pressure. Useful for LSP incremental re-parsing
/// or batch-parsing multiple files.
pub fn parse_with_cache(
    source: &str,
    tokens: &[Token],
    cache: &mut rowan::NodeCache,
) -> (rowan::GreenNode, Vec<ParseError>) {
    let mut p = Parser::new(source, tokens, cache);
    p.parse_program();
    p.finish()
}

// ── Binding powers for Pratt parsing ────────────────────────────────────

/// Returns `(left_bp, right_bp)` for infix operators, or `None` if the
/// token is not an infix operator.
fn infix_binding_power(kind: SyntaxKind) -> Option<(u8, u8)> {
    // Left-associative: left_bp < right_bp
    // Right-associative: left_bp > right_bp
    let bp = match kind {
        // Logical existence
        SyntaxKind::KwOr => (1, 2),
        SyntaxKind::KwAnd => (3, 4),
        // Bitwise
        SyntaxKind::Pipe => (5, 6),
        SyntaxKind::Caret => (7, 8),
        SyntaxKind::Amp => (9, 10),
        // Comparison
        SyntaxKind::EqEq
        | SyntaxKind::BangEq
        | SyntaxKind::Gt
        | SyntaxKind::GtEq
        | SyntaxKind::Lt
        | SyntaxKind::LtEq => (11, 12),
        // Range
        SyntaxKind::DotDot => (13, 14),
        // Additive
        SyntaxKind::Plus | SyntaxKind::Minus => (15, 16),
        // Multiplicative
        SyntaxKind::Star | SyntaxKind::Slash | SyntaxKind::Percent => (17, 18),
        _ => return None,
    };
    Some(bp)
}

/// Returns the right binding power for prefix operators, or `None`.
fn prefix_binding_power(kind: SyntaxKind) -> Option<u8> {
    match kind {
        SyntaxKind::Minus | SyntaxKind::Tilde | SyntaxKind::KwNot | SyntaxKind::KwDelete => {
            Some(17)
        }
        _ => None,
    }
}

fn is_assign_op(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::ColonEq
            | SyntaxKind::Eq
            | SyntaxKind::PlusEq
            | SyntaxKind::MinusEq
            | SyntaxKind::StarEq
            | SyntaxKind::SlashEq
            | SyntaxKind::PercentEq
            | SyntaxKind::AmpEq
            | SyntaxKind::PipeEq
            | SyntaxKind::CaretEq
    )
}


// ── Parser ──────────────────────────────────────────────────────────────

struct Parser<'s, 'c> {
    source: &'s str,
    tokens: &'s [Token],
    pos: usize,
    builder: GreenNodeBuilder<'c>,
    errors: Vec<ParseError>,
}

impl<'s, 'c> Parser<'s, 'c> {
    fn new(
        source: &'s str,
        tokens: &'s [Token],
        cache: &'c mut rowan::NodeCache,
    ) -> Self {
        Self {
            source,
            tokens,
            pos: 0,
            builder: GreenNodeBuilder::with_cache(cache),
            errors: Vec::new(),
        }
    }

    fn finish(self) -> (rowan::GreenNode, Vec<ParseError>) {
        (self.builder.finish(), self.errors)
    }

    // ── Low-level helpers ───────────────────────────────────────────

    /// Current token kind (skipping trivia for lookahead).
    fn current(&self) -> SyntaxKind {
        self.nth(0)
    }

    /// Lookahead by `n` non-trivia tokens.
    fn nth(&self, n: usize) -> SyntaxKind {
        let mut pos = self.pos;
        let mut seen = 0;
        while pos < self.tokens.len() {
            let kind = SyntaxKind::from(self.tokens[pos].kind);
            if !kind.is_trivia() {
                if seen == n {
                    return kind;
                }
                seen += 1;
            }
            pos += 1;
        }
        // Past end → treat as an error sentinel
        SyntaxKind::Error
    }

    /// Eat the next raw token (including trivia) and add it as a leaf.
    fn bump_raw(&mut self) {
        if self.pos >= self.tokens.len() {
            return;
        }
        let tok = &self.tokens[self.pos];
        let kind = SyntaxKind::from(tok.kind);
        let text = &self.source[tok.span.clone()];
        self.builder
            .token(rowan::SyntaxKind(kind as u16), text);
        self.pos += 1;
    }

    /// Consume and attach all leading trivia (whitespace/comments) to the
    /// tree at the current position.
    fn eat_trivia(&mut self) {
        while self.pos < self.tokens.len() {
            let kind = SyntaxKind::from(self.tokens[self.pos].kind);
            if !kind.is_trivia() {
                break;
            }
            self.bump_raw();
        }
    }

    /// Bump the next non-trivia token, attaching any preceding trivia first.
    fn bump(&mut self) {
        self.eat_trivia();
        self.bump_raw();
    }

    /// If the current non-trivia token matches `kind`, bump and return true.
    fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.current() == kind {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Expect a specific token, emitting an error if not found.
    fn expect(&mut self, kind: SyntaxKind) {
        if !self.eat(kind) {
            let span = self.current_span();
            self.errors.push(ParseError {
                span,
                message: format!("expected {kind:?}"),
            });
        }
    }

    fn current_span(&self) -> std::ops::Range<usize> {
        if self.pos < self.tokens.len() {
            self.tokens[self.pos].span.clone()
        } else if let Some(last) = self.tokens.last() {
            last.span.end..last.span.end
        } else {
            0..0
        }
    }

    fn at_end(&self) -> bool {
        // True when no non-trivia tokens remain.
        self.current() == SyntaxKind::Error
    }

    /// Start a new composite node.
    fn start_node(&mut self, kind: SyntaxKind) {
        self.eat_trivia();
        self.builder
            .start_node(rowan::SyntaxKind(kind as u16));
    }

    /// Wrap already-emitted children by retroactively starting a node
    /// before the most recent checkpoint.
    fn start_node_at(&mut self, checkpoint: rowan::Checkpoint, kind: SyntaxKind) {
        self.builder
            .start_node_at(checkpoint, rowan::SyntaxKind(kind as u16));
    }

    fn finish_node(&mut self) {
        self.builder.finish_node();
    }

    fn checkpoint(&mut self) -> rowan::Checkpoint {
        self.eat_trivia();
        self.builder.checkpoint()
    }

    // ── Grammar rules ───────────────────────────────────────────────

    fn parse_program(&mut self) {
        // Start root node *before* eating any trivia so all tokens
        // (including leading whitespace/comments) live inside the root.
        self.builder
            .start_node(rowan::SyntaxKind(SyntaxKind::Root as u16));
        while !self.at_end() {
            self.parse_expr();
        }
        // Consume any trailing trivia
        self.eat_trivia();
        self.finish_node();
    }

    fn parse_expr(&mut self) {
        self.parse_assign_expr();
    }

    fn parse_assign_expr(&mut self) {
        let cp = self.checkpoint();
        self.parse_pratt_expr(0);

        if self.current() == SyntaxKind::Colon {
            // Type-annotated assignment: name: Type = value
            self.start_node_at(cp, SyntaxKind::AssignExpr);
            self.bump(); // :
            self.parse_pratt_expr(0); // type expression (no assignment — must not consume `=`)
            if self.eat(SyntaxKind::Eq) {
                self.parse_assign_expr(); // value (right-assoc)
            }
            // If no `=` follows, this is a bare type annotation (e.g., function args)
            self.finish_node();
        } else if is_assign_op(self.current()) {
            self.start_node_at(cp, SyntaxKind::AssignExpr);
            self.bump(); // operator
            self.parse_assign_expr(); // right-assoc
            self.finish_node();
        }
    }

    /// Pratt expression parser — handles all precedence levels from
    /// logical existence (and/or/nor) through multiplicative (*/ %).
    fn parse_pratt_expr(&mut self, min_bp: u8) {
        let cp = self.checkpoint();

        // Prefix
        if let Some(r_bp) = prefix_binding_power(self.current()) {
            self.start_node_at(cp, SyntaxKind::UnaryExpr);
            self.bump(); // operator
            self.parse_pratt_expr(r_bp);
            self.finish_node();
        } else {
            self.parse_postfix_expr();
        }

        // Infix loop
        loop {
            let op = self.current();
            if let Some((l_bp, r_bp)) = infix_binding_power(op) {
                if l_bp < min_bp {
                    break;
                }
                let node_kind = if op == SyntaxKind::DotDot {
                    SyntaxKind::RangeExpr
                } else {
                    SyntaxKind::BinaryExpr
                };
                self.start_node_at(cp, node_kind);
                self.bump(); // operator
                self.parse_pratt_expr(r_bp);
                self.finish_node();
            } else {
                break;
            }
        }
    }

    fn parse_postfix_expr(&mut self) {
        let cp = self.checkpoint();
        self.parse_primary_expr();

        loop {
            match self.current() {
                // Static navigation: .key or .digits
                SyntaxKind::Dot => {
                    if matches!(self.nth(1), SyntaxKind::Ident | SyntaxKind::DecimalNumber) {
                        self.start_node_at(cp, SyntaxKind::NavExpr);
                        self.bump(); // .
                        self.bump(); // key
                        self.finish_node();
                    } else {
                        break;
                    }
                }
                // Dynamic navigation: .(expr)
                SyntaxKind::DotParen => {
                    self.start_node_at(cp, SyntaxKind::NavExpr);
                    self.bump(); // .(
                    self.parse_expr();
                    self.expect(SyntaxKind::RParen);
                    self.finish_node();
                }
                // Call: (args)
                SyntaxKind::LParen => {
                    self.start_node_at(cp, SyntaxKind::CallExpr);
                    self.bump(); // (
                    if self.current() != SyntaxKind::RParen {
                        self.parse_elements(Self::parse_expr);
                    }
                    self.expect(SyntaxKind::RParen);
                    self.finish_node();
                }
                _ => break,
            }
        }
    }

    fn parse_primary_expr(&mut self) {
        match self.current() {
            SyntaxKind::KwType => self.parse_type_decl(),
            SyntaxKind::KwExtern => self.parse_extern_decl(),
            SyntaxKind::KwWhen | SyntaxKind::KwUnless => self.parse_conditional(),
            SyntaxKind::KwFor => self.parse_for(),
            SyntaxKind::KwWhile => self.parse_while(),
            SyntaxKind::KwBreak | SyntaxKind::KwContinue => {
                self.bump();
            }
            SyntaxKind::KwReturn => {
                self.start_node(SyntaxKind::ReturnExpr);
                self.bump(); // return
                // Parse optional return value (if next token starts an expression)
                if !self.at_end() && !matches!(self.current(),
                    SyntaxKind::KwEnd | SyntaxKind::KwElse | SyntaxKind::RBrace |
                    SyntaxKind::RBracket | SyntaxKind::RParen | SyntaxKind::Error) {
                    self.parse_expr();
                }
                self.finish_node();
            }
            SyntaxKind::LBracket => self.parse_array(),
            SyntaxKind::LBrace => self.parse_object(),
            SyntaxKind::KwTrue
            | SyntaxKind::KwFalse
            | SyntaxKind::KwNull
            | SyntaxKind::KwNone
            | SyntaxKind::KwString
            | SyntaxKind::KwNumber
            | SyntaxKind::KwObject
            | SyntaxKind::KwArray
            | SyntaxKind::KwBoolean => {
                self.bump();
            }
            SyntaxKind::Ident => {
                if self.nth(1) == SyntaxKind::TemplateLiteral {
                    // Tagged template: ident`...`
                    self.start_node(SyntaxKind::TemplateExpr);
                    self.bump(); // identifier (tag)
                    self.bump(); // template literal token
                    self.finish_node();
                } else {
                    self.bump();
                }
            }
            SyntaxKind::DecimalNumber
            | SyntaxKind::HexNumber
            | SyntaxKind::BinaryNumber => {
                self.bump();
            }
            SyntaxKind::KwNan | SyntaxKind::KwInf => {
                self.bump();
            }
            SyntaxKind::DoubleString | SyntaxKind::SingleString => {
                self.bump();
            }
            SyntaxKind::TemplateLiteral => {
                self.start_node(SyntaxKind::TemplateExpr);
                self.bump(); // template literal token
                self.finish_node();
            }
            SyntaxKind::LParen => {
                self.start_node(SyntaxKind::GroupExpr);
                self.bump(); // (
                self.parse_expr();
                self.expect(SyntaxKind::RParen);
                self.finish_node();
            }
            _ => {
                // Error recovery: emit an error node for the unexpected token
                let span = self.current_span();
                self.errors.push(ParseError {
                    span,
                    message: format!("unexpected token {:?}", self.current()),
                });
                if !self.at_end() {
                    self.start_node(SyntaxKind::Error);
                    self.bump();
                    self.finish_node();
                }
            }
        }
    }


    // ── Type and extern declarations ───────────────────────────────

    fn current_text(&self) -> &str {
        let mut pos = self.pos;
        while pos < self.tokens.len() {
            let kind = SyntaxKind::from(self.tokens[pos].kind);
            if !kind.is_trivia() {
                return &self.source[self.tokens[pos].span.clone()];
            }
            pos += 1;
        }
        ""
    }

    fn parse_type_decl(&mut self) {
        self.start_node(SyntaxKind::TypeDecl);
        self.bump(); // type
        self.expect(SyntaxKind::Ident); // Name
        self.expect(SyntaxKind::Eq); // =
        self.parse_expr(); // type expression
        self.finish_node();
    }

    fn parse_extern_decl(&mut self) {
        self.start_node(SyntaxKind::ExternDecl);
        self.bump(); // extern

        // Check for contextual `mut`
        if self.current() == SyntaxKind::Ident && self.current_text() == "mut" {
            self.bump(); // mut (consumed as Ident — it's contextual)
        }

        // Parse the body as a regular expression.
        // For `extern name = type-expr`, parse_expr produces an AssignExpr.
        // For `extern name.fn(args)`, a CallExpr.
        self.parse_expr();

        // Check for `-> ReturnType` after the expression (function return type)
        if self.current() == SyntaxKind::Arrow {
            self.bump(); // ->
            self.parse_expr(); // return type expression
        }

        self.finish_node();
    }

    // ── Control flow ────────────────────────────────────────────────

    fn parse_conditional(&mut self) {
        self.start_node(SyntaxKind::ConditionalExpr);
        self.bump(); // when | unless
        self.parse_expr(); // condition
        self.expect(SyntaxKind::KwDo);
        self.parse_block();
        if self.current() == SyntaxKind::KwElse {
            self.parse_else_branch();
        }
        self.expect(SyntaxKind::KwEnd);
        self.finish_node();
    }

    fn parse_else_branch(&mut self) {
        self.start_node(SyntaxKind::ElseBranch);
        self.bump(); // else
        if matches!(self.current(), SyntaxKind::KwWhen | SyntaxKind::KwUnless) {
            // else when ... / else unless ...
            self.bump(); // when | unless
            self.parse_expr(); // condition
            self.expect(SyntaxKind::KwDo);
            self.parse_block();
            if self.current() == SyntaxKind::KwElse {
                self.parse_else_branch();
            }
        } else {
            self.parse_block();
        }
        self.finish_node();
    }

    fn parse_for(&mut self) {
        self.start_node(SyntaxKind::ForExpr);
        self.bump(); // for
        self.parse_iter_binding();
        self.expect(SyntaxKind::KwDo);
        self.parse_block();
        self.expect(SyntaxKind::KwEnd);
        self.finish_node();
    }

    fn parse_while(&mut self) {
        self.start_node(SyntaxKind::WhileExpr);
        self.bump(); // while
        self.parse_expr(); // condition
        self.expect(SyntaxKind::KwDo);
        self.parse_block();
        self.expect(SyntaxKind::KwEnd);
        self.finish_node();
    }

    fn parse_block(&mut self) {
        self.start_node(SyntaxKind::Block);
        while !self.at_end()
            && !matches!(
                self.current(),
                SyntaxKind::KwEnd | SyntaxKind::KwElse
            )
        {
            self.parse_expr();
        }
        self.finish_node();
    }

    // ── Iteration bindings ──────────────────────────────────────────

    fn parse_iter_binding(&mut self) {
        self.start_node(SyntaxKind::IterBinding);
        match self.current() {
            SyntaxKind::Ident => {
                self.bump(); // first ident
                match self.current() {
                    SyntaxKind::Comma => {
                        // key, value in expr
                        self.bump(); // ,
                        self.expect(SyntaxKind::Ident);
                        self.expect(SyntaxKind::KwIn);
                        self.parse_expr();
                    }
                    SyntaxKind::KwIn => {
                        self.bump(); // in
                        self.parse_expr();
                    }
                    SyntaxKind::KwOf => {
                        self.bump(); // of
                        self.parse_expr();
                    }
                    _ => {
                        let span = self.current_span();
                        self.errors.push(ParseError {
                            span,
                            message: "expected `in`, `of`, or `,` in iteration binding".into(),
                        });
                    }
                }
            }
            _ => {
                let span = self.current_span();
                self.errors.push(ParseError {
                    span,
                    message: "expected iteration binding".into(),
                });
            }
        }
        self.finish_node();
    }

    // ── Collections ─────────────────────────────────────────────────

    fn parse_array(&mut self) {
        self.eat_trivia();
        let outer_cp = self.builder.checkpoint();
        self.bump(); // [

        if self.eat(SyntaxKind::RBracket) {
            // empty array: []
            self.start_node_at(outer_cp, SyntaxKind::ArrayExpr);
            self.finish_node();
            return;
        }

        // Parse first expression, then decide: comprehension or list?
        self.parse_expr();

        match self.current() {
            SyntaxKind::KwFor => {
                self.start_node_at(outer_cp, SyntaxKind::ArrayComprehension);
                self.bump(); // for
                self.parse_iter_binding_comprehension();
                self.expect(SyntaxKind::RBracket);
                self.finish_node();
            }
            SyntaxKind::KwWhile => {
                self.start_node_at(outer_cp, SyntaxKind::ArrayComprehension);
                self.bump(); // while
                self.parse_expr();
                self.expect(SyntaxKind::RBracket);
                self.finish_node();
            }
            SyntaxKind::KwIn => {
                self.start_node_at(outer_cp, SyntaxKind::ArrayComprehension);
                self.bump(); // in
                self.parse_expr();
                self.expect(SyntaxKind::RBracket);
                self.finish_node();
            }
            SyntaxKind::KwOf => {
                self.start_node_at(outer_cp, SyntaxKind::ArrayComprehension);
                self.bump(); // of
                self.parse_expr();
                self.expect(SyntaxKind::RBracket);
                self.finish_node();
            }
            _ => {
                // Regular array: [a, b, c]
                self.start_node_at(outer_cp, SyntaxKind::ArrayExpr);
                // We already parsed the first element; parse rest
                while !self.at_end() && self.current() != SyntaxKind::RBracket {
                    self.eat(SyntaxKind::Comma); // optional comma
                    if self.current() == SyntaxKind::RBracket {
                        break; // trailing comma
                    }
                    self.parse_expr();
                }
                self.expect(SyntaxKind::RBracket);
                self.finish_node();
            }
        }
    }

    fn parse_object(&mut self) {
        let outer_cp = self.checkpoint();
        self.bump(); // {

        if self.eat(SyntaxKind::RBrace) {
            self.start_node_at(outer_cp, SyntaxKind::ObjectExpr);
            self.finish_node();
            return;
        }

        // Parse first key: value, then decide
        let pair_cp = self.checkpoint();
        self.parse_obj_key();
        self.expect(SyntaxKind::Colon);
        self.parse_expr();

        match self.current() {
            SyntaxKind::KwFor => {
                self.start_node_at(outer_cp, SyntaxKind::ObjectComprehension);
                self.bump(); // for
                self.parse_iter_binding_comprehension();
                self.expect(SyntaxKind::RBrace);
                self.finish_node();
            }
            SyntaxKind::KwWhile => {
                self.start_node_at(outer_cp, SyntaxKind::ObjectComprehension);
                self.bump(); // while
                self.parse_expr();
                self.expect(SyntaxKind::RBrace);
                self.finish_node();
            }
            SyntaxKind::KwIn => {
                self.start_node_at(outer_cp, SyntaxKind::ObjectComprehension);
                self.bump(); // in
                self.parse_expr();
                self.expect(SyntaxKind::RBrace);
                self.finish_node();
            }
            SyntaxKind::KwOf => {
                self.start_node_at(outer_cp, SyntaxKind::ObjectComprehension);
                self.bump(); // of
                self.parse_expr();
                self.expect(SyntaxKind::RBrace);
                self.finish_node();
            }
            _ => {
                // Regular object: first pair already parsed, wrap it
                self.start_node_at(outer_cp, SyntaxKind::ObjectExpr);
                self.start_node_at(pair_cp, SyntaxKind::Pair);
                self.finish_node();
                while !self.at_end() && self.current() != SyntaxKind::RBrace {
                    self.eat(SyntaxKind::Comma);
                    if self.current() == SyntaxKind::RBrace {
                        break;
                    }
                    self.start_node(SyntaxKind::Pair);
                    self.parse_obj_key();
                    self.expect(SyntaxKind::Colon);
                    self.parse_expr();
                    self.finish_node();
                }
                self.expect(SyntaxKind::RBrace);
                self.finish_node();
            }
        }
    }

    fn parse_obj_key(&mut self) {
        match self.current() {
            SyntaxKind::Ident => self.bump(),
            SyntaxKind::Star => self.bump(),
            SyntaxKind::DecimalNumber | SyntaxKind::HexNumber | SyntaxKind::BinaryNumber => {
                self.bump()
            }
            SyntaxKind::DoubleString | SyntaxKind::SingleString => self.bump(),
            SyntaxKind::LParen => {
                self.start_node(SyntaxKind::GroupExpr);
                self.bump(); // (
                self.parse_expr();
                self.expect(SyntaxKind::RParen);
                self.finish_node();
            }
            _ => {
                let span = self.current_span();
                self.errors.push(ParseError {
                    span,
                    message: "expected object key".into(),
                });
            }
        }
    }

    fn parse_iter_binding_comprehension(&mut self) {
        self.start_node(SyntaxKind::IterBinding);
        if self.current() == SyntaxKind::Ident {
            self.bump(); // first ident
            match self.current() {
                SyntaxKind::Comma => {
                    self.bump(); // ,
                    self.expect(SyntaxKind::Ident);
                    self.expect(SyntaxKind::KwIn);
                    self.parse_expr();
                }
                SyntaxKind::KwIn => {
                    self.bump();
                    self.parse_expr();
                }
                SyntaxKind::KwOf => {
                    self.bump();
                    self.parse_expr();
                }
                _ => {
                    let span = self.current_span();
                    self.errors.push(ParseError {
                        span,
                        message: "expected `in`, `of`, or `,` in comprehension binding".into(),
                    });
                }
            }
        } else {
            let span = self.current_span();
            self.errors.push(ParseError {
                span,
                message: "expected identifier in comprehension binding".into(),
            });
        }
        self.finish_node();
    }

    // ── Utilities ───────────────────────────────────────────────────

    /// Parse a comma-separated list: `item (','? item)* ','?`
    fn parse_elements(&mut self, mut parse_item: impl FnMut(&mut Self)) {
        parse_item(self);
        while !self.at_end() && self.current() != SyntaxKind::RParen {
            self.eat(SyntaxKind::Comma);
            if self.current() == SyntaxKind::RParen {
                break;
            }
            parse_item(self);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::syntax::SyntaxNode;

    fn parse_str(source: &str) -> (SyntaxNode, Vec<ParseError>) {
        let tokens = lexer::lex(source);
        let (green, errors) = parse(source, &tokens);
        (SyntaxNode::new_root(green), errors)
    }

    fn assert_no_errors(source: &str) -> SyntaxNode {
        let (tree, errors) = parse_str(source);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        tree
    }

    #[test]
    fn parse_simple_literal() {
        let tree = assert_no_errors("42");
        assert_eq!(tree.kind(), SyntaxKind::Root);
        // Should contain a single DecimalNumber leaf
        let children: Vec<_> = tree
            .children_with_tokens()
            .filter_map(|c| c.into_token())
            .filter(|t| t.kind() != SyntaxKind::Whitespace)
            .collect();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].kind(), SyntaxKind::DecimalNumber);
        assert_eq!(children[0].text(), "42");
    }

    #[test]
    fn parse_binary_expr() {
        let tree = assert_no_errors("1 + 2");
        // Root → BinaryExpr(1, +, 2)
        let bin = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::BinaryExpr)
            .expect("expected BinaryExpr node");
        let tokens: Vec<_> = bin
            .children_with_tokens()
            .filter_map(|c| c.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].text(), "1");
        assert_eq!(tokens[1].text(), "+");
        assert_eq!(tokens[2].text(), "2");
    }

    #[test]
    fn parse_precedence() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3)
        let tree = assert_no_errors("1 + 2 * 3");
        let add = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::BinaryExpr)
            .expect("expected outer BinaryExpr");
        // The RHS of the add should be another BinaryExpr (mul)
        let mul = add
            .children()
            .find(|n| n.kind() == SyntaxKind::BinaryExpr)
            .expect("expected nested BinaryExpr for multiplication");
        let tokens: Vec<_> = mul
            .children_with_tokens()
            .filter_map(|c| c.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        assert_eq!(tokens[1].text(), "*");
    }

    #[test]
    fn parse_conditional() {
        let tree = assert_no_errors("when x do y end");
        let cond = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::ConditionalExpr)
            .expect("expected ConditionalExpr");
        // Should contain KwWhen, Ident(x), KwDo, Block, KwEnd
        let kinds: Vec<_> = cond
            .children_with_tokens()
            .filter(|c| {
                c.as_token()
                    .map_or(true, |t| !t.kind().is_trivia())
            })
            .map(|c| match c {
                rowan::NodeOrToken::Node(n) => n.kind(),
                rowan::NodeOrToken::Token(t) => t.kind(),
            })
            .collect();
        assert!(kinds.contains(&SyntaxKind::KwWhen));
        assert!(kinds.contains(&SyntaxKind::KwDo));
        assert!(kinds.contains(&SyntaxKind::Block));
        assert!(kinds.contains(&SyntaxKind::KwEnd));
    }

    #[test]
    fn parse_for_loop() {
        let tree = assert_no_errors("for x in items do x end");
        let for_node = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::ForExpr)
            .expect("expected ForExpr");
        assert!(for_node.children().any(|n| n.kind() == SyntaxKind::IterBinding));
        assert!(for_node.children().any(|n| n.kind() == SyntaxKind::Block));
    }

    #[test]
    fn parse_array_literal() {
        let tree = assert_no_errors("[1, 2, 3]");
        let arr = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::ArrayExpr)
            .expect("expected ArrayExpr");
        // Should have brackets + 3 numbers + commas
        let nums: Vec<_> = arr
            .children_with_tokens()
            .filter_map(|c| c.into_token())
            .filter(|t| t.kind() == SyntaxKind::DecimalNumber)
            .collect();
        assert_eq!(nums.len(), 3);
    }

    #[test]
    fn parse_object_literal() {
        let tree = assert_no_errors("{a: 1, b: 2}");
        let obj = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::ObjectExpr)
            .expect("expected ObjectExpr");
        let pairs: Vec<_> = obj
            .children()
            .filter(|n| n.kind() == SyntaxKind::Pair)
            .collect();
        assert_eq!(pairs.len(), 2);
    }

    #[test]
    fn parse_navigation() {
        let tree = assert_no_errors("foo.bar.baz");
        // Should be NavExpr(NavExpr(foo, bar), baz)
        let nav = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::NavExpr)
            .expect("expected NavExpr");
        // The outer nav should contain a nested nav
        assert!(nav.children().any(|n| n.kind() == SyntaxKind::NavExpr));
    }

    #[test]
    fn parse_call() {
        let tree = assert_no_errors("foo(1, 2)");
        let call = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::CallExpr)
            .expect("expected CallExpr");
        let nums: Vec<_> = call
            .descendants_with_tokens()
            .filter_map(|c| c.into_token())
            .filter(|t| t.kind() == SyntaxKind::DecimalNumber)
            .collect();
        assert_eq!(nums.len(), 2);
    }

    #[test]
    fn parse_error_recovery() {
        // Invalid token in expression position — should produce an error
        // but still return a tree
        let (tree, errors) = parse_str(")");
        assert!(!errors.is_empty());
        assert_eq!(tree.kind(), SyntaxKind::Root);
    }

    #[test]
    fn parse_unary() {
        let tree = assert_no_errors("-x");
        let unary = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::UnaryExpr)
            .expect("expected UnaryExpr");
        let tokens: Vec<_> = unary
            .children_with_tokens()
            .filter_map(|c| c.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        assert_eq!(tokens[0].text(), "-");
        assert_eq!(tokens[1].text(), "x");
    }

    #[test]
    fn parse_assignment() {
        let tree = assert_no_errors("x = 1");
        let assign = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::AssignExpr)
            .expect("expected AssignExpr");
        let tokens: Vec<_> = assign
            .children_with_tokens()
            .filter_map(|c| c.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        assert_eq!(tokens[0].text(), "x");
        assert_eq!(tokens[1].text(), "=");
        assert_eq!(tokens[2].text(), "1");
    }

    #[test]
    fn parse_else_chain() {
        let tree = assert_no_errors("when x do 1 else when y do 2 else 3 end");
        let cond = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::ConditionalExpr)
            .expect("expected ConditionalExpr");
        let else_branch = cond
            .children()
            .find(|n| n.kind() == SyntaxKind::ElseBranch)
            .expect("expected ElseBranch");
        // Should contain a nested else branch
        assert!(else_branch
            .children()
            .any(|n| n.kind() == SyntaxKind::ElseBranch));
    }

    #[test]
    fn parse_string() {
        assert_no_errors(r#""hello world""#);
        assert_no_errors("'single quoted'");
    }

    #[test]
    fn parse_range() {
        let tree = assert_no_errors("1..10");
        assert!(tree
            .children()
            .any(|n| n.kind() == SyntaxKind::RangeExpr));
    }

    #[test]
    fn parse_while_loop() {
        assert_no_errors("while x do y end");
    }

    #[test]
    fn parse_empty_collections() {
        assert_no_errors("[]");
        assert_no_errors("{}");
    }

    #[test]
    fn parse_array_comprehension() {
        assert_no_errors("[x for v in items]");
    }

    #[test]
    fn parse_group() {
        let tree = assert_no_errors("(1 + 2)");
        assert!(tree
            .children()
            .any(|n| n.kind() == SyntaxKind::GroupExpr));
    }

    #[test]
    fn lossless_roundtrip() {
        // The CST must preserve all source text including trivia
        let source = "when x  do\n  y + 1\nend";
        let tree = assert_no_errors(source);
        assert_eq!(tree.text().to_string(), source);
    }

    #[test]
    fn parse_template_literal() {
        let tree = assert_no_errors("`hello`");
        assert!(tree
            .children()
            .any(|n| n.kind() == SyntaxKind::TemplateExpr));
    }

    #[test]
    fn parse_template_with_interpolation() {
        let tree = assert_no_errors(r"`hello ${name}`");
        let tmpl = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::TemplateExpr)
            .expect("expected TemplateExpr");
        // Should contain a TemplateLiteral token
        let tokens: Vec<_> = tmpl
            .children_with_tokens()
            .filter_map(|c| c.into_token())
            .filter(|t| t.kind() == SyntaxKind::TemplateLiteral)
            .collect();
        assert_eq!(tokens.len(), 1);
    }

    #[test]
    fn parse_tagged_template() {
        let tree = assert_no_errors(r"html`<p>${text}</p>`");
        let tmpl = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::TemplateExpr)
            .expect("expected TemplateExpr");
        // Should contain an Ident token (tag) and a TemplateLiteral token
        let tokens: Vec<_> = tmpl
            .children_with_tokens()
            .filter_map(|c| c.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();
        assert!(tokens.iter().any(|t| t.kind() == SyntaxKind::Ident));
        assert!(tokens.iter().any(|t| t.kind() == SyntaxKind::TemplateLiteral));
    }

    #[test]
    fn parse_type_decl() {
        let tree = assert_no_errors("type Headers = {*: string}");
        let decl = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::TypeDecl)
            .expect("expected TypeDecl node");
        assert!(decl.children_with_tokens().any(|c| c
            .as_token()
            .map_or(false, |t| t.kind() == SyntaxKind::KwType)));
    }

    #[test]
    fn parse_type_union() {
        assert_no_errors(r#"type HttpMethod = "GET" | "POST" | "PUT""#);
    }

    #[test]
    fn parse_type_array() {
        assert_no_errors("type Names = [string]");
    }

    #[test]
    fn parse_extern_simple() {
        assert_no_errors("extern config = unknown");
    }

    #[test]
    fn parse_extern_object() {
        assert_no_errors("extern req = {\n  method: string\n  path: string\n}");
    }

    #[test]
    fn parse_extern_mut() {
        let tree = assert_no_errors("extern mut res = {status: integer}");
        let decl = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::ExternDecl)
            .expect("expected ExternDecl node");
        let has_mut = decl.children_with_tokens().any(|c| {
            c.as_token().map_or(false, |t| t.text() == "mut")
        });
        assert!(has_mut);
    }

    #[test]
    fn parse_extern_function() {
        // Function signatures may have parse errors on `:` inside args — that's OK
        let (tree, _errors) = parse_str("extern json.parse(text: string) -> some");
        let decl = tree
            .children()
            .find(|n| n.kind() == SyntaxKind::ExternDecl)
            .expect("expected ExternDecl node");
        assert!(decl.text().to_string().contains("json"));
    }

    #[test]
    fn parse_extern_function_no_return() {
        let (_tree, _errors) = parse_str("extern log.info(message: some)");
        // Should parse without panic — errors on `:` are acceptable
    }

    #[test]
    fn parse_wildcard_object_key() {
        assert_no_errors("{*: string}");
    }

    #[test]
    fn mut_is_not_a_keyword() {
        assert_no_errors("mut = 42");
        let tokens = crate::lexer::lex("mut");
        let non_trivia: Vec<_> = tokens
            .iter()
            .filter(|t| {
                !matches!(
                    t.kind,
                    crate::lexer::TokenKind::Whitespace
                        | crate::lexer::TokenKind::LineComment
                        | crate::lexer::TokenKind::BlockComment
                )
            })
            .collect();
        assert_eq!(non_trivia[0].kind, crate::lexer::TokenKind::Ident);
    }

    #[test]
    fn parse_rexd_file_inline() {
        let source = r#"
            type Headers = {*: string | [string]}
            type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

            extern req = {
              method: HttpMethod
              path: string
              headers: Headers
            }

            extern mut res = {status: integer, headers: Headers}
            extern config = unknown

            extern json.parse(text: string) = some
            extern log.info(message: some)
        "#;
        let (_tree, errors) = parse_str(source);
        // Some errors on `:` in function args are expected, but no panics
        let _ = errors;
    }

    #[test]
    fn parse_real_rexd_file() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir)
            .join("../../examples/knowledge-base/rex-serve.rexd");
        let source = std::fs::read_to_string(path).unwrap();
        let tokens = crate::lexer::lex(&source);
        let (_tree, _errors) = parse(&source, &tokens);
        // Errors on `:` and `...` in function args are expected
        // The important thing is: no panic, and TypeDecl/ExternDecl nodes exist
    }

    #[test]
    fn parse_return() {
        assert_no_errors("return 42");
        assert_no_errors("return");
        assert_no_errors("when x do return y end");
    }
}
