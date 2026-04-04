//! CST-based formatter for Rex source code.
//!
//! Walks the rowan syntax tree directly, preserving all tokens (comments,
//! type annotations, extern declarations, dynamic navigation) while
//! normalizing horizontal whitespace. Vertical layout (line breaks) is
//! preserved from the original source.
//!
//! ## What it does
//! - Normalizes spaces around operators, after colons/commas, around keywords
//! - Strips commas from objects (Rex style: space-separated pairs)
//! - Normalizes indentation to 2 spaces per nesting level
//! - Ensures trailing newline
//!
//! ## What it doesn't do
//! - Won't split a single line into multiple lines
//! - Won't join multiple lines into one
//! - Won't add or remove blank lines

use crate::syntax::{SyntaxKind as SK, SyntaxNode, SyntaxToken};

pub fn format(source: &str) -> String {
    let tokens = crate::lexer::lex(source);
    let (green, _) = crate::parser::parse(source, &tokens);
    let root = SyntaxNode::new_root(green);
    let mut f = Formatter { out: String::new(), indent: 0, at_line_start: true };
    f.walk(&root);
    // Ensure trailing newline
    let trimmed = f.out.trim_end_matches('\n');
    let mut result = trimmed.to_string();
    result.push('\n');
    result
}

struct Formatter {
    out: String,
    indent: usize,
    at_line_start: bool,
}

impl Formatter {
    fn emit(&mut self, text: &str) {
        if text.is_empty() { return; }
        if self.at_line_start {
            for _ in 0..self.indent { self.out.push_str("  "); }
            self.at_line_start = false;
        }
        self.out.push_str(text);
    }

    fn newline(&mut self) {
        self.out.push('\n');
        self.at_line_start = true;
    }

    /// Walk a node, dispatching to the appropriate formatter.
    fn walk(&mut self, node: &SyntaxNode) {
        match node.kind() {
            SK::Root => self.root(node),
            SK::Block => self.block(node),
            SK::BinaryExpr | SK::RangeExpr => self.binary(node),
            SK::UnaryExpr => self.unary(node),
            SK::AssignExpr => self.assign(node),
            SK::ReturnExpr | SK::TypeDecl | SK::ExternDecl => self.spaced_punct(node),
            SK::CallExpr => self.call(node),
            SK::NavExpr | SK::GroupExpr => self.tight(node),
            SK::ConditionalExpr | SK::ForExpr | SK::WhileExpr => self.block_kw(node),
            SK::ElseBranch => self.else_branch(node),
            SK::ArrayExpr | SK::IndexedArrayExpr => self.array(node),
            SK::ObjectExpr | SK::IndexedObjectExpr => self.object(node),
            SK::ArrayComprehension | SK::ObjectComprehension => self.comprehension(node),
            SK::Pair => self.pair(node),
            SK::SpreadExpr => self.spread(node),
            SK::IterBinding => self.iter_binding(node),
            SK::CompoundExpr => self.compound(node),
            _ => self.emit(&node.text().to_string()),
        }
    }

    fn child(&mut self, c: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>) {
        match c {
            rowan::NodeOrToken::Token(t) => self.emit(t.text()),
            rowan::NodeOrToken::Node(n) => self.walk(n),
        }
    }

    // ── Root & Block: line-separated statements ────────────────────
    // Preserve original line structure. Normalize indentation.

    fn root(&mut self, node: &SyntaxNode) {
        self.line_items(node);
    }

    fn block(&mut self, node: &SyntaxNode) {
        self.line_items(node);
    }

    /// Emit children line by line, preserving blank lines and comments.
    /// Multiple blank lines are collapsed to at most one.
    fn line_items(&mut self, node: &SyntaxNode) {
        let mut first_on_line = true;

        for c in node.children_with_tokens() {
            let kind = c_kind(&c);

            if kind == SK::Whitespace {
                let text = c.as_token().unwrap().text();
                let nl = text.chars().filter(|&ch| ch == '\n').count();
                let emit_nl = nl.min(2);
                for _ in 0..emit_nl {
                    self.newline();
                    first_on_line = true;
                }
                continue;
            }

            if kind == SK::LineComment {
                if !first_on_line { self.emit(" "); }
                self.emit(c.as_token().unwrap().text().trim_end_matches('\n'));
                // Line comments consume their trailing newline, so emit one
                self.newline();
                first_on_line = true;
                continue;
            }
            if kind == SK::BlockComment {
                if !first_on_line { self.emit(" "); }
                self.emit(c.as_token().unwrap().text());
                first_on_line = false;
                continue;
            }

            // Non-trivia
            if !first_on_line { self.emit(" "); }
            self.child(&c);
            first_on_line = false;
        }
    }

