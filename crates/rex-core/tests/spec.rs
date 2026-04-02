//! Golden spec test runner.
//!
//! Parses `docs/spec-by-example.md` and runs each test case.
//!
//! ## Block types
//!
//! - `rex` — input: compile and run in the shared VM, preserving state
//! - `json` — output check: structural match against the last expression result
//! - `json vars` — output check: structural match against all current variables
//! - `rexc` — output check: exact match against compiled bytecode of previous rex block
//!
//! Multiple blocks per test, interleaved freely. State carries across rex blocks.

use std::collections::HashMap;

#[derive(Debug)]
enum Step {
    Rex(String),
    CheckJson(String),
    CheckJsonVars(String),
    CheckBytecode(String),
}

#[derive(Debug)]
struct SpecTest {
    name: String,
    steps: Vec<Step>,
    #[allow(dead_code)]
    line: usize,
}

fn parse_spec(markdown: &str) -> Vec<SpecTest> {
    let mut tests = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut steps: Vec<Step> = Vec::new();
    let mut test_line: usize = 0;

    let mut in_fence = false;
    let mut fence_lang = String::new();
    let mut fence_body = String::new();

    for (lineno, line) in markdown.lines().enumerate() {
        if in_fence {
            if line.starts_with("```") {
                let body = fence_body.trim().to_string();
                match fence_lang.as_str() {
                    "rex" => steps.push(Step::Rex(body)),
                    "json" => steps.push(Step::CheckJson(body)),
                    "json vars" => steps.push(Step::CheckJsonVars(body)),
                    "rexc" => steps.push(Step::CheckBytecode(body)),
                    _ => {} // ignore unknown
                }
                in_fence = false;
                fence_body.clear();
            } else {
                if !fence_body.is_empty() { fence_body.push('\n'); }
                fence_body.push_str(line);
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("```") {
            in_fence = true;
            fence_lang = rest.trim().to_string();
            fence_body.clear();
            continue;
        }

        if line.starts_with('#') {
            // Flush previous test
            if !steps.is_empty() {
                tests.push(SpecTest {
                    name: headers.join(" > "),
                    steps: std::mem::take(&mut steps),
                    line: test_line,
                });
            }

            let level = line.chars().take_while(|&c| c == '#').count();
            let text = line[level..].trim().to_string();
            while headers.len() >= level { headers.pop(); }
            headers.push(text);
            test_line = lineno + 1;
        }
    }

    // Flush last test
    if !steps.is_empty() {
        tests.push(SpecTest {
            name: headers.join(" > "),
            steps,
            line: test_line,
        });
    }

    tests
}

fn run_test(test: &SpecTest) {
    let mut vars: HashMap<String, rex_core::heap::Value> = HashMap::new();
    let mut heap = rex_core::heap::Heap::new();
    let mut last_value = rex_core::heap::Value::NONE;
    let mut last_bytecode = String::new();

    for step in &test.steps {
        match step {
            Step::Rex(source) => {
                let bytecode = rex_core::compile(source);
                last_bytecode = bytecode.clone();

                let mut ctx = rex_core::interpret::Context::default();
                ctx.vars = std::mem::take(&mut vars);
                ctx.heap = std::mem::take(&mut heap);
                ctx.gas_limit = 1_000_000;

                let result = rex_core::interpret::run(&bytecode, ctx)
                    .unwrap_or_else(|e| panic!("[{}] runtime error: {e}\n  source: {source}", test.name));

                last_value = result.value;
                vars = result.vars;
                heap = result.heap;
            }
            Step::CheckJson(expected) => {
                let actual = value_to_json(last_value, &heap);
                let expected_json: serde_json::Value = serde_json::from_str(expected.trim())
                    .unwrap_or_else(|e| panic!("[{}] invalid expected JSON: {e}\n  text: {expected}", test.name));
                assert!(
                    json_eq_ordered(&actual, &expected_json),
                    "\n[{}] output mismatch\n  expected: {}\n  actual:   {}",
                    test.name, expected_json, actual
                );
            }
            Step::CheckJsonVars(expected) => {
                let expected_json: serde_json::Value = serde_json::from_str(expected.trim())
                    .unwrap_or_else(|e| panic!("[{}] invalid expected JSON vars: {e}\n  text: {expected}", test.name));
                let actual = vars_to_json(&vars, &heap);
                assert!(
                    json_eq_ordered(&actual, &expected_json),
                    "\n[{}] vars mismatch\n  expected: {}\n  actual:   {}",
                    test.name, expected_json, actual
                );
            }
            Step::CheckBytecode(expected) => {
                assert_eq!(
                    last_bytecode.trim(), expected.trim(),
                    "\n[{}] bytecode mismatch\n  expected: {}\n  actual:   {}",
                    test.name, expected.trim(), last_bytecode.trim()
                );
            }
        }
    }
}

// ── JSON conversion ───────────────────────────────────────────────────

fn value_to_json(v: rex_core::heap::Value, heap: &rex_core::heap::Heap) -> serde_json::Value {
    if v.is_none() || v.is_null() { return serde_json::Value::Null; }
    if let Some(b) = v.as_bool() { return serde_json::Value::Bool(b); }
    if let Some(n) = v.as_i64() { return serde_json::json!(n); }
    if let Some(f) = v.as_f64(heap) { return serde_json::json!(f); }
    if let Some(s) = v.as_str(heap) { return serde_json::Value::String(s.to_string()); }
    if v.is_array() {
        return serde_json::Value::Array(
            heap.array_items(v).iter().map(|&item| value_to_json(item, heap)).collect()
        );
    }
    if v.is_object() {
        let map: serde_json::Map<String, serde_json::Value> = heap.object_pairs(v).iter()
            .map(|&(k, val)| (heap.resolve_str(k).to_string(), value_to_json(val, heap)))
            .collect();
        return serde_json::Value::Object(map);
    }
    serde_json::Value::Null
}

fn vars_to_json(vars: &HashMap<String, rex_core::heap::Value>, heap: &rex_core::heap::Heap) -> serde_json::Value {
    let map: serde_json::Map<String, serde_json::Value> = vars.iter()
        .map(|(k, &v)| (k.clone(), value_to_json(v, heap)))
        .collect();
    serde_json::Value::Object(map)
}

/// Order-sensitive deep comparison of JSON values.
fn json_eq_ordered(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    match (a, b) {
        (serde_json::Value::Object(am), serde_json::Value::Object(bm)) => {
            if am.len() != bm.len() { return false; }
            am.iter().zip(bm.iter()).all(|((ak, av), (bk, bv))| {
                ak == bk && json_eq_ordered(av, bv)
            })
        }
        (serde_json::Value::Array(aa), serde_json::Value::Array(ba)) => {
            aa.len() == ba.len() && aa.iter().zip(ba.iter()).all(|(a, b)| json_eq_ordered(a, b))
        }
        _ => a == b,
    }
}

// ── Test entry point ──────────────────────────────────────────────────

#[test]
fn run_spec() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let spec_path = std::path::Path::new(manifest_dir).join("../../docs/spec-by-example.md");
    let markdown = match std::fs::read_to_string(&spec_path) {
        Ok(content) => content,
        Err(_) => {
            eprintln!("spec-by-example.md not found, skipping");
            return;
        }
    };

    let tests = parse_spec(&markdown);
    if tests.is_empty() {
        eprintln!("no tests found in spec-by-example.md");
        return;
    }

    let mut passed = 0;
    let mut failed = 0;

    for test in &tests {
        let result = std::panic::catch_unwind(|| run_test(test));
        match result {
            Ok(_) => passed += 1,
            Err(e) => {
                failed += 1;
                let msg = e.downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| e.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown error");
                eprintln!("FAIL: {}\n  {}", test.name, msg);
            }
        }
    }

    eprintln!("\nspec: {passed} passed, {failed} failed out of {} tests", tests.len());
    assert_eq!(failed, 0, "{failed} spec tests failed");
}
