# Instructions: Length-Prefixed & Indexed Containers — COMPLETE

## Goal

Two related changes to the container encoding:

1. **Optional length prefix** — any paired container (`[]`, `{}`, `()`) can be preceded by a varint byte count. This lets the interpreter skip branches by jumping instead of parsing. Fixes pointer dedup safety across conditional branches.

2. **Indexed containers** — the `#` marker after an opener provides random-access element pointers for lazy evaluation of large data. Skipping uses the length prefix (not an internal end pointer), so `#` is purely for random access.

## Prerequisites

- Bytecode v2 migration — **COMPLETE**
- Early return — **COMPLETE**

## Key Document

Read `packages/rusty-rex/bytecode-v2.md` — the "Containers" and "Indexed Containers" sections document both designs.

---

## Part 1: Length Prefixes

### Background

The interpreter skips untaken branches (when/unless/or/and) by calling `skip_value()`, which recursively parses each child to find the end. This is slow and fragile — pointer deduplication can create cross-branch references that break during skipping (see `AGENT-INSTRUCTIONS-POINTER-DEDUP-FIX.md`).

The fix: the encoder puts a byte-count prefix before container values that might be skipped. The interpreter reads the prefix and jumps past the container without parsing.

### Encoding Format

A varint before a container opener gives the byte count of the body (between delimiters, excluding delimiters):

```
[body]           → no prefix — must parse all children
6[2+4+6+]        → length-prefixed — body is 6 bytes, skippable
```

Since `[`, `{`, `(` are not b64 digits, the parser always distinguishes a length prefix from an unprefixed container. The varint is consumed as `varint_raw` by the decoder and harmlessly ignored by the tree decoder (which always parses all children).

**Length prefixes only work before container openers.** Before scalars, the varint digits would merge with the value's own varint. So only container-valued branches get prefixed — scalars are small enough to skip by parsing.

### Where to Emit Length Prefixes

The encoder length-prefixes **container-valued branch children** inside conditionals (`?`, `!`, `|`, `&`). The condition (child 0) is always evaluated and never prefixed. Non-conditional compounds (for-in, for-of, while) don't skip branches.

```
?(cond 5{then-body} 7{else-body})   → branches are length-prefixed
>(iterable bindings {body})          → not conditional, no prefix
```

### Changes — Encoder (`bytecode.rs`)

#### Simple encoder

Update `encode_compound` to length-prefix container-valued branch children:

```rust
fn encode_compound(modifier: char, open: char, close: char, items: &[Value], out: &mut String) {
    out.push(modifier);
    out.push(open);
    for (i, item) in items.iter().enumerate() {
        if i > 0 && is_conditional_modifier(modifier) && is_container(item) {
            encode_sized_value(item, out);
        } else {
            encode_into(item, out);
        }
    }
    out.push(close);
}

fn is_conditional_modifier(modifier: char) -> bool {
    matches!(modifier, '?' | '!' | '|' | '&')
}

fn is_container(value: &Value) -> bool {
    match value {
        Value::Block(_) | Value::Array(_) | Value::Object(_) | Value::Call(_)
        | Value::When(_) | Value::Unless(_) | Value::Or(_) | Value::And(_)
        | Value::ForIn(_) | Value::ForOf(_) | Value::While(_)
        | Value::ListCompIn(_) | Value::ListCompOf(_) | Value::ListCompWhile(_)
        | Value::MapCompIn(_) | Value::MapCompOf(_) | Value::MapCompWhile(_)
        | Value::Chain(_) => true,
        // Return is transparent — it's skippable if its child is a container.
        // The `;` prefix is just one byte; the child value is what needs
        // the length prefix for efficient skipping.
        Value::Return(child) => is_container(child),
        _ => false,
    }
}

/// Encode a value with a length prefix (varint byte count before the value).
/// For Return, the length prefix wraps the entire `;[value]` sequence, so
/// the interpreter can skip the return and its child in one jump.
fn encode_sized_value(value: &Value, out: &mut String) {
    let mut body = String::new();
    encode_into(value, &mut body);
    out.push_str(&encode_varint(body.len() as u64));
    out.push_str(&body);
}
```

#### RevEncoder (dedup encoder)

Update `emit_compound` — the RevEncoder writes right-to-left, so the varint is pushed after the child content (appears before it in the final output):

