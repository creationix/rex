# Agent Task: Bytecode V2 — Remove self, unless, nor from Rust compiler

## Overview

The bytecode spec (`rexc-bytecode.md`) and language spec (`language.md`) have been updated. The Rust compiler in `packages/rusty-rex/crates/rex-core/src/` needs to match.

## What changed in the spec

### Removed from bytecode
- `@` scalar (self) — no longer exists
- `!` modifier (unless) — no longer exists

### Removed from language
- `self` keyword — use explicit bindings
- `nor` keyword — removed entirely
- 0-binding `for in` loops — require 1-2 bindings

### Kept as syntax sugar
- `unless` keyword — compiles to `?` with swapped branches

### Changed
- `&()` (and) — now variadic (was binary)
- `|()` (or) — now variadic (was binary)
- `?()` (cond) — now variadic: `?(c1 t1 [c2 t2 ...] [else])` (was `?(cond then [else])`)
- `and` binds tighter than `or` (were same precedence)

### Compilation mappings

```
a and b and c              →  &(a b c)
a or b or c                →  |(a b c)
when c do t end            →  ?(c t)
when c do t else e end     →  ?(c t e)
when c1 do t1
  else when c2 do t2
  else e end               →  ?(c1 t1 c2 t2 e)
unless c do t end          →  ?(c no' t)
unless c do t else e end   →  ?(c e t)
```

## Files to update

### 1. `bytecode.rs` — Value enum, encoder, decoder

**Value enum** — remove two variants:
- `SelfRef(u32)` — delete
- `Unless(Vec<Value>)` — delete

**Simple encoder (`encode_into`):**
- Remove `Value::SelfRef` arm (line ~161)
- Remove `Value::Unless` arm (line ~189) — was already encoding as `?`

**Dedup encoder (`emit`):**
- Remove `Value::SelfRef` arm (line ~662) — was emitting `@`
- Remove `Value::Unless` arm (line ~680)

**`is_container` helper (line ~382):**
- Remove `Value::Unless` from pattern

**`prescan_counts` (line ~458):**
- Remove `Value::Unless` from pattern

**Decoder (`decode_one`, line ~991):**
- Remove `b'@'` arm — was producing `Value::SelfRef`
- Remove `b'!'` from the modifier case (line ~1061)
- Remove the `b'!'` mapping (line ~1248)

**`emit_compound` (line ~851):**
- Already correct — `is_conditional` checks `b'?' | b'|' | b'&'` (no `b'!'`)

**Tests:**
- Remove `SelfRef` roundtrip test (line ~1413)
- Update `unless_roundtrip` test (line ~1556) — Unless variant no longer exists, test should use When directly
- Remove `SelfRef` from other tests (lines ~1614-1615)
- Update dedup test using `Unless` (lines ~1837, 1850)

### 2. `lower.rs` — CST to IR lowering

**`KwSelf` (line ~101):**
- Remove — was producing `Value::SelfRef(0)`

**`SelfRef` depth helper (line ~466):**
- Remove the `self@N` lowering function

**`KwNor` (line ~251):**
- Remove — `nor` is no longer in the language
- Currently emits `Value::When(vec![lhs, Value::Ref("no".into()), rhs])` which is approximately right, but the variant is being deleted

**`KwUnless` handling (lines ~476, 537):**
- Keep — `unless` is syntax sugar
- Must emit `Value::When` with branches swapped (and `no'` placeholder for no-else case)
- `unless c do t end` → `Value::When(vec![c, Value::Ref("no".into()), t])`
- `unless c do t else e end` → `Value::When(vec![c, e, t])`

**Variadic flattening (already done):**
- `flatten_variadic` already handles `And`/`Or` chains — good
- Remove `Unless` from `flatten_variadic` — it no longer exists as a variant

### 3. `lexer.rs` — Token kinds

- Remove `KwNor` token kind
- Remove `KwSelf` token kind (if it maps to the self keyword)
- Keep `KwUnless` — still valid syntax

### 4. `syntax.rs` — Syntax kinds

- Remove `KwNor`
- Remove `KwSelf` (if present)
- Keep `KwUnless`

### 5. `parser.rs` — Parsing

- Remove `KwNor` from binary operator precedence (line ~48)
- Keep `KwUnless` conditional parsing (lines ~364, 521)
- Split `and`/`or` precedence: `and` should bind tighter than `or`

### 6. `ast.rs` — AST helpers

- Remove `KwNor` from binary operator token list (line ~87)
- Keep `KwUnless` in conditional detection (line ~144)

### 7. `decompile.rs` — IR to source

- Remove `Value::SelfRef` decompilation (line ~57) and helper
- Remove `Value::Unless` decompilation (line ~73)
- Update tests that use `SelfRef` (lines ~773-774, 911-912)

## Testing strategy

After all changes, run:
```sh
cd packages/rusty-rex
cargo test
```

All existing tests should pass (after updating the ones that reference removed variants). Key things to verify:

1. `When` with 2 args roundtrips correctly (simple when, no else)
2. `When` with 3 args roundtrips (when/else)
3. `When` with 5 args roundtrips (when/else-when/else)
4. `And` and `Or` with 1, 2, 3+ args roundtrip
5. `unless` source compiles to `When` with swapped branches
6. Length prefixes are correct on variadic children
7. Dedup still works with the simplified variant set
8. `@` and `!` in bytecode input produce decode errors (or are silently ignored)

## Notes

- The lowerer's `flatten_variadic` function already collapses chained `and`/`or` into single variadic nodes — this is correct for the new spec.
- The encoder already emits `Unless` as `?` modifier — so the bytecode output is already correct, we're just cleaning up the IR representation.
- The decoder should probably still accept `!` and `@` for backwards compatibility with old bytecode, mapping `!` → `When` and `@` → error/ignore. Or just reject them cleanly. Your call.
