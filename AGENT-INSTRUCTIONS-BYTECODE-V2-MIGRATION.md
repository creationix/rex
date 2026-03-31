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
| `;` tag | Sized lazy list | **Freed** — now available for `return` |
| `:` tag | Sized lazy map | **Freed** — available for future use |
| String chains | `[size].[segments]` — sized body | Same (no change — chains keep size prefix) |

### Core insight

In v1, the distinction between lazy and eager was by TAG: `;` `:`= lazy, `[]` `{}` = eager.

In v2, the distinction is by INDEX: any container with `#` after the opener is lazy/indexed/skippable. Without `#`, it's eager.

This means `Value::List` and `Value::Array` merge — both use `[]` in the bytecode. `Value::Map` and `Value::Object` merge — both use `{}`. The difference is only whether the encoder adds an index.

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

**After:** Merge `List` into `Array` and `Map` into `Object`. The laziness decision moves from the Value enum to the encoder.

```rust
pub enum Value {
    // ...
    Array(Vec<Value>),                // [] — both data arrays and code arrays
    Object(Vec<(Value, Value)>),      // {} — both data objects and code objects (replacing Map)
    Block(Vec<Value>),                // {} — code blocks (sequence, returns last)
    Call(Vec<Value>),                 // () — calls
    // ...
}
```

**Note:** `Block` stays separate because it has different semantics (returns last value, not a data structure). `Object` replaces `Map` — key-value pairs are always `(Value, Value)`.

Remove `List` and `Map` variants. Update ALL code that constructs or matches on them.

### 2. `crates/rex-core/src/bytecode.rs` — Encoder

**Remove:** The `encode_sized_body` function and all `;` / `:` tag emission.

**Update `encode_into`:**

```rust
// BEFORE:
Value::List(items) => encode_sized_body(';', items, out),
Value::Map(pairs) => { /* emit size : body */ }
Value::Array(items) => encode_paired('[', ']', items, out),

// AFTER:
Value::Array(items) => encode_paired('[', ']', items, out),  // same as before
Value::Object(pairs) => {
    out.push('[');  // wait, objects use {} not []
    // Actually:
    out.push('{');
    for (k, v) in pairs {
        encode_into(k, out);
        encode_into(v, out);
    }
    out.push('}');
}
```

**Update `RevEncoder`:** Same changes — `emit` method for `List` and `Map` become `Array` and `Object` using paired delimiters.

### 3. `crates/rex-core/src/bytecode.rs` — Decoder

**Remove:** The `b';'` and `b':'` match arms in `decode_one`.

**Update `b'['` and `b'{'`:**

The `[` decoder already reads until `]`. No change needed for arrays.

The `{` decoder needs to handle both objects and blocks. In v2, `{` in the data context means object (key-value pairs). In REXC context, it could be a block. The decoder can't distinguish — it should return an `Object` if it sees key-value pairs, or a `Block` otherwise.

**Simplest approach:** The `{` decoder reads values until `}`. If the values are alternating string/value pairs, it's an object. Otherwise it's a block. But this is fragile.

**Better approach:** The decoder always returns the raw children. The caller/interpreter decides the semantics. Use `Value::Object` for the `{` case with an even number of children where odd positions are strings, and `Value::Block` otherwise.

**Or simplest:** Just decode `{` as a flat `Vec<Value>` and let the interpreter/consumer decide. You could add a `Value::Braced(Vec<Value>)` that the higher layers interpret contextually.

**Recommended:** Keep `Object` and `Block` separate in the `Value` enum but decode `{` by checking if it looks like key-value pairs:

```rust
b'{' => {
    let mut children = Vec::new();
    while self.peek() != b'}' && !self.at_end() {
        children.push(self.read_value()?);
    }
    self.read_byte(); // consume '}'

    // Heuristic: if children are key-value pairs (even count, odd positions are strings)
    if children.len() % 2 == 0 && children.iter().step_by(2).all(|c| matches!(c, Value::String(_))) {
        let pairs: Vec<(Value, Value)> = children.chunks(2)
            .map(|chunk| (chunk[0].clone(), chunk[1].clone()))
            .collect();
        Ok(Value::Object(pairs))
    } else {
        Ok(Value::Block(children))
    }
}
```

**Schema-shared objects:** The decoder also needs to handle the schema case — if the first child resolves to an array or object (not a string), it's a schema pointer. See the v2 spec "Schema sharing" section.

### 4. `crates/rex-core/src/bytecode.rs` — Indexed Containers

**New feature.** When the encoder wants a container to be lazy/indexed, it emits `#` after the opener:

