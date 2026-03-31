# Instructions: Add `type` and `extern` Keywords to Rex

> **Status: COMPLETE.** `KwType` and `KwExtern` tokens, `TypeDecl` and `ExternDecl` CST nodes, contextual `mut` detection, and `Star` as a valid object key are all implemented in the Rust lexer, parser, and syntax. The real `rex-serve.rexd` file parses successfully. This file is retained as a design reference.

## Goal

Add two new keywords (`type`, `extern`) to the Rex lexer, syntax, and parser. These keywords enable `.rexd` domain interface files to parse as valid Rex. The compiler and interpreter ignore them — they are consumed only by the type checker (a separate task).

This is a pure lexer + parser change. No changes to the lowerer, bytecode, interpreter, or decompiler.

## Prerequisites

None. This task has no dependencies on the bytecode v2 migration or any other task. It is a prerequisite for both:
- `AGENT-INSTRUCTIONS-TYPE-SYSTEM.md` — the type checker engine
- `AGENT-INSTRUCTIONS-TYPECHECKER-CLI.md` — the `rex check` CLI

## What These Keywords Do

### `type` — define a named type alias

```rex
type Headers = {*: string | [string]}
type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"
type FileMeta = {size: integer, modified: integer}
```

`type` followed by an identifier, `=`, and a type expression. By convention type names are uppercase, but the parser does not enforce this.

### `extern` — declare a host-provided binding

```rex
// Simple global
extern config = unknown

// Structural global
extern req = {
  method: HttpMethod
  path: string | [string]
  headers: Headers
}

// Mutable global (mut is contextual after extern)
extern mut res = {status: integer, headers: Headers}

// Function signature (call shape on the left, return type on the right)
extern json.parse(text: string) = some
extern log.info(message: some)

// Function with no return type annotation → implicitly returns none
extern db.delete(key: string) = boolean
```

`extern` followed by an optional `mut`, then either:
- A name + `=` + type expression (global declaration)
- A dotted call expression (function signature), optionally followed by `=` + return type

### `mut` — contextual, not a keyword

`mut` is only recognized immediately after `extern`. It is NOT a standalone reserved word. `mut` used anywhere else is a regular identifier.

```rex
extern mut res = {status: integer}   // mut is a modifier here
x = mut                               // mut is an identifier here
```

## Type Expressions

The right-hand side of `=` in `type` and `extern` declarations is a **type expression**. Type expressions reuse Rex value syntax but are interpreted as types by the type checker. The parser does NOT need to distinguish type expressions from regular expressions — it parses them using the existing expression parser. The type checker interprets them later.

This means:
- `{*: string}` — the `*` is parsed as `Star` token in key position. The parser already handles `parse_obj_key` and currently rejects `*`. **Add `Star` as a valid object key.**
- `string | number` — parsed as a `BinaryExpr` with `Pipe` operator (bitwise OR). The type checker interprets this as a union.
- `[string]` — parsed as an `ArrayExpr`. The type checker interprets this as an array type.
- `{key: T}` — parsed as an `ObjectExpr`. The type checker interprets field values as types.
- `"GET"` — parsed as a string literal. The type checker interprets this as a literal string type.
- `HttpMethod` — parsed as an `Ident`. The type checker resolves this as a type alias reference.

The only parser change needed for type expressions is allowing `Star` in `parse_obj_key`. Everything else already parses correctly.

## Rest Parameters

The `rex-serve.rexd` file uses `...values: some` for rest parameters (e.g., `extern html(parts: [string], ...values: some) = string`). The lexer tokenizes `...` as `DotDot` + `Dot` (two tokens). This token sequence appears inside the call args and will produce parse errors alongside the `:` errors described in section 3c. This is acceptable — the CST preserves all tokens and the type checker can extract the rest parameter pattern from the raw token sequence.

## Changes by File

### 1. Lexer (`crates/rex-core/src/lexer.rs`)

Add two keywords in alphabetical position among existing keywords:

```rust
#[token("extern", word_boundary)]
KwExtern,

// ... existing keywords ...

#[token("type", word_boundary)]
KwType,
```

**Important:** The `TokenKind` enum order must match `SyntaxKind` exactly (the `From<TokenKind>` impl uses a discriminant cast). Insert `KwExtern` after `KwEnd` and `KwType` after `KwTrue`.

The full keyword block after changes (alphabetical order):

```rust
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
#[token("nor", word_boundary)]
KwNor,
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
#[token("self", word_boundary)]
KwSelf,
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
```

### 2. Syntax (`crates/rex-core/src/syntax.rs`)