```rust
fn emit_compound(&mut self, modifier: u8, open: u8, close: u8, items: &[Value]) {
    let is_conditional = matches!(modifier, b'?' | b'!' | b'|' | b'&');
    if is_conditional { self.scope_depth += 1; }
    self.push(close);
    for (i, item) in items.iter().enumerate().rev() {
        if i > 0 && is_conditional && is_container(item) {
            let before = self.pos;
            self.write(item);
            let body_len = self.pos - before;
            self.push_varint(body_len as u64);
        } else {
            self.write(item);
        }
    }
    self.push(open);
    self.push(modifier);
    if is_conditional { self.scope_depth -= 1; }
}
```

#### Tree decoder — no changes needed

The tree decoder already ignores `varint_raw` for `[`, `{`, `(` tags. Length-prefixed containers decode identically to unprefixed ones.

### Changes — Interpreter (`interpret.rs`)

#### `skip_value_fast`

Add a fast-path skip that uses length prefixes when available:

```rust
fn skip_value_fast(&mut self) -> Result<(), RexError> {
    if self.at_end() { return Ok(()); }
    let save = self.pos;
    let raw = self.read_raw();
    if !raw.is_empty() && matches!(self.peek(), b'[' | b'{' | b'(') {
        let size = parse_uint(raw) as usize;
        self.read_byte(); // consume opener
        self.pos += size;
        self.read_byte(); // consume closer
        return Ok(());
    }
    self.pos = save;
    self.skip_value()
}
```

#### Update branch skipping

Replace `self.skip_value()` with `self.skip_value_fast()` in `eval_when`, `eval_unless`, `eval_or`, and `eval_and` — specifically when skipping the then or else branch.

#### Update `skip_value`

Handle length-prefixed containers encountered during recursive skipping:

```rust
b'(' | b'[' | b'{' => {
    if !raw.is_empty() {
        let size = parse_uint(raw) as usize;
        self.pos += size;
        let closer = match tag { b'(' => b')', b'[' => b']', _ => b'}' };
        if self.peek() == closer { self.read_byte(); }
    } else {
        let closer = match tag { b'(' => b')', b'[' => b']', _ => b'}' };
        self.skip_until(closer)?;
    }
}
b';' => {
    // Return: prefix compound — skip the child value
    self.skip_value()?;
}
```

---

## Part 2: Indexed Containers

### Background

The v2 migration removed lazy containers (`;` `:` tags) and made all data eager. This is a known regression for large JSON payloads. Indexed containers restore random-access behavior with a better design.

### Encoding Format

When a container has `#` immediately after the opening delimiter, it is indexed:

```
[#<count><ptr0><ptr1>...<ptrN><elem0><elem1>...<elemN>]
```

- `count` — varint giving the number of elements
- `ptr0..ptrN` — fixed-width pointers, one per element. Each is a relative delta from the end of the index to the start of that element.
- Pointer width — minimum number of b64 digits needed to reach the farthest element. All pointers use this same width.

Skipping an indexed container uses the **length prefix** (before the opener), not an internal end pointer:

```
a3[#...]         → length prefix = a3 (639 bytes), indexed with random access
[#...]           → indexed but not skippable (no length prefix)
```

#### Indexed objects

For objects, the index contains one pointer per key-value pair, pointing to the start of the key. Pointers are sorted by key (byte-order comparison of the encoded key) for O(log n) binary search lookup.

### When to Index

The encoder decides which containers to index. Typical thresholds:
- Containers with >16 elements
- Data containers only (not code blocks or calls)
- Root-level containers in JSON payloads

### Design Questions (resolve before implementing)

- **Fixed-width pointer size:** minimum b64 digits needed for the farthest element. Determined by encoding all elements first, then computing the index. This means the encoder must buffer the body.
- **Nested indexing:** Should nested containers inside an indexed container also be indexed? Probably only if they exceed the threshold independently.
- **Decoder behavior:** Initially, decode indexed containers eagerly (parse all elements). True lazy evaluation (parsing on access) is a separate task requiring changes to `RexValue`.

### Changes — Encoder

Add an `encode_indexed_array` and `encode_indexed_object` function:

1. Encode all elements to a buffer
2. Compute element offsets
3. Determine pointer width (min b64 digits for max offset)
4. Emit: `[size][` `#` count ptr0..ptrN elements `]`

The length prefix (before `[`) enables skipping the entire indexed container.

