# Instructions: Template Literals in Rex

> **Status: COMPLETE.** Template literals are fully implemented across lexer, parser, lowerer, interpreter, and syntax highlighting. The rex-serve knowledge-base examples use both untagged and tagged (`html` tag) template literals in production. This file is retained as a design reference.

## Goal

Add template literal syntax to the Rex language. This is a parser + compiler change only — no new bytecode tags needed. Template literals compile to existing string chains (`.`) for untagged, or to regular calls `(` `)` for tagged.

## Syntax

### Untagged template literals

```rex
`hello ${name}, you have ${count} items`
```

Backtick-delimited string with `${expr}` interpolations. Compiles to a string chain.

### Tagged template literals (like JavaScript)

```rex
html`<a href="/articles/${slug}">${title}</a>`
sql`SELECT * FROM ${table} WHERE id = ${id}`
```

A tag (identifier) immediately before the backtick. The tag function receives the static string parts as an array and the interpolated values as separate arguments. Compiles to a call.

### Raw strings (no interpolation)

A template with no `${}` expressions is just a convenient string syntax that doesn't require escaping quotes:

```rex
`he said "hello" and she said 'goodbye'`
```

This compiles to a regular string value (`,` tag).

## How It Compiles

### Untagged → string chain (`.`)

```rex
`hello ${name}`
```

Compiles to a string chain with interleaved string literals and expressions:

```
.[6,hello name$]
```

The chain (`.`) concatenates all segments. String segments are string values (`,`), expression segments are evaluated and coerced to strings.

Important: the chain already exists in the bytecode — currently used for string prefix dedup. Template literals reuse this mechanism. The interpreter needs to handle arbitrary expressions inside chains (not just strings and pointers). This should already work if the interpreter calls `eval()` on each chain segment.

### Tagged → call with string parts array

```rex
html`<a>${title}</a>`
```

Compiles to:

```
(html%[4,<a >5,</a>]title$)
```

This is a regular call where:
- Callee: `html%` (opcode)
- First arg: `[4,<a >5,</a>]` (array of static string parts)
- Remaining args: `title$` (the interpolated values)

The tag function receives `(["<a>", "</a>"], title)`.

If the tag is a variable (not an opcode), it's a navigation call instead:

```rex
myTag`hello ${x}`
```

Compiles to a call with `myTag$` as callee.

### No interpolations → plain string

```rex
`no escaping "needed" here`
```

Compiles to just a string: `1e,no escaping "needed" here`

## Changes Needed

### 1. Lexer (`crates/rex-core/src/lexer.rs`)

Add new token kinds:

```rust
// Template literal parts
#[regex(r"`[^`$]*`")]  // Simple: no interpolations (but this is tricky with logos)
TemplateLiteral,

// Or better: handle templates as multiple tokens:
BacktickOpen,     // `  (opening backtick)
BacktickClose,    // `  (closing backtick)
TemplateChars,    // literal characters between interpolations
DollarBrace,      // ${
// } is already RBrace
```

**Recommended approach**: The lexer should emit template literals as a sequence of tokens. This is similar to how most parsers handle template strings — the lexer switches modes when it sees a backtick.

However, logos (the lexer generator) doesn't support modal lexing well. A simpler approach:

**Simple approach**: Lex the entire template literal as a single token (backtick to backtick), then do a secondary parse in the parser/lowerer to split it into parts and interpolations. This avoids lexer mode changes.

```rust
#[regex(r"`([^`\\]|\\.|\$\{[^}]*\})*`")]  // rough — needs refinement
TemplateLiteral,
```

The exact regex needs care to handle nested braces in `${}` expressions. An alternative is to lex backtick-to-backtick and do the splitting in a post-lex step.

### 2. Parser (`crates/rex-core/src/parser.rs`)

Add a new `SyntaxKind::TemplateLiteral` or `SyntaxKind::TemplateExpr` node.

In `parse_primary_expr`, when the parser sees a backtick token (or template literal token):
- If preceded by an identifier → tagged template
- Otherwise → untagged template

The CST node should preserve the raw template string so the lowerer can split it.

### 3. Syntax (`crates/rex-core/src/syntax.rs`)

Add to `SyntaxKind`:
```rust
TemplateLiteral,   // leaf token for the raw template string
TemplateExpr,      // composite node for the parsed template
```

### 4. Lower (`crates/rex-core/src/lower.rs`)

This is where the main work happens. When lowering a `TemplateExpr`:

1. Parse the raw template string to extract static parts and `${...}` expressions
2. Parse each `${...}` expression as Rex source (recursive parse + lower)
3. Decide the output:
   - **No interpolations** → `Value::String(content)`
   - **Untagged with interpolations** → string chain: interleave `Value::String` parts with expression values, wrap in chain structure
   - **Tagged** → `Value::Call([Value::Opcode(tag) or Value::Variable(tag), Value::List(string_parts), expr1, expr2, ...])`

### 5. Tests

Add tests for:
- Simple template: `` `hello` `` → string
- Interpolation: `` `${x}` `` → chain with variable
- Multiple interpolations: `` `${a} and ${b}` `` → chain
- Tagged: `` html`<p>${text}</p>` `` → call
- Escaped backtick: `` `\`` `` → string with backtick
- Escaped dollar: `` `\${not interpolated}` `` → string
- Expression in interpolation: `` `${a + b}` `` → chain with call
- Empty template: `` `` `` → empty string
- Nested template: `` `outer ${`inner ${x}`}` `` → nested chains (if we want to support this)

### 6. Decompiler (`crates/rex-core/src/decompile.rs`)

Update to recognize chains that came from templates and optionally decompile them back to template literal syntax. This is nice-to-have — chains can also decompile as string concatenation.

## String Coercion in Templates

When a non-string expression appears in a template, it's coerced to a string. The coercion rules (from `rex-types.md`):

| Value | String form |
|-------|-------------|
| `string` | as-is |
| `integer` | decimal digits |
| `number` | decimal |
| `boolean` | `✓` or `✗` |
| `null` | `␀` |
| `none` | `∅` |
| `NaN` | `NaN` |
| `Infinity` | `∞` |

This coercion happens in the interpreter's chain evaluation, not in the compiler.

## What NOT to Change

- **Bytecode format** — no new tags. Templates use existing chains (`.`) and calls (`()`).
- **Interpreter** — should already handle chains with expressions if it calls `eval()` on segments. May need to add string coercion for non-string segment results.
- **Type system** — template literals produce `string` type. Tagged templates produce whatever the tag function returns.

## File Summary

| File | Change |
|------|--------|
| `crates/rex-core/src/lexer.rs` | Add template literal token(s) |
| `crates/rex-core/src/syntax.rs` | Add `TemplateLiteral` / `TemplateExpr` syntax kinds |
| `crates/rex-core/src/parser.rs` | Parse template literals in primary expressions |
| `crates/rex-core/src/lower.rs` | Lower templates to chains (untagged) or calls (tagged) |
| `crates/rex-core/src/decompile.rs` | Optional: decompile chains back to template syntax |
| `crates/rex-core/tests/samples.rs` | Add template literal parse tests |
| `crates/rex-core/tests/roundtrip.rs` | Add template round-trip tests |
| `packages/rusty-rex/rex.ohm` | Update grammar (for reference, not used by Rust parser) |

## Verification

```sh
cargo test -p rex-core          # all existing tests still pass
echo '`hello ${name}`' | cargo run -p rex-cli -- compile    # produces chain bytecode
echo '`hello ${name}`' | cargo run -p rex-cli -- run        # outputs "hello ∅" (name is none)
```
