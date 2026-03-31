# Instructions: Migrate Bytecode Encoder/Decoder to V2

## Goal

Update the Rex bytecode encoder, decoder, and interpreter to match the v2 spec (`packages/rusty-rex/bytecode-v2.md`). This is the foundational change that unblocks early return (`;` tag) and simplifies the format.

## Key Document

**Read `packages/rusty-rex/bytecode-v2.md` thoroughly.** It is the spec. Everything in this file describes what the code should do after the migration.

## What Changes

### Summary

| Feature | V1 (current) | V2 (target) |
|---------|-------------|-------------|
| Data arrays | `[size];[body]` — sized, always lazy | `[elem0 elem1...]` — paired delimiters, eager by default |
| Data maps | `[size]:[body]` — sized, always lazy | `{key0 val0 key1 val1...}` — paired delimiters, eager by default |
| Indexed containers | Not implemented | `[#end count ptr0...ptrN elems]` or `{#end count ptr0...ptrN pairs}` — lazy + random access |
| REXC arrays | `[elem0 elem1...]` | Same (no change) |
| REXC blocks | `{expr0 expr1...}` | Same (no change) |
| REXC calls | `(callee args...)` | Same (no change) |
| `;` tag | Sized lazy list | **Removed** — freed for `return` (see `AGENT-INSTRUCTIONS-EARLY-RETURN.md`) |
| `:` tag | Sized lazy map | **Removed** — no longer used in v2 |
| String chains | `[size].[segments]` — sized body | Same (no change — chains keep size prefix) |
| Lazy evaluation | All `;`/`:` containers are lazy | Only indexed (`#`) containers are lazy — **known regression until indexed containers are implemented** |

### Core insight

In v1, the distinction between lazy and eager was by TAG: `;` `:`= lazy, `[]` `{}` = eager.

In v2, the distinction is by INDEX: any container with `#` after the opener is lazy/indexed/skippable. Without `#`, it's eager.

This means `Value::List` and `Value::Array` merge — both use `[]` in the bytecode. `Value::Map` becomes `Value::Object` — both use `{}`. The difference is only whether the encoder adds an index.

**Known regression:** After the core migration (before indexed containers are implemented), all data containers become eager. Large JSON payloads that currently benefit from lazy access will be fully materialized. This is acceptable as a temporary state — indexed container support can be added as a follow-up.

## Changes by File

### 1. `crates/rex-core/src/bytecode.rs` — Value Enum

**Before:**
```rust
pub enum Value {
    // ...
    List(Vec<Value>),            // ; lazy list
    Map(Vec<(Value, Value)>),    // : lazy map
    Array(Vec<Value>),           // [] eager array
    Block(Vec<Value>),           // {} eager block
    Call(Vec<Value>),            // () call
    // ...
}
```

**After:** Remove `List` (merge into `Array`) and rename `Map` to `Object`.

```rust
pub enum Value {
    // ...
    Array(Vec<Value>),                // [] — both data arrays and code arrays
    Object(Vec<(Value, Value)>),      // {} — data objects (key-value pairs, replacing Map)
    Block(Vec<Value>),                // {} — code blocks (sequence, returns last)
    Call(Vec<Value>),                 // () — calls
    // ...
}
```

**Note:** `Block` stays separate because it has different semantics (returns last value, not a data structure). `Object` replaces `Map` — key-value pairs are always `(Value, Value)`.