### Changes — Decoder

Handle `#` after `[` or `{`:

```rust
b'[' => {
    if *pos < input.len() && input[*pos] == b'#' {
        *pos += 1; // consume '#'
        return decode_indexed_array(input, pos, resolve);
    }
    // ... normal array decoding
}
```

Initially, `decode_indexed_array` can ignore the index and decode all elements eagerly.

### Changes — Interpreter

Handle `#` after `[` or `{` in the cursor interpreter:

```rust
b'[' => {
    if self.peek() == b'#' {
        self.read_byte(); // consume '#'
        return self.eval_indexed_array();
    }
    // ... normal array eval
}
```

Initially, `eval_indexed_array` can read the index, skip past it, and evaluate all elements eagerly. True lazy evaluation is a follow-up.

### Benchmarks

Use the files in `/Users/tim/Code/routes-data/data/` for benchmarks:

| File | Size | Notes |
|------|------|-------|
| `vaskange-scraped-metadata.json` | 5KB | Baseline, no indexing expected |
| `whop-scraped-metadata.json` | 623KB | Medium |
| `styfle-scraped-metadata.json` | 760KB | Medium |
| `vercel-marketing-scraped-metadata.json` | 94MB | Large, good stress test |
| `vercel-docs-scraped-metadata.json` | 121MB | Largest |

Measure:
- **Output size** — overhead of index tables vs eager encoding
- **Random-access read** — property lookup on large objects with vs without index
- **Sequential iteration** — lazy vs eager iteration
- **Partial reads** — accessing 1 field from a 50-field object

---

## What NOT to Change

- **Don't change `Value` enum** — length prefixes and indexes are encoding details, not tree structure changes.
- **Don't always length-prefix** — only prefix container-valued branch children inside conditionals.
- **Don't remove scope-aware dedup** — keep `scope_depth` in RevEncoder as defense-in-depth.
- **Don't change the tree decoder for length prefixes** — it already ignores `varint_raw` for container openers.
- **Don't implement true lazy evaluation yet** — decode indexed containers eagerly initially. Lazy `RexValue` is a separate task.

## Implementation Order

1. **Length prefixes** (Part 1) — encoder changes, interpreter skip changes, tests
2. **Indexed containers** (Part 2) — encoder, decoder, interpreter, benchmarks

Part 1 is self-contained and unblocks safer pointer dedup. Part 2 can follow as a separate effort.

## Tests

### Length prefix round-trips

```rust
#[test]
fn roundtrip_when_with_length_prefixed_branches() {
    roundtrip(Value::Block(vec![
        Value::When(vec![
            Value::Variable("x".into()),
            Value::Block(vec![Value::Integer(1), Value::Integer(2)]),
            Value::Block(vec![Value::Integer(3)]),
        ]),
    ]));
}
```

### Interpreter skip tests

```rust
#[test]
fn skip_length_prefixed_branch() {
    assert_eq!(eval("x = none\nwhen x do\n  99\nelse\n  42\nend"), RexValue::Int(42));
}

#[test]
fn cross_branch_dedup_safe() {
    let source = r#"
        x = none
        unless x do y = 401 end
        when x do
          unless x do y = 401 end
        end
        y
    "#;
    assert_eq!(eval(source), RexValue::Int(401));
}
```

### Encoding format tests

```rust
#[test]
fn conditional_branches_are_length_prefixed() {
    let v = Value::When(vec![
        Value::Variable("x".into()),
        Value::Block(vec![Value::Integer(1)]),
    ]);
    let encoded = encode(&v);
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded, v);
}
```

## Verification

```sh
cargo test -p rex-core          # all tests pass
cargo test                      # all crates pass

echo 'x = none
unless x do "first" end
when x do
  unless x do "second" end
end' | cargo run -p rex-cli -- run
# Expected: "first"

echo '1 + 2' | cargo run -p rex-cli -- run          # 3
echo '[1, 2, 3]' | cargo run -p rex-cli -- run      # [1, 2, 3]
```

## File Summary

| File | Change |
|------|--------|
| `crates/rex-core/src/bytecode.rs` | Length-prefix branch children; indexed container encode/decode |
| `crates/rex-core/src/interpret.rs` | `skip_value_fast`; branch skip updates; indexed container eval |
| `crates/rex-core/tests/roundtrip.rs` | Tests for length-prefixed and indexed containers |