    // ── Compound: a; b; c ────────────────────────────────────────────

    fn compound(&mut self, node: &SyntaxNode) {
        for c in ntc(node) {
            if let rowan::NodeOrToken::Token(t) = &c {
                if t.kind() == SK::Semicolon { self.emit("; "); continue; }
            }
            self.child(&c);
        }
    }

    // ── Binary: a + b ──────────────────────────────────────────────

    fn binary(&mut self, node: &SyntaxNode) {
        let mut first = true;
        for c in ntc(node) {
            if !first { self.emit(" "); }
            first = false;
            self.child(&c);
        }
    }

    // ── Unary: -x, ~x, delete x ───────────────────────────────────

    fn unary(&mut self, node: &SyntaxNode) {
        let items: Vec<_> = ntc(node).collect();
        for (i, c) in items.iter().enumerate() {
            if i == 1 {
                // Space after keyword prefix (delete), tight after symbol prefix (- ~)
                if let rowan::NodeOrToken::Token(t) = &items[0] {
                    if t.kind() == SK::KwDelete { self.emit(" "); }
                }
            }
            self.child(c);
        }
    }

    // ── Assignment: x = 1, x: T = 1, x += 1 ──────────────────────

    fn assign(&mut self, node: &SyntaxNode) {
        let mut after_punct = false;
        for (i, c) in ntc(node).enumerate() {
            if let rowan::NodeOrToken::Token(t) = &c {
                match t.kind() {
                    SK::Colon => { self.emit(": "); after_punct = true; continue; }
                    k if is_assign_op(k) => {
                        self.emit(" "); self.emit(t.text()); self.emit(" ");
                        after_punct = true; continue;
                    }
                    _ => {}
                }
            }
            if i > 0 && !after_punct { self.emit(" "); }
            after_punct = false;
            self.child(&c);
        }
    }

    // ── Spaced with punctuation: return, decl, iter-binding ────────
    // Space between children, but `: `, ` = `, ` -> `, `, ` get special treatment.

    fn spaced_punct(&mut self, node: &SyntaxNode) {
        let mut after_punct = false;
        for (i, c) in ntc(node).enumerate() {
            if let rowan::NodeOrToken::Token(t) = &c {
                match t.kind() {
                    SK::Colon => { self.emit(":"); after_punct = true; continue; }
                    SK::Eq => { self.emit(" = "); after_punct = true; continue; }
                    SK::Arrow => { self.emit(" -> "); after_punct = true; continue; }
                    SK::Comma => { self.emit(", "); after_punct = true; continue; }
                    SK::KwIn | SK::KwOf => {
                        self.emit(" "); self.emit(t.text()); self.emit(" ");
                        after_punct = true; continue;
                    }
                    _ => {}
                }
            }
            if i > 0 && !after_punct { self.emit(" "); }
            after_punct = false;
            self.child(&c);
        }
    }

    // ── Call: f(a, b) — tight around parens, comma-space between args

    fn call(&mut self, node: &SyntaxNode) {
        for c in ntc(node) {
            if let rowan::NodeOrToken::Token(t) = &c {
                if t.kind() == SK::Comma { self.emit(", "); continue; }
            }
            self.child(&c);
        }
    }

    // ── Tight: no spaces (navigation, group) ───────────────────────

    fn tight(&mut self, node: &SyntaxNode) {
        for c in ntc(node) { self.child(&c); }
    }

    // ── Block keywords: when/unless/for/while ──────────────────────
    // Each section (header, body, else) decides independently whether
    // it's multiline based on its own content. `do` and `end` are
    // section boundaries.

