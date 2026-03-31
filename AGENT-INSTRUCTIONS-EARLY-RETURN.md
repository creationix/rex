# Instructions: Implement Early Return in Rex

## Goal

Add a `return` keyword to the Rex language that halts execution and produces a final value. This requires changes to the lexer, parser, compiler (lowerer), interpreter, and decompiler.

## Motivation

Without `return`, Rex programs that need to produce different results from different branches must use `when/else` chains:

```rex
// WORKS but verbose — must chain all branches
when method == "GET" do
  {ok: true, data: items}
else when method == "POST" do
  {ok: true, created: id}
else
  res.status = 405
  {ok: false, error: "method_not_allowed"}
end
```

With `return`, branches can exit independently:

```rex
when method == "GET" do
  return {ok: true, data: items}
end
when method == "POST" do
  return {ok: true, created: id}
end
res.status = 405
{ok: false, error: "method_not_allowed"}
```

## Syntax

```rex
return expr       // return a value
return            // return none (bare return)
```

`return` is a statement-level keyword. It evaluates its expression (if any) and halts execution of the entire program, producing that value as the final result. It propagates through all enclosing blocks, loops, and conditionals.

## Prerequisite

This task requires the **bytecode v2 migration** to be complete (see `AGENT-INSTRUCTIONS-BYTECODE-V2-MIGRATION.md`). After v2, the `;` tag is freed — it is no longer used for lazy lists. Do not attempt this task until the v2 migration has landed.

## Bytecode Encoding

In v2 bytecode, return uses the `;` tag as a postfix operator:

```
[value][varint];
```

The value comes first (already evaluated), then `;` signals return. The varint is reserved for future multi-return (currently always 0 = empty = single return).

Examples:
```
1k+;              → return 42
(ad%x$2+);        → return x + 1
no';              → return none (bare return)
```

## Changes Needed

### 1. Lexer (`crates/rex-core/src/lexer.rs`)

Add a new keyword:

```rust
#[token("return", word_boundary)]
KwReturn,
```

Add it in alphabetical order among the existing keywords. The `word_boundary` callback prevents `returnValue` from matching.

### 2. Syntax (`crates/rex-core/src/syntax.rs`)

Add `KwReturn` to the `SyntaxKind` enum, in the same position as the lexer (the enums must stay in sync — the `From<TokenKind>` impl relies on discriminant ordering).

### 3. Parser (`crates/rex-core/src/parser.rs`)

In `parse_primary_expr`, add `KwReturn` alongside `KwBreak` and `KwContinue`:

```rust
SyntaxKind::KwReturn => {
    self.start_node(SyntaxKind::ReturnExpr);
    self.bump(); // consume 'return'
    // Parse optional return value (if next token starts an expression)
    if !self.at_end() && !matches!(self.current(),
        SyntaxKind::KwEnd | SyntaxKind::KwElse | SyntaxKind::RBrace |
        SyntaxKind::RBracket | SyntaxKind::RParen | SyntaxKind::Error) {
        self.parse_expr();
    }
    self.finish_node();
}
```

Add `ReturnExpr` to the composite node kinds in `SyntaxKind`:
```rust
ReturnExpr,
```

Also add `KwReturn` to the reserved words list and the keyword literal handling.

### 4. AST (`crates/rex-core/src/ast.rs`)

Add a typed wrapper:

```rust
ast_node!(ReturnExpr, ReturnExpr);
```

### 5. Lower (`crates/rex-core/src/lower.rs`)

In `lower_node`, add:

```rust
SyntaxKind::ReturnExpr => Some(lower_return(node)),
```

The lowerer needs a new `Value` variant or a way to represent return. Two options:

**Option A:** Add `Value::Return(Box<Value>)` to the bytecode `Value` enum. The encoder emits the child value followed by `;`.

**Option B:** Since return is a postfix tag in v2, emit the value directly and then a special return marker. This is harder with the current tree-based encoder.

**Recommended: Option A.**

```rust
fn lower_return(node: &SyntaxNode) -> Value {
    let value = non_trivia_children(node)
        .filter_map(|c| lower_child(c))
        .next()
        .unwrap_or(Value::Ref("no".into())); // bare return → none
    Value::Return(Box::new(value))
}
```

### 6. Bytecode (`crates/rex-core/src/bytecode.rs`)

Add `Return(Box<Value>)` to the `Value` enum:

```rust
pub enum Value {
    // ... existing variants ...
    Return(Box<Value>),
}
```

In the encoder (`encode_into`):
```rust
Value::Return(val) => {
    encode_into(val, out);
    out.push(';');
}
```

