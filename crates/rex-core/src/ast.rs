//! Typed AST wrappers over the untyped rowan CST.
//!
//! Each struct wraps a `SyntaxNode` and provides typed accessors for its
//! children. Constructing these is free — they just borrow the green tree.

use crate::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

// ── Helpers ─────────────────────────────────────────────────────────────

/// Try to cast a `SyntaxNode` to a typed AST node.
macro_rules! ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl $name {
            pub fn cast(node: SyntaxNode) -> Option<Self> {
                if node.kind() == SyntaxKind::$kind {
                    Some(Self(node))
                } else {
                    None
                }
            }

            pub fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

/// Find the first child token of a given kind, skipping trivia.
fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(|c| c.into_token())
        .find(|t| t.kind() == kind)
}

/// Find the first child node of a given kind.
fn child_node(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    parent.children().find(|n| n.kind() == kind)
}

/// Collect all non-trivia child tokens.
fn non_trivia_tokens(parent: &SyntaxNode) -> impl Iterator<Item = SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(|c| c.into_token())
        .filter(|t| !t.kind().is_trivia())
}

// ── Node types ──────────────────────────────────────────────────────────

ast_node!(BinaryExpr, BinaryExpr);

impl BinaryExpr {
    pub fn lhs(&self) -> Option<SyntaxNode> {
        self.0
            .children_with_tokens()
            .filter(|c| {
                c.as_token().map_or(true, |t| !t.kind().is_trivia())
            })
            .find_map(|c| c.into_node())
    }

    pub fn op(&self) -> Option<SyntaxToken> {
        non_trivia_tokens(&self.0).find(|t| {
            matches!(
                t.kind(),
                SyntaxKind::Plus
                    | SyntaxKind::Minus
                    | SyntaxKind::Star
                    | SyntaxKind::Slash
                    | SyntaxKind::Percent
                    | SyntaxKind::Amp
                    | SyntaxKind::Pipe
                    | SyntaxKind::Caret
                    | SyntaxKind::EqEq
                    | SyntaxKind::BangEq
                    | SyntaxKind::Gt
                    | SyntaxKind::GtEq
                    | SyntaxKind::Lt
                    | SyntaxKind::LtEq
                    | SyntaxKind::KwAnd
                    | SyntaxKind::KwOr
            )
        })
    }

    pub fn rhs(&self) -> Option<SyntaxNode> {
        let mut nodes = self.0.children();
        nodes.next(); // skip lhs
        nodes.next() // rhs
    }
}

ast_node!(UnaryExpr, UnaryExpr);

impl UnaryExpr {
    pub fn op(&self) -> Option<SyntaxToken> {
        non_trivia_tokens(&self.0).next()
    }

    pub fn operand(&self) -> Option<SyntaxNode> {
        self.0.children().next()
    }
}

ast_node!(AssignExpr, AssignExpr);

impl AssignExpr {
    pub fn op(&self) -> Option<SyntaxToken> {
        non_trivia_tokens(&self.0).find(|t| {
            matches!(
                t.kind(),
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
        })
    }
}

ast_node!(RangeExpr, RangeExpr);
ast_node!(CallExpr, CallExpr);
ast_node!(NavExpr, NavExpr);
ast_node!(GroupExpr, GroupExpr);
ast_node!(SelfExpr, SelfExpr);

ast_node!(ConditionalExpr, ConditionalExpr);

impl ConditionalExpr {
    pub fn head_keyword(&self) -> Option<SyntaxToken> {
        non_trivia_tokens(&self.0)
            .find(|t| matches!(t.kind(), SyntaxKind::KwWhen | SyntaxKind::KwUnless))
    }

    pub fn block(&self) -> Option<Block> {
        child_node(&self.0, SyntaxKind::Block).and_then(Block::cast)
    }

    pub fn else_branch(&self) -> Option<ElseBranch> {
        child_node(&self.0, SyntaxKind::ElseBranch).and_then(ElseBranch::cast)
    }
}

ast_node!(ElseBranch, ElseBranch);

impl ElseBranch {
    pub fn block(&self) -> Option<Block> {
        child_node(&self.0, SyntaxKind::Block).and_then(Block::cast)
    }

    pub fn nested_else(&self) -> Option<ElseBranch> {
        child_node(&self.0, SyntaxKind::ElseBranch).and_then(ElseBranch::cast)
    }
}

ast_node!(ForExpr, ForExpr);

impl ForExpr {
    pub fn binding(&self) -> Option<IterBinding> {
        child_node(&self.0, SyntaxKind::IterBinding).and_then(IterBinding::cast)
    }

    pub fn block(&self) -> Option<Block> {
        child_node(&self.0, SyntaxKind::Block).and_then(Block::cast)
    }
}

ast_node!(WhileExpr, WhileExpr);

impl WhileExpr {
    pub fn block(&self) -> Option<Block> {
        child_node(&self.0, SyntaxKind::Block).and_then(Block::cast)
    }
}

ast_node!(Block, Block);
ast_node!(ArrayExpr, ArrayExpr);
ast_node!(ArrayComprehension, ArrayComprehension);
ast_node!(ObjectExpr, ObjectExpr);
ast_node!(ObjectComprehension, ObjectComprehension);

ast_node!(Pair, Pair);

impl Pair {
    pub fn colon(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::Colon)
    }
}

ast_node!(IterBinding, IterBinding);
ast_node!(ReturnExpr, ReturnExpr);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn parse_and_cast<T>(
        source: &str,
        cast: fn(SyntaxNode) -> Option<T>,
    ) -> T {
        let tokens = lexer::lex(source);
        let (green, errors) = parser::parse(source, &tokens);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let root = SyntaxNode::new_root(green);
        root.children()
            .find_map(cast)
            .expect("expected matching AST node")
    }

    #[test]
    fn binary_expr_accessors() {
        let bin = parse_and_cast("1 + 2", BinaryExpr::cast);
        assert_eq!(bin.op().unwrap().text(), "+");
    }

    #[test]
    fn conditional_accessors() {
        let cond = parse_and_cast("when x do y end", ConditionalExpr::cast);
        assert_eq!(cond.head_keyword().unwrap().text(), "when");
        assert!(cond.block().is_some());
        assert!(cond.else_branch().is_none());
    }

    #[test]
    fn conditional_with_else() {
        let cond = parse_and_cast("when x do 1 else 2 end", ConditionalExpr::cast);
        let else_br = cond.else_branch().unwrap();
        assert!(else_br.block().is_some());
    }

    #[test]
    fn for_expr_accessors() {
        let f = parse_and_cast("for x in items do x end", ForExpr::cast);
        assert!(f.binding().is_some());
        assert!(f.block().is_some());
    }

    #[test]
    fn object_pair_accessors() {
        let obj = parse_and_cast("{a: 1}", ObjectExpr::cast);
        let pair: Pair = obj
            .syntax()
            .children()
            .find_map(Pair::cast)
            .unwrap();
        assert!(pair.colon().is_some());
    }
}
