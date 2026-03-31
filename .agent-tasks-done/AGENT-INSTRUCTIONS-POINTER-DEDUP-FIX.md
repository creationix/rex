# Instructions: Fix Pointer Deduplication in Skipped Branches

> **Status: COMPLETE.** Scope-aware deduplication added to RevEncoder. `compile_no_dedup()` workaround removed. rex-serve switched back to `compile()`.

## Problem

The bytecode encoder's pointer deduplication (`encode_dedup` in `crates/rex-core/src/bytecode.rs`) causes incorrect execution when duplicate patterns appear in conditional branches that get skipped.

### How it manifests

When the same expression appears in multiple branches (e.g., `res.status = 401` in both an `unless` and a nested `unless`), the encoder replaces the second occurrence with a pointer (`^`) referencing the first. But if the first occurrence is inside a `when`/`unless` branch that gets skipped at runtime, the pointer target is at a bytecode position the interpreter never evaluated — causing the interpreter to read garbage or skip over the wrong bytes.

### Reproduction

```rex
api-key = headers.authorization

unless api-key do
  res.status = 401
  {ok: false, error: "missing_api_key"}
end

when api-key do
  key-valid = db.get("keys:" + api-key)

  unless key-valid do
    res.status = 401                    /* ← this gets deduped to a pointer */
    {ok: false, error: "invalid_api_key"}
  end

  when key-valid do
    principal = api-key
  end
end
```

With dedup enabled, the second `res.status = 401` becomes a pointer to the first. When `api-key` is `none`, the `unless api-key` branch executes (setting status 401), but the `when api-key` branch is skipped. The pointer inside the skipped branch references a position that was already consumed — the interpreter misreads the bytecode.

### Current workaround

rex-serve uses `rex_core::compile_no_dedup()` which calls `bytecode::encode()` instead of `bytecode::encode_dedup()`. This avoids pointers entirely but produces larger bytecode.

## Root Cause

The issue is that pointer deduplication is **position-dependent** but **execution-independent**. A pointer says "the value at position X+delta is the same as what I represent." But the interpreter's cursor-based design reads bytecode sequentially — when a branch is skipped, the cursor jumps past it using `skip_value()`. If a pointer inside the skipped branch references a target that's *also* inside the skipped branch (or was already read), the position arithmetic breaks.

Specifically, `eval_set` (the `=` tag handler) reads the "place" expression first, then the "value" expression. When the place is a navigation chain like `(res$6,status)`, this is a call that reads multiple sub-values. If any of these sub-values is a pointer, the pointer's delta is relative to the pointer's position — but `skip_value` may not correctly account for pointer targets during skipping.

## Solution Approaches

### Approach 1: Scope-aware deduplication (recommended)

Only deduplicate values that appear in the **same branch scope**. Never create a pointer that crosses a conditional boundary (`?`, `!`, `|`, `&`).

In `encode_dedup`:
1. Track the current "scope depth" — increment on entering a conditional branch, decrement on leaving
2. When recording a value's position for potential reuse, tag it with its scope depth
3. When considering a pointer to a previously seen value, only allow it if the target is at the same or an ancestor scope depth

This ensures pointers only reference values that are guaranteed to be in the same execution path.

### Approach 2: Forward-only pointers within containers

Restrict pointers to only reference targets that come **later** in the bytecode (forward pointers to deduplicate shared suffixes within the same container). This is already the intended direction per the v2 spec — pointers reference positions ahead of the pointer.

The bug occurs because the current encoder creates pointers that reference positions *within other branches* of the same parent. If pointers are restricted to reference values within the same sequential container (not across conditional branches), the bug disappears.

### Approach 3: Fix skip_value to handle pointers correctly

Make `skip_value` in the interpreter correctly handle pointers during branch skipping. When skipping a value that is a pointer, the interpreter should skip the pointer itself (varint + `^` tag) without following the delta. Currently `skip_value` may be following the pointer to compute the size to skip.

Check `skip_value` for the `^` tag handling — it should just skip past the varint + tag bytes without seeking to the target.

## Files to Change

| File | Change |
|------|--------|
| `crates/rex-core/src/bytecode.rs` | `encode_dedup`: add scope tracking to prevent cross-branch pointers |
| `crates/rex-core/src/interpret.rs` | `skip_value`: verify `^` tag is handled correctly (just skip the varint + tag, don't follow) |

## Verification

```sh
# 1. The existing tests must pass
cargo test -p rex-core --lib

# 2. This program must work correctly with dedup enabled:
echo 'x = none
unless x do "first" end
when x do
  unless x do "second" end
end' | cargo run -p rex-cli -- run
# Expected: "first" (unless x fires, when x is skipped)

# 3. The rex-serve API middleware must work with compile() instead of compile_no_dedup():
# In crates/rex-serve/src/router.rs, change compile_no_dedup to compile
# Then test:
cargo run -p rex-serve -- --dir examples/knowledge-base --port 4000
curl http://localhost:4000/api/articles  # should return 401, not 200

# 4. Once fixed, update router.rs to use compile() instead of compile_no_dedup()
```

## Context

This bug was discovered during the rex-serve project. The `compile_no_dedup()` function was added to `rex_core::lib.rs` as a workaround. Once the dedup bug is fixed:

1. Remove `compile_no_dedup` from `crates/rex-core/src/lib.rs`
2. Update `crates/rex-serve/src/router.rs` to use `compile()`
3. The bytecode will be smaller due to dedup, improving startup time and memory usage
