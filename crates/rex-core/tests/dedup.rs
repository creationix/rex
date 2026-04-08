//! Tests for pointer deduplication correctness.
//! These test that compile() (with dedup) produces bytecode that the interpreter
//! evaluates to the same result as compile_no_dedup() (without dedup).

use rex_core::heap::{Value, Heap};
use rex_core::interpret::{self, Context};

/// Serialize a heap Value to a comparable string representation.
fn value_to_string(v: Value, heap: &Heap) -> String {
    if v.is_none() { return "none".into(); }
    if v.is_null() { return "null".into(); }
    if let Some(b) = v.as_bool() { return format!("{b}"); }
    if let Some(n) = v.as_i64() { return format!("{n}"); }
    if let Some(f) = v.as_f64(heap) { return format!("{f}"); }
    if let Some(s) = v.as_str(heap) { return format!("{s:?}"); }
    if v.is_array() {
        let items: Vec<String> = heap.array_items(v).iter()
            .map(|&item| value_to_string(item, heap))
            .collect();
        return format!("[{}]", items.join(", "));
    }
    if v.is_object() {
        let pairs: Vec<String> = heap.object_pairs(v).iter()
            .map(|&(k, val)| format!("{:?}: {}", heap.resolve_str(k), value_to_string(val, heap)))
            .collect();
        return format!("{{{}}}", pairs.join(", "));
    }
    format!("{v:?}")
}

/// Compile source with dedup, verify decode roundtrip, run, return string repr.
fn eval_dedup(source: &str) -> String {
    let bytecode = rex_core::compile(source);
    rex_core::bytecode::decode(&bytecode)
        .unwrap_or_else(|e| panic!("decode failed (dedup): {e}\n  source: {source}\n  bytecode: {bytecode}"));
    let ctx = Context::default();
    let result = interpret::run(&bytecode, ctx)
        .unwrap_or_else(|e| panic!("runtime error (dedup): {e}\n  source: {source}\n  bytecode: {bytecode}"));
    value_to_string(result.value, &result.heap)
}

/// Compile source without dedup, run, return string repr.
fn eval_no_dedup(source: &str) -> String {
    let bytecode = rex_core::compile_no_dedup(source);
    let ctx = Context::default();
    let result = interpret::run(&bytecode, ctx)
        .unwrap_or_else(|e| panic!("runtime error (no dedup): {e}\n  source: {source}\n  bytecode: {bytecode}"));
    value_to_string(result.value, &result.heap)
}

/// Assert that dedup and no-dedup produce the same result.
fn assert_dedup_matches(source: &str) {
    let with = eval_dedup(source);
    let without = eval_no_dedup(source);
    assert_eq!(
        with, without,
        "dedup mismatch!\n  source: {source}\n  dedup:    {with}\n  no_dedup: {without}"
    );
}

// ── Basic dedup ────────────────────────────────────────────────────────