Add the corresponding leaf tokens in the **exact same position** as in `TokenKind` (the `From<TokenKind>` cast requires 1:1 discriminant matching):

```rust
// Insert after KwEnd:
KwExtern,

// Insert after KwTrue:
KwType,
```

Add composite node kinds after the existing ones (order here is flexible):

```rust
TypeDecl,        // type Name = type-expr
ExternDecl,      // extern [mut] name = type-expr  OR  extern [mut] name.fn(args) [= return-type]
```

### 3. Parser (`crates/rex-core/src/parser.rs`)

#### 3a. Allow `Star` as an object key

In `parse_obj_key`, add `SyntaxKind::Star`:

```rust
fn parse_obj_key(&mut self) {
    match self.current() {
        SyntaxKind::Ident => self.bump(),
        SyntaxKind::Star => self.bump(),  // ADD: wildcard key for type expressions
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
```

#### 3b. Parse `type` declarations

In `parse_primary_expr`, add a case for `KwType`:

```rust
SyntaxKind::KwType => self.parse_type_decl(),
```

The parse function:

```rust
fn parse_type_decl(&mut self) {
    self.start_node(SyntaxKind::TypeDecl);
    self.bump(); // type
    self.expect(SyntaxKind::Ident); // Name
    self.expect(SyntaxKind::Eq);    // =
    self.parse_expr();              // type expression (parsed as regular expr)
    self.finish_node();
}
```

#### 3c. Parse `extern` declarations

In `parse_primary_expr`, add a case for `KwExtern`:

```rust
SyntaxKind::KwExtern => self.parse_extern_decl(),
```

The parse function:

```rust
fn parse_extern_decl(&mut self) {
    self.start_node(SyntaxKind::ExternDecl);
    self.bump(); // extern

    // Check for contextual `mut`
    if self.current() == SyntaxKind::Ident {
        // Peek: is this `mut` followed by another ident?
        // We need to distinguish `extern mut res = ...` from `extern config = ...`
        let text = self.current_text();
        if text == "mut" {
            self.bump(); // mut (consumed as Ident token — it's contextual)
        }
    }

    // Parse the left-hand side: could be:
    //   name = type-expr           (simple global)
    //   name.path = type-expr      (dotted global — rare)
    //   name.fn(args)              (function, no return type)
    //   name.fn(args) = ret-type   (function with return type)
    self.parse_expr();

    // If we see `=` and it wasn't consumed by the expr parser as assignment,
    // the left side was a call expression (function signature).
    // The `=` for simple globals is already consumed by parse_expr → parse_assign_expr.

    self.finish_node();
}
```

**Note on the `extern` parse:** The existing `parse_expr` already handles assignment (`name = expr`) and calls (`name.fn(args)`). For `extern name = type-expr`, the parse_expr produces an `AssignExpr`. For `extern name.fn(args) = ret-type`, the call is parsed first, then the assignment wraps it. For `extern name.fn(args)` with no return type, parse_expr produces a `CallExpr`. The `ExternDecl` node just wraps whatever parse_expr produces.

However, there is a subtlety: `extern json.parse(text: string) = some` — the `text: string` inside the parens needs to parse. In current Rex, `text: string` inside a call would be parsed as... `text` (Ident), then `:` (Colon), then `string` (KwString). The colon is not an operator in expression context, so the parser would see `text` as one arg and `:` as an error.

**The fix:** Function argument declarations in `extern` use the same `key: value` syntax as object fields. The simplest approach is: since the call args are inside parentheses, and `parse_elements` already parses comma-separated exprs, the colon between `text` and `string` will be an error token in the current parser. **This is acceptable.** The CST will still contain all the tokens (identifiers, colons, type names) — the type checker can walk the children of the `CallExpr` and extract the argument names and types from the raw tokens, tolerating parse errors.

Alternatively, if clean CST nodes are preferred: modify `parse_elements` (or add a variant) that treats each argument as a `Pair` node (like object fields) when inside an `extern` context. This is more work but produces a cleaner tree.

**Recommended: accept the parse errors for now.** The CST preserves all tokens regardless of errors, and the type checker will walk the raw tokens anyway. Clean argument parsing can be refined later.

#### 3d. Add `current_text` helper

The parser needs to read the text of the current token to check for contextual `mut`:

```rust
fn current_text(&self) -> &str {
    if self.pos < self.tokens.len() {
        // Find the actual non-trivia token
        let mut pos = self.pos;
        while pos < self.tokens.len() {
            let kind = SyntaxKind::from(self.tokens[pos].kind);
            if !kind.is_trivia() {
                return &self.source[self.tokens[pos].span.clone()];
            }
            pos += 1;
        }
    }
    ""
}
```

