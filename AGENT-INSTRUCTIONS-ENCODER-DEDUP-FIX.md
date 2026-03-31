# Instructions: Fix encode_dedup Pointer Delta Calculation

## Problem

The `RevEncoder` in `crates/rex-core/src/bytecode.rs` produces invalid pointer deltas for complex programs with many dedup opportunities. The decoder fails with "unexpected end of input" and the interpreter produces wrong results.

## How to Reproduce

```sh
# This test fails — the deduped bytecode can't be decoded:
cargo test -p rex-core --test dedup dedup_complex_handler_with_multiple_branches

# The decoder catches the bad pointer:
echo 'when method == "GET" do
    items = [json.parse(a.value) for a in db.list("article:")]
    return {ok: true, articles: [{slug: a.slug, title: a.title} for a in items]}
end
when method == "POST" do
    input = json.parse(body)
    unless input and input.slug and input.title and input.body do
        return {ok: false, error: "missing_fields"}
    end
    record = {slug: input.slug, title: input.title, body: input.body}
    return {ok: true, slug: input.slug}
end
{ok: false, error: "method_not_allowed"}' | cargo run -p rex-cli -- compile | cargo run -p rex-cli -- decompile
# Error: decode error at ...: unexpected end of input
```

Simple programs dedup correctly. The bug appears when there are many pointer targets spread across conditional branches, comprehensions, and return statements.

## Root Cause

The `RevEncoder` builds bytecode right-to-left, then reverses the buffer. It records each value's position after emitting it (`self.pos`), and computes pointer deltas as `self.pos - target_left`.

A pointer delta should be the distance from the **right edge of the pointer** (the byte after the `^` tag) to the **left edge of the target** (the first byte of the target value, typically a varint). In the reversed buffer, this corresponds to `self.pos - target_left` at the time the pointer is written.

However, this calculation is wrong in some cases. The `RevEncoder.pos` field counts bytes pushed, but the relationship between `pos` at write time and the final forward position after reversal may be off by one or more bytes in certain scenarios — particularly when:

1. Pointers reference targets inside compound structures (calls, blocks, conditionals)
2. Multiple levels of nesting create long chains of pointers
3. String dedup and value dedup interact (both `write_string` and `write` have independent dedup paths)

## How to Debug

**Use the decoder to validate.** The decoder (`bytecode::decode`) parses bytecode left-to-right and will fail if a pointer delta lands on the wrong byte. This is the most reliable way to catch bad deltas:

```rust
let bc = rex_core::compile(source);
rex_core::bytecode::decode(&bc).expect("decode should succeed");
```

**Compare dedup vs no-dedup.** The `compile_no_dedup` function produces bytecode without pointers. If the interpreter produces different results for the same source with `compile` vs `compile_no_dedup`, the dedup is wrong:

```rust
let with = compile(source);
let without = compile_no_dedup(source);
// Run both and compare results
```

**Use the CLI tools** for programs under ~10 bytes:
```sh
echo 'expr' | cargo run -p rex-cli -- compile | cargo run -p rex-cli -- inspect
echo 'expr' | cargo run -p rex-cli -- compile | cargo run -p rex-cli -- decompile
```

**Don't manually trace bytecode** for larger programs — the reversed buffer with varints and nested pointers is too error-prone to follow by hand.

## Where to Fix

The fix is in `crates/rex-core/src/bytecode.rs`, in the `RevEncoder` implementation.

### Key locations

1. **`RevEncoder::write`** (line ~602) — the main dedup path for non-string values. Records `(self.pos, len, scope_depth)` in `seen` after emitting. Computes delta as `self.pos - target_left`.

2. **`RevEncoder::write_string`** (line ~710) — the string-specific dedup path. Same recording and delta pattern.

3. **`RevEncoder::write_pointer`** (line ~578) — writes the actual pointer bytes. Computes `delta = self.pos - target_left`.

4. **`RevEncoder::emit_object`** (line ~789) — emits objects with potential schema dedup. Writes a schema pointer with its own delta calculation.

### The invariant

For any pointer in the final (reversed) bytecode:
- The pointer is `[varint delta][^]`
- The interpreter reads the varint, reads `^`, then `target = current_pos + delta`
- `target` must point to the **first byte** of the target value (its varint prefix, or its tag if it has no varint)

In the `RevEncoder`:
- `self.pos` grows monotonically as bytes are pushed (right-to-left)
- After `buf.reverse()`, position `P` in the reverse buffer maps to position `total - 1 - P` in the forward buffer
- The delta between two positions in the reverse buffer equals the delta in the forward buffer (reversal preserves gaps)

So the formula `delta = self.pos - target_left` should be correct IF `target_left` is recorded at the right moment. Verify that:
- `target_left` is recorded AFTER the full value is emitted (including any varints, tags, and body)
- The recording happens at `self.pos` which is the position of the leftmost byte of the value
- No off-by-one from the `push` ordering

### Suggested approach

1. Add a validation pass: after `encode_dedup`, immediately `decode` the result and assert success. This catches all bad pointers at encode time.

2. Write a focused test that isolates the minimal failing case. Start with the complex handler test and remove pieces until you find the smallest program that fails.

3. Add logging to `write` and `write_string` to print the delta, target_left, and self.pos for each pointer written. Compare these with what the decoder expects.

## Verification

```sh
# All existing tests must pass
cargo test -p rex-core

# The complex handler test must pass (currently fails)
cargo test -p rex-core --test dedup dedup_complex_handler_with_multiple_branches

# The decoder must accept all deduped output
# (this is enforced by the decode check in eval_dedup)

# rex-serve should work with compile() instead of compile_no_dedup():
# In crates/rex-serve/src/router.rs, change compile_no_dedup to compile, then:
cargo run -p rex-serve -- --dir examples/knowledge-base --port 4000
sqlite3 examples/knowledge-base/data.db "INSERT INTO kv VALUES('keys:demo','1')"
curl -X POST http://localhost:4000/api/articles -H 'Authorization: demo' \
  -d '{"slug":"hello","title":"Hello","body":"# Hello"}'
# Should return 201, not 422
```

## Context

- The `compile_no_dedup()` function in `lib.rs` is the current workaround used by rex-serve
- Two interpreter bugs related to dedup were already fixed (eval_block object disambiguation, eval_set pointer places) — those are correct and should stay
- The 13 tests in `tests/dedup.rs` cover runtime correctness — 12 pass, 1 fails due to this encoder bug
- The failing test also verifies decode roundtrip, which is how the bad delta was caught