#[test]
fn dedup_simple_duplicate_strings() {
    assert_dedup_matches(r#"
        x = "hello"
        y = "hello"
        x
    "#);
}

#[test]
fn dedup_duplicate_objects() {
    assert_dedup_matches(r#"
        x = {name: "Ada", score: 95}
        y = {name: "Ada", score: 95}
        x
    "#);
}

// ── Duplicate keys across objects ──────────────────────────────────────

#[test]
fn dedup_shared_key_different_values() {
    assert_dedup_matches(r#"
        x = {ok: false, error: "bad"}
        {ok: true}
    "#);
}

#[test]
fn dedup_shared_key_in_return() {
    assert_dedup_matches(r#"
        return {ok: false, error: "denied"}
        {ok: true}
    "#);
}

// ── Return inside conditionals with duplicate keys ────────────────────

#[test]
fn dedup_return_in_unless_with_trailing_object() {
    assert_dedup_matches(r#"
        x = none
        unless x do
            return {ok: false, error: "denied"}
        end
        {ok: true}
    "#);
}

#[test]
fn dedup_return_in_when_with_trailing_object() {
    assert_dedup_matches(r#"
        x = 42
        when x do
            return {ok: true, value: x}
        end
        {ok: false}
    "#);
}

#[test]
fn dedup_multiple_returns_shared_keys() {
    // The middleware pattern: multiple branches returning objects with shared keys
    assert_dedup_matches(r#"
        x = none
        unless x do
            return {ok: false, error: "missing"}
        end
        when x do
            return {ok: true, value: x}
        end
        {ok: false, error: "unreachable"}
    "#);
}

#[test]
fn dedup_nested_unless_in_when() {
    // The exact API middleware pattern that originally broke
    assert_dedup_matches(r#"
        api-key = none
        unless api-key do
            return {ok: false, error: "missing_api_key"}
        end
        key-valid = none
        unless key-valid do
            return {ok: false, error: "invalid_api_key"}
        end
        {ok: true}
    "#);
}

// ── Dedup across when/else chains ─────────────────────────────────────

#[test]
fn dedup_when_else_shared_strings() {
    assert_dedup_matches(r#"
        when x do
            {status: "ok", code: 200}
        else
            {status: "error", code: 500}
        end
    "#);
}

// ── Dedup with comprehensions ─────────────────────────────────────────

#[test]
fn dedup_comprehension_with_shared_keys() {
    assert_dedup_matches(r#"
        items = [{name: "a"}, {name: "b"}]
        [{name: item.name} for item in items]
    "#);
}

// ── Larger programs ───────────────────────────────────────────────────

#[test]
fn dedup_http_handler_pattern() {
    assert_dedup_matches(r#"
        when method == "GET" do
            return {ok: true, data: "list"}
        end
        when method == "POST" do
            return {ok: true, data: "created"}
        end
        {ok: false, error: "method_not_allowed"}
    "#);
}

#[test]
fn dedup_set_navigation_via_pointer() {
    // res.status = 401 appears twice — the second gets deduped as a pointer.
    // eval_set must follow the pointer and still perform the write.
    assert_dedup_matches(r#"
        unless x do
            res.status = 401
            return {ok: false, error: "first"}
        end
        unless y do
            res.status = 401
            return {ok: false, error: "second"}
        end
        {ok: true}
    "#);
}

#[test]
fn dedup_complex_handler_with_multiple_branches() {
    // The full articles handler pattern: GET/POST branches with shared keys,
    // validation, db ops, and a fallback.
    assert_dedup_matches(r#"
        when method == "GET" do
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
        {ok: false, error: "method_not_allowed"}
    "#);
}

#[test]
fn dedup_middleware_with_guard_returns() {
    assert_dedup_matches(r#"
        unless token do
            return {ok: false, error: "unauthorized"}
        end
        unless valid do
            return {ok: false, error: "forbidden"}
        end
        principal = token
    "#);
}

// ── Schema pointer regression tests ────────────────────────────────

#[test]
fn dedup_schema_no_double_side_effects() {
    // When objects share a schema, the schema source's values must not
    // be evaluated during key extraction. If they were, the counter
    // would be incremented an extra time.
    assert_dedup_matches(r#"
        counter = 0
        counter = counter + 1
        a = {x: counter}
        counter = counter + 1
        b = {x: counter}
        [a.x, b.x]
    "#);
}

#[test]
fn dedup_schema_with_mutation_values() {
    // Ensure shared-schema objects with mutation expressions as values
    // produce correct results — the schema scan must skip values, not
    // evaluate them.
    assert_dedup_matches(r#"
        n = 0
        n = n + 1
        first = {val: n}
        n = n + 1
        second = {val: n}
        first.val + second.val
    "#);
}

#[test]
fn dedup_schema_comprehension_shared_keys_multi() {
    // Multi-key variant of the original comprehension bug — ensures
    // schema scanning works with multiple keys.
    assert_dedup_matches(r#"
        items = [{name: "a", score: 1}, {name: "b", score: 2}]
        [{name: item.name, score: item.score} for item in items]
    "#);
}