In the decoder (`decode_one`): The `;` tag is no longer used for lazy lists (removed in v2). Decode `;` as return:

In the dedup encoder (`RevEncoder`): handle `Return` like other single-child nodes.

### 7. Interpreter (`crates/rex-core/src/interpret.rs`)

Add a new error variant for return signal (same pattern as `BreakSignal`/`ContinueSignal`):

```rust
pub enum RexError {
    // ... existing variants ...
    ReturnSignal(RexValue),
}
```

In the eval match:
The interpreter evaluates the `Value` enum (not raw bytecode), so `Return(Box<Value>)` is a wrapper. Handle it directly:

```rust
Value::Return(val) => {
    let result = self.eval_value(val)?;
    return Err(RexError::ReturnSignal(result));
}
```

Then in `eval_top`, catch the signal:

```rust
fn eval_top(&mut self) -> Result<RexValue, RexError> {
    let mut last = RexValue::RexNone;
    while !self.at_end() {
        match self.eval() {
            Ok(val) => last = val,
            Err(RexError::ReturnSignal(val)) => return Ok(val),
            Err(e) => return Err(e),
        }
    }
    Ok(last)
}
```

The `ReturnSignal` propagates through all blocks, loops, and conditionals — each `eval` call site that matches on `Result` will propagate the `Err(ReturnSignal)` upward until `eval_top` catches it.

### 8. Decompiler (`crates/rex-core/src/decompile.rs`)

In the `write` method:
```rust
Value::Return(val) => {
    out.push_str("return");
    // Check if value is none (bare return)
    if !matches!(val.as_ref(), Value::Ref(r) if r == "no") {
        out.push(' ');
        self.write(val, out, Prec::Top);
    }
}
```

## Tests

### Parser tests (`tests/samples.rs`)

```rust
#[test]
fn parse_return() {
    assert_parses("return 42");
    assert_parses("return");
    assert_parses("when x do return y end");
}
```

### Interpreter tests (in `interpret.rs` or `tests/`)

```rust
#[test]
fn eval_return() {
    // Return halts execution
    assert!(matches!(eval("return 42\n99"), RexValue::Int(42)));
}

#[test]
fn eval_bare_return() {
    assert!(matches!(eval("return"), RexValue::RexNone));
}

#[test]
fn eval_return_in_when() {
    // Return exits the entire program, not just the when block
    let result = eval("when true do return 1 end\n2");
    assert!(matches!(result, RexValue::Int(1)));
}

#[test]
fn eval_return_in_loop() {
    // Return exits the loop AND the program
    let result = eval("x = 0\nwhile true do\n  x += 1\n  when x == 5 do return x end\nend\n99");
    assert!(matches!(result, RexValue::Int(5)));
}

#[test]
fn eval_sequential_returns() {
    // First return wins
    let result = eval("when method == \"GET\" do\n  return 1\nend\nwhen method == \"POST\" do\n  return 2\nend\n3");
    // method is none, so neither when matches, result is 3
    assert!(matches!(result, RexValue::Int(3)));
}
```

### Round-trip tests

```rust
#[test]
fn roundtrip_return() {
    // return 42 → compile → decompile → should contain "return"
    roundtrip(Value::Return(Box::new(Value::Integer(42))));
}
```

## File Summary

| File | Change |
|------|--------|
| `crates/rex-core/src/lexer.rs` | Add `KwReturn` keyword |
| `crates/rex-core/src/syntax.rs` | Add `KwReturn` token + `ReturnExpr` node |
| `crates/rex-core/src/parser.rs` | Parse `return [expr]` in primary expressions |
| `crates/rex-core/src/ast.rs` | Add `ReturnExpr` typed wrapper |
| `crates/rex-core/src/lower.rs` | Lower `ReturnExpr` → `Value::Return(child)` |
| `crates/rex-core/src/bytecode.rs` | Add `Return(Box<Value>)` variant, encode/decode |
| `crates/rex-core/src/interpret.rs` | Add `ReturnSignal`, catch in `eval_top` |
| `crates/rex-core/src/decompile.rs` | Decompile `Return` → `return expr` |
| `crates/rex-core/tests/samples.rs` | Parse tests |
| `crates/rex-core/tests/roundtrip.rs` | Round-trip tests |
| `packages/rusty-rex/rex.ohm` | Add `returnTok`, `ReturnKw` to grammar |

## Verification

```sh
cargo test -p rex-core                    # all tests pass
echo 'return 42' | rex run               # outputs 42
echo 'x = 1\nreturn x\n99' | rex run     # outputs 1 (not 99)
echo 'return' | rex run                   # outputs none
```