**Important:** `Value::Array` already exists, so you cannot do a simple find-and-replace of `List` → `Array`. Instead: delete the `List` variant, then update each `Value::List(...)` construction/match site to use `Value::Array(...)`. See [Migration Strategy](#migration-strategy) for the correct order.

### 2. `crates/rex-core/src/bytecode.rs` — Encoder

**Update `encode_sized_body`:** Do NOT delete this function — `Value::Chain` (template literals) still uses it for the `.` tag. Only remove the `;` and `:` callers.

**Update `encode_into`:**

```rust
// REMOVE:
Value::List(items) => encode_sized_body(';', items, out),
Value::Map(pairs) => { /* emit size : body */ }

// KEEP (already exists):
Value::Array(items) => encode_paired('[', ']', items, out),

// ADD (replacing Map):
Value::Object(pairs) => {
    out.push('{');
    for (k, v) in pairs {
        encode_into(k, out);
        encode_into(v, out);
    }
    out.push('}');
}

// KEEP (unchanged):
Value::Chain(items) => encode_sized_body('.', items, out),
```

**Update `RevEncoder`:** The `emit` method's `List` arm becomes `Array` (using `[`/`]` delimiters). Rename `emit_map` to `emit_object` — the schema pointer mechanism stays the same, but the container syntax changes from `[size]:[body]` to `{`/`}` paired delimiters:

```rust
fn emit_object(&mut self, pairs: &[(Value, Value)]) {
    if pairs.is_empty() {
        self.push(b'}');
        self.push(b'{');
        return;
    }

    let schema = Self::schema_key(pairs);

    if let Some(&(schema_left, _schema_len)) = self.schemas.get(&schema) {
        // Schema match: emit values + pointer to schema, wrapped in {}
        self.push(b'}');
        for (_k, v) in pairs.iter().rev() {
            self.write(v);
        }
        let delta = (self.pos - schema_left) as u64;
        self.push(b'^');
        self.push_varint(delta);
        self.push(b'{');
    } else {
        // First occurrence: encode all key-value pairs
        let before = self.pos;
        self.push(b'}');
        for (k, v) in pairs.iter().rev() {
            self.write(v);
            self.write(k);
        }
        self.push(b'{');
        let obj_len = self.pos - before;
        self.schemas.insert(schema, (self.pos, obj_len));
    }
}
```

### 3. `crates/rex-core/src/bytecode.rs` — Decoder

**Remove:** The `b';'` and `b':'` match arms in `decode_one`. These tags no longer exist in v2.

**Update `b'{'` to handle both objects and blocks:**

The encoder controls what goes in — `Value::Object` and `Value::Block` both emit `{`/`}`, but they round-trip faithfully because the content structure differs. The decoder distinguishes them by content:

- If the first child is a `Value::String` → object with explicit key-value pairs
- If the first child resolves to an `Object` or `Array` (via pointer) → schema-shared object (use schema keys, remaining children are values)
- Otherwise → block (sequence of expressions)

```rust
b'{' => {
    let mut children = Vec::new();
    while *pos < input.len() && input[*pos] != b'}' {
        children.push(read_value(input, pos, resolve)?);
    }
    *pos += 1; // consume '}'

    if children.is_empty() {
        return Ok(Value::Object(vec![]));
    }

    match &children[0] {
        // First child is a string → explicit key-value object
        Value::String(_) if children.len() % 2 == 0 => {
            let pairs = children.chunks(2)
                .map(|chunk| (chunk[0].clone(), chunk[1].clone()))
                .collect();
            Ok(Value::Object(pairs))
        }
        // First child is an object/array (schema pointer resolved) → schema-shared object
        Value::Object(schema_pairs) => {
            let pairs = schema_pairs.iter().zip(children[1..].iter())
                .map(|((k, _), v)| (k.clone(), v.clone()))
                .collect();
            Ok(Value::Object(pairs))
        }
        Value::Array(schema_keys) => {
            let pairs = schema_keys.iter().zip(children[1..].iter())
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            Ok(Value::Object(pairs))
        }
        // Otherwise → code block
        _ => Ok(Value::Block(children))
    }
}
```

**Note on the heuristic:** This works because the lowerer and encoder guarantee that objects always have string keys first and an even child count. A code block whose first expression happens to be a string literal with an even total count is a theoretical false positive, but in practice Rex code blocks don't start with bare string literals — they contain assignments, calls, or control flow. If this ever becomes a problem, the encoder can add a distinguishing marker.

### 4. `crates/rex-core/src/bytecode.rs` — Indexed Containers (follow-up)

**This is a follow-up task, not part of the core migration.** Without indexed containers, all data is eager — see "Known regression" above.

When implemented, the encoder emits `#` after the opener for containers that should be lazy/indexed:

```
[#<end_ptr><count><ptr0>...<ptrN><elements>]
```

The decoder needs to handle `#` after `[` or `{`:

```rust
b'[' => {
    if *pos < input.len() && input[*pos] == b'#' {
        *pos += 1; // consume '#'
        return decode_indexed_array(input, pos, resolve);
    }
    // ... normal array decoding
}
```

The encoder can choose to index based on a threshold (e.g., containers with >16 elements). For the initial implementation, decoding indexed containers eagerly (reading all elements) is fine. True lazy evaluation requires changes to `RexValue` and the interpreter, which is a separate task.

### 5. `crates/rex-core/src/lower.rs`

**Replace `Value::List(...)` → `Value::Array(...)` everywhere:**

- Line 665: Pure data arrays — change `Value::List(items)` to `Value::Array(items)`
- Line 980: Template literal string parts — change `Value::List(string_parts)` to `Value::Array(string_parts)`

**Replace `Value::Map(...)` → `Value::Object(...)` everywhere:**

- Line 730/734: Object literal lowering — change `Value::Map(pairs)` to `Value::Object(pairs)`

**Update `is_data()` — keep the function, update the variants it checks:**

```rust
fn is_data(v: &Value) -> bool {
    match v {
        Value::Integer(_) | Value::Decimal { .. } | Value::String(_) | Value::Ref(_) => true,
        Value::Array(items) => items.iter().all(is_data),
        Value::Object(pairs) => pairs.iter().all(|(k, v)| is_data(k) && is_data(v)),
        _ => false,
    }
}
```

`is_data()` is still useful — the encoder may use it to decide whether to add an index (`#`) to a container when indexed container support is added.

### 6. `crates/rex-core/src/interpret.rs`

**Remove lazy evaluation infrastructure:**

- Remove `RexValue::Lazy(LazySpan)` variant from the `RexValue` enum
- Remove the `LazySpan` struct definition
- Remove the `b';'` match arm (lazy list creation)
- Remove the `b':'` match arm (lazy map creation)
- Remove `read_lazy_property()` method
- Remove `RexValue::Lazy` arm in `materialize_iterable()`
- Remove `RexValue::Lazy` arm in `materialize_keys()`
- Remove `RexValue::Lazy` arm in `force_value()`
- Remove `RexValue::Lazy` arm in the type name method (returns `"lazy"`)

**Update all `Value::List` → `Value::Array`** and **`Value::Map` → `Value::Object`** matches in the eval function.

### 7. `crates/rex-core/src/decompile.rs`

- Change `Value::List(items) => self.write_list(items, out)` to match `Value::Array`
  - Note: `Array` already has a match arm (for eager arrays). Merge the two — after v2 they're the same. Use `write_list` (bracket syntax) for both.
- Change `Value::Map(pairs) => self.write_map(pairs, out)` to match `Value::Object`

### 8. `crates/rex-core/src/json_fast.rs`

- Change `Value::List(items)` → `Value::Array(items)` (JSON arrays)
- Change `Value::Map(pairs)` → `Value::Object(pairs)` (JSON objects)
- Update tests that construct or match on these variants

### 9. `crates/rex-cli/src/main.rs`

- Change `Value::List(items) | Value::Array(items) =>` to just `Value::Array(items) =>` (JSON output)
- Change `Value::Map(pairs) =>` to `Value::Object(pairs) =>` (JSON output, value printing)
- Update `count_values` function for the renamed variants
- Remove `RexValue::Lazy(span)` display formatting

### 10. `crates/rex-serve/src/refs.rs`

- Remove `RexValue::Lazy(_) => serde_json::Value::Null` match arm (lazy is gone)

### 11. `crates/rex-serve/src/opcodes.rs`

- Remove `RexValue::Lazy(span) =>` lazy materialization arm

### 12. `crates/rex-node/src/lib.rs`

- Change `Value::List` → `Value::Array` (JavaScript array conversion)
- Change `Value::Map` → `Value::Object` (JavaScript object conversion)

### 13. `crates/rex-luajit/src/lib.rs` and `src/ffi.rs`

- Change `Value::List` → `Value::Array` (Lua array conversion)
- Change `Value::Map` → `Value::Object` (Lua table conversion)

### 14. All test files

- `crates/rex-core/tests/roundtrip.rs` — Update all `Value::List(...)` and `Value::Map(...)` constructors and expected output strings
- `crates/rex-core/src/bytecode.rs` — Inline tests using `List`/`Map`
- `crates/rex-core/src/json_fast.rs` — Inline tests
- `crates/rex-core/src/decompile.rs` — Inline tests

Search for: `Value::List(`, `Value::Map(`, `RexValue::Lazy`, `LazySpan`, `encode_sized_body` and update all occurrences.

## Migration Strategy

The order matters — some steps have dependencies.

### Step 1: Remove `Value::List`, merge into `Value::Array`

`Value::Array` already exists, so you cannot add another `Array` variant. Instead:

1. Delete the `List(Vec<Value>)` variant from the `Value` enum
2. At every `Value::List(...)` construction site, change to `Value::Array(...)`
3. At every `Value::List(items)` match arm, change to `Value::Array(items)` — merge with existing `Array` arms where both exist
4. Remove the `b';'` encoder arm (`encode_sized_body(';', ...)`)
5. Remove the `b';'` decoder arm
6. In `RevEncoder::emit`, remove the `List` arm (the `Array` arm already uses `[`/`]`)

Run `cargo check -p rex-core` — it should compile with just `Array` warnings about changed output.

### Step 2: Rename `Value::Map` → `Value::Object`

No conflict — `Object` doesn't exist yet.

1. Rename the `Map(Vec<(Value, Value)>)` variant to `Object(Vec<(Value, Value)>)`
2. Find-and-replace all `Value::Map(` → `Value::Object(`
3. Update the encoder to emit `{`/`}` instead of `[size]:[body]`
4. Remove the `b':'` decoder arm
5. Update the `b'{'` decoder to handle objects (see section 3 above)
6. Rename `RevEncoder::emit_map` to `emit_object`, update to use `{`/`}` delimiters with schema sharing

Run `cargo check -p rex-core`.

### Step 3: Remove lazy evaluation

1. Remove `LazySpan` struct and `RexValue::Lazy` variant from `interpret.rs`
2. Remove all `RexValue::Lazy` match arms in `interpret.rs` (6 sites)
3. Remove `RexValue::Lazy` match arms in `rex-cli` and `rex-serve`

Run `cargo check` across all crates.

### Step 4: Update downstream crates

1. `rex-cli/src/main.rs` — update `Value::List`/`Map` references and remove `Lazy` display
2. `rex-serve/src/refs.rs` and `src/opcodes.rs` — remove `Lazy` arms
3. `rex-node/src/lib.rs` — rename variants
4. `rex-luajit/src/lib.rs` — rename variants

### Step 5: Fix tests

Update all test expectations:
- Encoded output strings change (e.g., `6;2+4+6+` → `[2+4+6+]`, `l:4,name3,Ada5,score2-+` → `{4,name3,Ada5,score2-+}`)
- Constructor calls change (`Value::List(...)` → `Value::Array(...)`, `Value::Map(...)` → `Value::Object(...)`)
- Remove any lazy-evaluation tests

Run `cargo test` across all crates.

## Verification

```sh
cargo test -p rex-core          # all tests pass with new encoding
cargo test -p rex-cli            # CLI tests pass
cargo test                       # all crates pass

# Encoder output uses [] and {} instead of ; and :
echo '[1, 2, 3]' | rex encode   # should output [2+4+6+]
echo '{"a":1}' | rex encode     # should output {1,a2+}

# Round-trip
echo '{"a":1}' | rex encode | rex decode --pretty  # {"a": 1}

# Compile + run still works
echo '1 + 2' | rex run          # 3
echo 'x = 42\nx' | rex run     # 42

# Template literals still work (chain encoding unchanged)
echo '`hello ${name}`' | rex compile -c  # uses . tag, not affected
```

## What NOT to Do

- Don't delete `encode_sized_body` — chains (`.`) still need it
- Don't implement the `return` tag (`;`) — that's a separate task after this migration (see `AGENT-INSTRUCTIONS-EARLY-RETURN.md`)
- Don't change string chains (`.`) — they keep their size prefix
- Don't change REXC control flow tags — `?`, `!`, `|`, `&`, `>`, `<` are unchanged
- Don't worry about backward compatibility with v1 bytecode
- Don't implement indexed containers (`#`) in this migration — that's a follow-up
- Don't delete `is_data()` — update it for the new variant names