### 4. No changes needed

These files are NOT modified:

| File | Why |
|------|-----|
| `crates/rex-core/src/lower.rs` | The lowerer skips unknown node kinds — `TypeDecl`/`ExternDecl` are silently ignored |
| `crates/rex-core/src/bytecode.rs` | No new `Value` variants |
| `crates/rex-core/src/interpret.rs` | No runtime semantics |
| `crates/rex-core/src/decompile.rs` | Nothing to decompile |
| `crates/rex-core/src/ast.rs` | Optional — typed wrappers can be added later when the type checker needs them |

## Tests

### Lexer tests (`crates/rex-core/src/lexer.rs`)

```rust
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
```

### Parser tests (`crates/rex-core/src/parser.rs` or `tests/samples.rs`)

```rust
#[test]
fn parse_type_decl() {
    let tree = assert_no_errors("type Headers = {*: string}");
    let decl = tree.children()
        .find(|n| n.kind() == SyntaxKind::TypeDecl)
        .expect("expected TypeDecl node");
    // Should contain: KwType, Ident("Headers"), Eq, ObjectExpr
    assert!(decl.children_with_tokens().any(|c|
        c.as_token().map_or(false, |t| t.kind() == SyntaxKind::KwType)
    ));
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
    let decl = tree.children()
        .find(|n| n.kind() == SyntaxKind::ExternDecl)
        .expect("expected ExternDecl node");
    // Should contain `mut` as an Ident token (contextual)
    let has_mut = decl.children_with_tokens().any(|c|
        c.as_token().map_or(false, |t| t.text() == "mut")
    );
    assert!(has_mut);
}

#[test]
fn parse_extern_function() {
    // Function signatures may have parse errors on `:` inside args — that's OK
    let (tree, _errors) = parse_str("extern json.parse(text: string) = some");
    let decl = tree.children()
        .find(|n| n.kind() == SyntaxKind::ExternDecl)
        .expect("expected ExternDecl node");
    assert!(decl.text().contains("json"));
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
    // `mut` used as a regular identifier
    assert_no_errors("mut = 42");
    assert_eq!(non_trivia("mut"), vec![TokenKind::Ident]);
}

#[test]
fn parse_rexd_file_inline() {
    // A realistic .rexd file should parse without panics
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
    // Parse the actual rex-serve.rexd — should not panic
    let source = std::fs::read_to_string("examples/knowledge-base/rex-serve.rexd").unwrap();
    let tokens = lexer::lex(&source);
    let (_tree, _errors) = parser::parse(&source, &tokens);
    // Errors on `:` and `...` in function args are expected
    // The important thing is: no panic, and TypeDecl/ExternDecl nodes exist
}
```

### Syntax tests (`crates/rex-core/src/syntax.rs`)

```rust
#[test]
fn new_keywords_convert() {
    assert_eq!(SyntaxKind::from(TokenKind::KwType), SyntaxKind::KwType);
    assert_eq!(SyntaxKind::from(TokenKind::KwExtern), SyntaxKind::KwExtern);
}
```

## Verification

```sh
cargo test -p rex-core                    # all tests pass
cargo check -p rex-core                   # no warnings

# Keywords lex correctly
echo 'type Foo = string' | cargo run -p rex-cli -- compile    # should not panic (may produce empty/error bytecode — that's fine, lowerer ignores TypeDecl)
echo 'extern x = number' | cargo run -p rex-cli -- compile    # same

# Existing code is unaffected
echo '1 + 2' | cargo run -p rex-cli -- run    # 3
echo 'x = 42' | cargo run -p rex-cli -- run   # 42
```

## File Summary

| File | Change |
|------|--------|
| `crates/rex-core/src/lexer.rs` | Add `KwType`, `KwExtern` tokens |
| `crates/rex-core/src/syntax.rs` | Add `KwType`, `KwExtern` leaf tokens + `TypeDecl`, `ExternDecl` composite nodes |
| `crates/rex-core/src/parser.rs` | Parse `type` and `extern` declarations, allow `Star` as object key, add `current_text` helper |

## What NOT to Do

- Don't modify the lowerer — `TypeDecl`/`ExternDecl` nodes are silently skipped
- Don't modify the bytecode encoder/decoder — no new `Value` variants
- Don't modify the interpreter — no runtime semantics for type declarations
- Don't implement the type checker — that's a separate task that depends on this one
- Don't make `mut` a keyword — it's contextual, recognized only after `extern`
- Don't enforce uppercase names for `type` — the convention is documented but not enforced by the parser
