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

## Core Concept

The dedup logic is extremely simple:

1. **When writing a value**, store the current total document size (`self.pos`) indexed by the value's hash/key.
2. **When writing a duplicate**, look up the stored entry. If a pointer is cheaper to write than the value itself, emit a pointer.
3. **The pointer delta** is simply: `current self.pos - stored self.pos`. That's it.

`self.pos` tracks total bytes written so far. The delta between two `self.pos` values is preserved through the final `buf.reverse()` — if value A was recorded at pos=10 and we're now at pos=25, the delta is 15, and that same distance of 15 holds in the forward buffer after reversal.

Don't think about reverse encoding, the "right edge of the pointer", the "left edge of the target", or the size of the pointer vs target in terms of buffer positions. It's just the delta of total document sizes.

## Root Cause

The `RevEncoder` currently over-complicates this. The recording and delta computation may be wrong in some cases because the code conflates buffer positions with document sizes or introduces off-by-one errors when:

1. Pointers reference targets inside compound structures (calls, blocks, conditionals)
2. Multiple levels of nesting create long chains of pointers
3. String dedup and value dedup interact (both `write_string` and `write` have independent dedup paths)

The fix should simplify the logic to match the core concept above: record `self.pos` after writing, compute `self.pos - recorded_pos` when deduplicating.

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

The dedup invariant is simple:

1. `self.pos` = total bytes written so far (grows monotonically)
2. After writing a value, record `self.pos` keyed by the value's hash
3. When encountering a duplicate, `delta = self.pos - recorded_pos`
4. The pointer `[varint delta][^]` must be cheaper (fewer bytes) than re-emitting the value

This works because reversal preserves deltas — the gap between any two `self.pos` snapshots is the same in the forward buffer.

Verify that:
- `self.pos` is recorded AFTER the full value is emitted (including any varints, tags, and body)
- No off-by-one from the `push` ordering
- The cost comparison uses the actual pointer size (`varint_len(delta) + 1`) vs the target's encoded length

### Suggested approach

1. **Simplify the recording**: ensure every dedup path (in `write` and `write_string`) follows the same pattern: emit the value, then record `self.pos`. The stored value is the total document size at that point, nothing more.

2. **Simplify the delta**: when a duplicate is found, compute `delta = self.pos - recorded_pos`. Don't reason about buffer positions, left edges, or right edges.

3. **Add a validation pass**: after `encode_dedup`, immediately `decode` the result and assert success. This catches all bad pointers at encode time.

4. **Write a focused test** that isolates the minimal failing case. Start with the complex handler test and remove pieces until you find the smallest program that fails.

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