    fn block_kw(&mut self, node: &SyntaxNode) {
        // The body is multiline if there are any newlines between `do` and `end`
        // (checked by looking at the full node text — if the Block or surrounding
        // whitespace contains newlines, the body section is multiline)
        let body_multiline = {
            let mut saw_do = false;
            let mut has_nl = false;
            for c in node.children_with_tokens() {
                let kind = c_kind(&c);
                if kind == SK::KwDo { saw_do = true; continue; }
                if saw_do && kind == SK::KwEnd { break; }
                if saw_do {
                    if let Some(t) = c.as_token() {
                        if t.text().contains('\n') { has_nl = true; break; }
                    } else if is_multiline(c.as_node().unwrap()) {
                        has_nl = true; break;
                    }
                }
            }
            has_nl
        };

        for c in node.children_with_tokens() {
            let kind = c_kind(&c);

            if kind == SK::Whitespace {
                // Between sections (e.g. after `do`, before `end`), whitespace
                // is handled by the do/end logic. Skip it.
                continue;
            }

            if kind == SK::LineComment {
                self.emit(" ");
                self.emit(c.as_token().unwrap().text().trim_end_matches('\n'));
                if body_multiline { self.newline(); }
                continue;
            }
            if kind == SK::BlockComment {
                self.emit(" ");
                self.emit(c.as_token().unwrap().text());
                continue;
            }

            match &c {
                rowan::NodeOrToken::Token(t) => match t.kind() {
                    SK::KwWhen | SK::KwUnless | SK::KwFor | SK::KwWhile => {
                        self.emit(t.text()); self.emit(" ");
                    }
                    SK::KwDo => {
                        self.emit(" do");
                        if body_multiline {
                            self.indent += 1;
                            self.newline();
                        } else {
                            self.emit(" ");
                        }
                    }
                    SK::KwEnd => {
                        if body_multiline {
                            if self.indent > 0 { self.indent -= 1; }
                            self.newline();
                        } else {
                            self.emit(" ");
                        }
                        self.emit("end");
                    }
                    _ => self.emit(t.text()),
                }
                rowan::NodeOrToken::Node(n) => {
                    if n.kind() == SK::ElseBranch { self.else_branch(n); }
                    else if n.kind() == SK::Block { self.block(n); }
                    else { self.walk(n); }
                }
            }
        }
    }

    // ── Else branch ────────────────────────────────────────────────

    fn else_branch(&mut self, node: &SyntaxNode) {
        // Check if there are newlines between the last keyword and the Block
        let block_multiline = has_newlines_around_block(node);
        let has_do = node.children_with_tokens()
            .any(|c| c.as_token().map_or(false, |t| t.kind() == SK::KwDo));

        for c in ntc(node) {
            match &c {
                rowan::NodeOrToken::Token(t) => match t.kind() {
                    SK::KwElse => {
                        if self.indent > 0 { self.indent -= 1; }
                        self.newline();
                        self.emit("else");
                    }
                    SK::KwWhen | SK::KwUnless => {
                        self.emit(" "); self.emit(t.text()); self.emit(" ");
                    }
                    SK::KwDo => {
                        self.emit(" do");
                        if block_multiline {
                            self.indent += 1;
                            self.newline();
                        } else {
                            self.emit(" ");
                        }
                    }
                    _ => self.emit(t.text()),
                }
                rowan::NodeOrToken::Node(n) if n.kind() == SK::Block => {
                    if !has_do {
                        if block_multiline {
                            self.indent += 1;
                            self.newline();
                        } else {
                            self.emit(" ");
                        }
                    }
                    self.block(n);
                }
                _ => self.child(&c),
            }
        }
    }

    // ── Array: [a, b, c] or multiline ──────────────────────────────

    fn array(&mut self, node: &SyntaxNode) {
        let indexed = node.kind() == SK::IndexedArrayExpr;
        if is_multiline(node) {
            self.collection_multiline(node, if indexed { "[#" } else { "[" }, "]");
        } else {
            let has_commas = node.children_with_tokens()
                .any(|c| c.as_token().map_or(false, |t| t.kind() == SK::Comma));
            let sep = if has_commas { ", " } else { " " };
            let items: Vec<_> = ntc(node)
                .filter(|c| !is_bracket(c_kind(c)) && c_kind(c) != SK::Comma && c_kind(c) != SK::Hash)
                .collect();
            if items.is_empty() {
                self.emit(if indexed { "[#]" } else { "[]" });
            } else {
                self.emit(if indexed { "[# " } else { "[ " });
                for (i, c) in items.iter().enumerate() {
                    if i > 0 { self.emit(sep); }
                    self.child(c);
                }
                self.emit(" ]");
            }
        }
    }