```
[#<end_ptr><count><ptr0>...<ptrN><elements>]
```

For now, the encoder can choose to index based on a threshold (e.g., containers with >16 elements). Or the caller can specify.

The decoder needs to handle `#` after `[` or `{`:

```rust
b'[' => {
    if self.peek() == b'#' {
        self.read_byte(); // consume '#'
        return self.decode_indexed_array();
    }
    // ... normal array decoding
}
```

**`decode_indexed_array`:**
1. Read `end_ptr` (first fixed-width pointer — gives body end position for skipping)
2. Read `count` (varint — number of elements)
3. Compute pointer width from end_ptr (how many b64 digits)
4. Read `count` pointers (each `width` b64 digits)
5. The elements start after the pointers
6. Return a `Value::Array` with all elements decoded (or a lazy handle)

For the initial migration, decoding indexed containers eagerly (reading all elements) is fine. Lazy evaluation can be added later.

### 5. `crates/rex-core/src/lower.rs`

**Replace all `Value::List` → `Value::Array`** and **`Value::Map` → `Value::Object`**.

Currently:
```rust
Value::List(items)  // pure data array
Value::Map(pairs)   // pure data object
Value::Array(items) // code array (eager)
```

After:
```rust
Value::Array(items)  // all arrays (data and code)
Value::Object(pairs) // all objects (data and code)
```

The `is_data()` function that distinguished List from Array can be removed or simplified.

### 6. `crates/rex-core/src/interpret.rs`

**Remove `RexValue::Lazy(LazySpan)`** for now (lazy eval requires working with raw bytecode, not Value trees).

**Update all `Value::List` → `Value::Array`** and `Value::Map` → `Value::Object` matches.

The interpreter already handles `Array` and `Object` — just need to remove the `List`/`Map`/`Lazy` code paths.

### 7. `crates/rex-core/src/decompile.rs`

**Remove `Value::List`** handling (merge into `Value::Array`).
**Remove `Value::Map`** handling (merge into `Value::Object`).

### 8. `crates/rex-core/src/json_fast.rs`

**Replace `Value::List` → `Value::Array`** and `Value::Map` → `Value::Object`**.

Currently `json_fast` returns `Value::List` for JSON arrays and `Value::Map` for JSON objects. Change to `Value::Array` and `Value::Object`.

### 9. `crates/rex-node/src/lib.rs`

Same — `Value::List` → `Value::Array`, `Value::Map` → `Value::Object`.

### 10. `crates/rex-luajit/src/lib.rs` and `src/ffi.rs`

Same renames.

### 11. All test files

Update all tests that reference `Value::List` or `Value::Map`.

Search for: `List(`, `Map(`, `"lazy"`, `encode_sized_body`, and update.

## Migration Strategy

1. **Rename `Value::List` → `Value::Array` and `Value::Map` → `Value::Object`** across the entire codebase. This is a mechanical find-and-replace. Run `cargo test` after — many tests will break because encoded output changes.

2. **Update the encoder** to use `[` `]` for arrays and `{` `}` for objects instead of `[size];` and `[size]:`. Remove `encode_sized_body`.

3. **Update the RevEncoder** (dedup encoder) similarly.

4. **Update the decoder** to remove `b';'` and `b':'` match arms. Update `b'{'` to decode objects.

5. **Update tests** — the expected encoded output strings change. For example:
   - `6;2+4+6+` (v1 list) → `[2+4+6+]` (v2 array)
   - `l:4,name3,Ada5,score2-+` (v1 map) → `{4,name3,Ada5,score2-+}` (v2 object)

6. **Add indexed container support** (can be a follow-up — not blocking the core migration).

## Verification

```sh
cargo test -p rex-core          # all tests pass with new encoding
cargo test -p rex-cli            # CLI tests pass

# Encoder output uses [] and {} instead of ; and :
echo '[1, 2, 3]' | rex encode   # should output [2+4+6+]
echo '{"a":1}' | rex encode     # should output {1,a2+}

# Round-trip
echo '{"a":1}' | rex encode | rex decode --pretty  # {"a": 1}

# Compile + run still works
echo '1 + 2' | rex run          # 3
echo 'x = 42\nx' | rex run     # 42
```

## What NOT to Do

- Don't implement lazy evaluation yet — just decode everything eagerly for now
- Don't implement the `return` tag (`;`) — that's a separate task after this migration
- Don't change string chains (`.`) — they keep their size prefix
- Don't change REXC control flow tags — `?`, `!`, `|`, `&`, `>`, `<`, `#` are unchanged
- Don't worry about backward compatibility with v1 bytecode
