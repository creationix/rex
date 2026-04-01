//! Tests for local variable renaming (minification).
//! Verifies that renamed bytecode produces the same runtime results as debug bytecode.

use rex_core::interpret::{self, Context};

/// Compile with and without minification, verify both produce the same result.
fn assert_same_result(source: &str) {
    let minified = rex_core::compile(source);
    let debug = rex_core::compile(source);

    let result_min = interpret::run(&minified, Context::default())
        .unwrap_or_else(|e| panic!("minified runtime error: {e}\n  source: {source}\n  bytecode: {minified}"));
    let result_dbg = interpret::run(&debug, Context::default())
        .unwrap_or_else(|e| panic!("debug runtime error: {e}\n  source: {source}\n  bytecode: {debug}"));

    assert_eq!(
        format!("{:?}", result_min.value),
        format!("{:?}", result_dbg.value),
        "minified vs debug mismatch for: {source}\n  minified bc: {minified}\n  debug bc:    {debug}"
    );
}

/// Compile both ways and return (debug_len, minified_len).
fn sizes(source: &str) -> (usize, usize) {
    let debug = rex_core::compile(source);
    let minified = rex_core::compile(source);
    (debug.len(), minified.len())
}

// ── Semantic equivalence ──────────────────────────────────────────────

#[test]
fn minify_simple_assignment() {
    assert_same_result("x = 42\nx");
}

#[test]
fn minify_multiple_locals() {
    assert_same_result("a = 1\nb = 2\nc = a + b\nc");
}

#[test]
fn minify_compound_assignment() {
    assert_same_result("x = 10\nx += 5\nx");
}

#[test]
fn minify_for_loop() {
    assert_same_result("total = 0\nfor i in 1..5 do\n  total += i\nend\ntotal");
}

#[test]
fn minify_nested_conditionals() {
    assert_same_result("x = 10\nwhen x == 10 do\n  result = \"yes\"\nelse\n  result = \"no\"\nend\nresult");
}

#[test]
fn minify_fibonacci() {
    assert_same_result(
        "a = 1\nb = 1\ni = 0\nwhile i < 8 do\n  c = a + b\n  a = b\n  b = c\n  i += 1\nend\na",
    );
}

#[test]
fn minify_no_locals_unchanged() {
    // No assignments → no renaming → identical bytecode
    let debug = rex_core::compile("1 + 2");
    let minified = rex_core::compile("1 + 2");
    assert_eq!(debug, minified);
}

// ── Size savings ──────────────────────────────────────────────────────

#[test]
fn minify_reduces_size() {
    let programs = [
        ("x = 42\nx", "single assignment"),
        ("result = 1 + 2\nresult", "expression result"),
        ("a = 1\nb = 2\nc = a + b\nc", "three locals"),
        (
            "a = 1\nb = 1\ni = 0\nwhile i < 8 do\n  c = a + b\n  a = b\n  b = c\n  i += 1\nend\na",
            "fibonacci",
        ),
        (
            "request-id = \"abc\"\nroute-key = \"GET /\"\nstatus = 200\n{id: request-id, route: route-key, status: status}",
            "handler-like",
        ),
    ];

    println!("\n{:<20} {:>8} {:>8} {:>8}", "program", "debug", "minified", "savings");
    println!("{}", "-".repeat(50));

    for (source, label) in programs {
        let (dbg, min) = sizes(source);
        let saved = dbg as i64 - min as i64;
        let pct = if dbg > 0 { saved as f64 / dbg as f64 * 100.0 } else { 0.0 };
        println!("{:<20} {:>8} {:>8} {:>5} ({:.0}%)", label, dbg, min, saved, pct);
        assert!(min <= dbg, "minified should not be larger: {label}");
        assert_same_result(source);
    }
}