    // ── Object: {a: 1 b: 2} or multiline ──────────────────────────

    fn object(&mut self, node: &SyntaxNode) {
        let indexed = node.kind() == SK::IndexedObjectExpr;
        if is_multiline(node) {
            self.collection_multiline(node, if indexed { "{#" } else { "{" }, "}");
        } else {
            let items: Vec<_> = node.children()
                .filter(|n| n.kind() == SK::Pair || n.kind() == SK::SpreadExpr)
                .collect();
            if items.is_empty() {
                self.emit(if indexed { "{#}" } else { "{}" });
            } else {
                self.emit(if indexed { "{# " } else { "{ " });
                for (i, n) in items.iter().enumerate() {
                    if i > 0 { self.emit(" "); }
                    if n.kind() == SK::SpreadExpr { self.spread(n); }
                    else { self.pair(n); }
                }
                self.emit(" }");
            }
        }
    }

    /// Multiline collection — preserve line structure, normalize indentation.
    fn collection_multiline(&mut self, node: &SyntaxNode, _open: &str, _close: &str) {
        for c in node.children_with_tokens() {
            let kind = c_kind(&c);
            if kind == SK::Whitespace {
                let nl = c.as_token().unwrap().text().chars().filter(|&ch| ch == '\n').count();
                let emit_nl = nl.min(2);
                for _ in 0..emit_nl { self.newline(); }
                continue;
            }
            if kind == SK::LineComment {
                self.emit(c.as_token().unwrap().text().trim_end_matches('\n'));
                self.newline();
                continue;
            }
            if kind == SK::BlockComment {
                self.emit(c.as_token().unwrap().text());
                continue;
            }
            if kind == SK::Comma || kind == SK::Hash { continue; }
            if kind == SK::LBracket || kind == SK::LBrace {
                let indexed = matches!(node.kind(), SK::IndexedArrayExpr | SK::IndexedObjectExpr);
                self.emit(c.as_token().unwrap().text());
                if indexed { self.emit("#"); }
                self.indent += 1;
                // For indexed containers, skip the Hash when checking for a newline
                let has_nl = if indexed { next_has_newline_skip(node, &c, 1) } else { next_has_newline(node, &c) };
                if !has_nl { self.emit(" "); }
                continue;
            }
            if kind == SK::RBracket || kind == SK::RBrace {
                self.indent -= 1;
                if !self.at_line_start { self.emit(" "); }
                self.emit(c.as_token().unwrap().text());
                continue;
            }
            self.child(&c);
        }
    }

    // ── Pair: key:value ────────────────────────────────────────────

    fn pair(&mut self, node: &SyntaxNode) {
        let mut after_colon = false;
        for (i, c) in ntc(node).enumerate() {
            if let rowan::NodeOrToken::Token(t) = &c {
                if t.kind() == SK::Colon { self.emit(":"); after_colon = true; continue; }
            }
            if i > 0 && !after_colon { self.emit(" "); }
            after_colon = false;
            self.child(&c);
        }
    }

    fn iter_binding(&mut self, node: &SyntaxNode) {
        let mut first = true;
        for c in ntc(node) {
            if c.as_token().map_or(false, |t| t.kind() == SK::Comma) { continue; }
            if let rowan::NodeOrToken::Token(t) = &c {
                match t.kind() {
                    SK::KwIn | SK::KwOf => {
                        self.emit(" "); self.emit(t.text()); self.emit(" ");
                        first = true; continue;
                    }
                    _ => {}
                }
            }
            if !first { self.emit(" "); }
            first = false;
            self.child(&c);
        }
    }

    fn spread(&mut self, node: &SyntaxNode) {
        self.emit("...");
        for c in ntc(node) {
            if c.as_token().map_or(false, |t| t.kind() == SK::DotDotDot) { continue; }
            self.child(&c);
        }
    }

    // ── Comprehension: [expr for v in items] ───────────────────────

    fn comprehension(&mut self, node: &SyntaxNode) {
        if is_multiline(node) {
            self.comprehension_multiline(node);
        } else {
            self.comprehension_inline(node);
        }
    }

    fn comprehension_inline(&mut self, node: &SyntaxNode) {
        let (open, close) = if node.kind() == SK::ArrayComprehension { ("[ ", " ]") } else { ("{ ", " }") };
        self.emit(open);
        let items: Vec<_> = ntc(node).filter(|c| !is_bracket(c_kind(c))).collect();
        let mut after_colon = false;
        for (i, c) in items.iter().enumerate() {
            if let rowan::NodeOrToken::Token(t) = c {
                if t.kind() == SK::Colon { self.emit(": "); after_colon = true; continue; }
            }
            if i > 0 && !after_colon { self.emit(" "); }
            after_colon = false;
            self.child(c);
        }
        self.emit(close);
    }

    fn comprehension_multiline(&mut self, node: &SyntaxNode) {
        // Walk children preserving newlines, like line_items but with bracket handling
        for c in node.children_with_tokens() {
            let kind = c_kind(&c);

            if kind == SK::Whitespace {
                let nl = c.as_token().unwrap().text().chars().filter(|&ch| ch == '\n').count();
                let emit_nl = nl.min(2);
                for _ in 0..emit_nl { self.newline(); }
                continue;
            }
            if kind == SK::LineComment {
                self.emit(c.as_token().unwrap().text().trim_end_matches('\n'));
                self.newline();
                continue;
            }
            if kind == SK::BlockComment {
                self.emit(c.as_token().unwrap().text());
                continue;
            }
            if kind == SK::LBracket || kind == SK::LBrace {
                self.emit(c.as_token().unwrap().text());
                self.indent += 1;
                if !next_has_newline(node, &c) { self.emit(" "); }
                continue;
            }
            if kind == SK::RBracket || kind == SK::RBrace {
                self.indent -= 1;
                if !self.at_line_start { self.emit(" "); }
                self.emit(c.as_token().unwrap().text());
                continue;
            }
            if kind == SK::Comma { continue; }

            // Keywords like `for`, `while`, `in`, `of` need a space after them
            if let rowan::NodeOrToken::Token(t) = &c {
                match t.kind() {
                    SK::KwFor | SK::KwWhile | SK::KwIn | SK::KwOf => {
                        self.emit(t.text());
                        self.emit(" ");
                        continue;
                    }
                    SK::Colon => {
                        self.emit(": ");
                        continue;
                    }
                    _ => {}
                }
            }
            self.child(&c);
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn ntc(node: &SyntaxNode) -> impl Iterator<Item = rowan::NodeOrToken<SyntaxNode, SyntaxToken>> + '_ {
    node.children_with_tokens().filter(|c| !c.as_token().map_or(false, |t| t.kind().is_trivia()))
}

fn c_kind(c: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>) -> SK {
    match c { rowan::NodeOrToken::Token(t) => t.kind(), rowan::NodeOrToken::Node(n) => n.kind() }
}

fn is_assign_op(kind: SK) -> bool {
    matches!(kind, SK::Eq | SK::ColonEq | SK::PlusEq | SK::MinusEq |
        SK::StarEq | SK::SlashEq | SK::PercentEq | SK::AmpEq | SK::PipeEq | SK::CaretEq)
}

fn is_bracket(kind: SK) -> bool {
    matches!(kind, SK::LBracket | SK::RBracket | SK::LBrace | SK::RBrace)
}

/// Check if the next sibling token after `current` in `parent` contains a newline.
fn next_has_newline(parent: &SyntaxNode, current: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>) -> bool {
    next_has_newline_skip(parent, current, 0)
}

/// Like next_has_newline but skips `skip` extra tokens after `current` before checking.
fn next_has_newline_skip(parent: &SyntaxNode, current: &rowan::NodeOrToken<SyntaxNode, SyntaxToken>, skip: usize) -> bool {
    let mut found = false;
    let mut remaining = skip;
    for c in parent.children_with_tokens() {
        if found {
            if remaining > 0 { remaining -= 1; continue; }
            if let Some(t) = c.as_token() {
                return t.text().contains('\n');
            }
            return false;
        }
        if c.text_range() == current.text_range() {
            found = true;
        }
    }
    false
}

fn is_multiline(node: &SyntaxNode) -> bool {
    let text = node.text().to_string();
    text.contains('\n') || text.len() > 120
}

/// Check if there are newlines between the last keyword token and the Block child.
fn has_newlines_around_block(node: &SyntaxNode) -> bool {
    let mut saw_kw = false;
    for c in node.children_with_tokens() {
        let kind = c_kind(&c);
        // Any keyword resets the search
        if kind == SK::KwDo || kind == SK::KwElse || kind == SK::KwWhen || kind == SK::KwUnless {
            saw_kw = true;
            continue;
        }
        if saw_kw && kind == SK::Block {
            return is_multiline(c.as_node().unwrap());
        }
        if saw_kw {
            if let Some(t) = c.as_token() {
                if t.text().contains('\n') { return true; }
            }
        }
    }
    false
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_assignment() { assert_eq!(format("x = 42"), "x = 42\n"); }

    #[test]
    fn binary_expr() {
        assert_eq!(format("1+2"), "1 + 2\n");
        assert_eq!(format("1  +  2"), "1 + 2\n");
    }

    #[test]
    fn when_inline() { assert_eq!(format("when x do y end"), "when x do y end\n"); }

    #[test]
    fn when_multiline() {
        assert_eq!(format("when x do\n  y\n  z\nend"), "when x do\n  y\n  z\nend\n");
    }

    #[test]
    fn when_else() {
        assert_eq!(
            format("when x do\n  1\nelse\n  2\nend"),
            "when x do\n  1\nelse\n  2\nend\n"
        );
    }

    #[test]
    fn for_loop() {
        assert_eq!(format("for v in items do\n  v\nend"), "for v in items do\n  v\nend\n");
    }

    #[test]
    fn preserves_comments() {
        let out = format("// hello\nx = 1");
        assert!(out.contains("// hello") && out.contains("x = 1"), "{out}");
    }

    #[test]
    fn preserves_type_annotation() { assert!(format("bonus: int = 10").contains(": int")); }

    #[test]
    fn preserves_extern() { assert!(format("extern config = unknown").contains("extern config")); }

    #[test]
    fn preserves_dynamic_nav() { assert!(format("grades.(subj)").contains(".(subj)")); }

    #[test]
    fn inline_array() { assert_eq!(format("[1, 2, 3]"), "[ 1, 2, 3 ]\n"); }

    #[test]
    fn inline_array_no_commas() { assert_eq!(format("[1 2 3]"), "[ 1 2 3 ]\n"); }

    #[test]
    fn empty_collections() {
        assert_eq!(format("[]"), "[]\n");
        assert_eq!(format("{}"), "{}\n");
    }

    #[test]
    fn inline_object() {
        assert_eq!(format("{a: 1 b: 2}"), "{ a:1 b:2 }\n");
        assert_eq!(format("{a: 1, b: 2}"), "{ a:1 b:2 }\n");
    }

    #[test]
    fn navigation() { assert_eq!(format("foo.bar.baz"), "foo.bar.baz\n"); }

    #[test]
    fn function_call() { assert_eq!(format("f(1, 2)"), "f(1, 2)\n"); }

    #[test]
    fn return_expr() { assert_eq!(format("return 42"), "return 42\n"); }

    #[test]
    fn trailing_newline() { assert!(format("x").ends_with('\n')); }

    #[test]
    fn idempotent() {
        for source in [
            "x = 1", "when x do y end", "[ 1, 2, 3 ]", "{ a: 1 b: 2 }",
            "// comment\nx = 1", "return 42", "f(1, 2)", "foo.bar.baz",
            "for v in items do\n  v\nend", "[]", "{}",
        ] {
            let once = format(source);
            let twice = format(&once);
            assert_eq!(once, twice, "not idempotent for: {source}");
        }
    }

    #[test]
    fn doesnt_split_lines() {
        // Single-line constructs should stay single-line
        assert_eq!(format("when x do y end"), "when x do y end\n");
        assert_eq!(format("for v in x do v end"), "for v in x do v end\n");
        assert_eq!(format("while x do y end"), "while x do y end\n");
    }

    #[test]
    fn comprehension() { assert_eq!(format("[v * 2 for v in items]"), "[ v * 2 for v in items ]\n"); }

    #[test]
    fn unary() {
        assert_eq!(format("-x"), "-x\n");
        assert_eq!(format("delete x"), "delete x\n");
    }

    #[test]
    fn range() { assert_eq!(format("1..10"), "1 .. 10\n"); }

    #[test]
    fn object_comprehension() { assert_eq!(format("{(k): v for k in items}"), "{ (k):v for k in items }\n"); }

    #[test]
    fn comment_in_block() {
        let out = format("when x do\n  /* comment */\n  y\nend");
        assert!(out.contains("/* comment */"), "block comment lost: {out}");
    }
}
