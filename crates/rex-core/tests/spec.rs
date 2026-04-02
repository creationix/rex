//! Golden spec test runner.
//!
//! Parses `docs/spec.md` and runs each test case defined in markdown.
//! See docs/spec.md for the format.

use std::collections::HashMap;

/// A single test case extracted from the spec.
#[derive(Debug)]
struct SpecTest {
    name: String,
    source: Option<String>,
    domain: Option<String>,
    inputs: Vec<(String, String)>, // (format, content) — "rex", "json", "rx", "rexc"
    expected_output: Option<String>,
    expected_bytecode: Option<String>,
    line: usize,
}

fn parse_spec(markdown: &str) -> Vec<SpecTest> {
    let mut tests = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut current = SpecTest {
        name: String::new(),
        source: None,
        domain: None,
        inputs: Vec::new(),
        expected_output: None,
        expected_bytecode: None,
        line: 0,
    };
    let mut in_fence = false;
    let mut fence_lang = String::new();
    let mut fence_body = String::new();

    for (lineno, line) in markdown.lines().enumerate() {
        if in_fence {
            if line.starts_with("```") {
                // Close fence
                let body = fence_body.trim().to_string();
                match fence_lang.as_str() {
                    "rex" => current.source = Some(body),
                    "rexd" => current.domain = Some(body),
                    "rexc" => current.expected_bytecode = Some(body),
                    s if s.ends_with(" output") => current.expected_output = Some(body),
                    s if s.ends_with(" input") => {
                        let fmt = s.strip_suffix(" input").unwrap().to_string();
                        current.inputs.push((fmt, body));
                    }
                    _ => {} // ignore unknown fence types
                }
                in_fence = false;
                fence_body.clear();
            } else {
                if !fence_body.is_empty() {
                    fence_body.push('\n');
                }
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
            // Flush previous test if it has content
            if current.source.is_some() || current.expected_output.is_some() {
                current.name = headers.join(" > ");
                tests.push(current);
                current = SpecTest {
                    name: String::new(),
                    source: None,
                    domain: None,
                    inputs: Vec::new(),
                    expected_output: None,
                    expected_bytecode: None,
                    line: 0,
                };
            }

            // Parse header level and text
            let level = line.chars().take_while(|&c| c == '#').count();
            let text = line[level..].trim().to_string();

            // Adjust header stack
            while headers.len() >= level {
                headers.pop();
            }
            headers.push(text);
            current.line = lineno + 1;
        }
    }

    // Flush last test
    if current.source.is_some() || current.expected_output.is_some() {
        current.name = headers.join(" > ");
        tests.push(current);
    }

    tests
}

fn run_test(test: &SpecTest) {
    let source = match &test.source {
        Some(s) => s.clone(),
        None => return, // no source to test
    };

    // Compile
    let bytecode = match &test.domain {
        Some(domain) => rex_core::compile_with_domain(&source, domain),
        None => rex_core::compile(&source),
    };

    // Check expected bytecode
    if let Some(expected) = &test.expected_bytecode {
        assert_eq!(
            bytecode.trim(), expected.trim(),
            "\n[{}] bytecode mismatch\n  source: {}\n  expected: {}\n  actual:   {}",
            test.name, source, expected, bytecode.trim()
        );
    }

    // Check expected output
    if let Some(expected) = &test.expected_output {
        let mut ctx = rex_core::interpret::Context::default();

        // Process inputs
        for (fmt, content) in &test.inputs {
            match fmt.as_str() {
                "rex" => {
                    // Compile and run input to set up variables
                    let input_bc = rex_core::compile(content);
                    let result = rex_core::interpret::run(&input_bc, ctx)
                        .unwrap_or_else(|e| panic!("[{}] input error: {e}", test.name));
                    ctx = rex_core::interpret::Context::default();
                    ctx.vars = result.vars;
                    ctx.heap = result.heap;
                }
                // TODO: json, rx, rexc input formats
                _ => {}
            }
        }

        ctx.gas_limit = 1_000_000;
        let result = rex_core::interpret::run(&bytecode, ctx)
            .unwrap_or_else(|e| panic!("[{}] runtime error: {e}\n  source: {}", test.name, source));

        let actual = format_value(result.value, &result.heap);
        assert_eq!(
            actual.trim(), expected.trim(),
            "\n[{}] output mismatch\n  source: {}\n  expected: {}\n  actual:   {}",
            test.name, source, expected.trim(), actual.trim()
        );
    }
}

fn format_value(v: rex_core::heap::Value, heap: &rex_core::heap::Heap) -> String {
    if v.is_none() { return "none".into(); }
    if v.is_null() { return "null".into(); }
    if let Some(b) = v.as_bool() { return if b { "true" } else { "false" }.into(); }
    if let Some(n) = v.as_i64() { return n.to_string(); }
    if let Some(f) = v.as_f64(heap) {
        if f == f.floor() && f.abs() < 1e15 { return format!("{}", f as i64); }
        return f.to_string();
    }
    if let Some(s) = v.as_str(heap) { return format!("{s:?}"); }
    if v.is_array() {
        let items: Vec<String> = heap.array_items(v).iter()
            .map(|&item| format_value(item, heap))
            .collect();
        return format!("[ {} ]", items.join(", "));
    }
    if v.is_object() {
        let pairs: Vec<String> = heap.object_pairs(v).iter()
            .map(|&(k, val)| format!("{}: {}", heap.resolve_str(k), format_value(val, heap)))
            .collect();
        if pairs.is_empty() { return "{}".into(); }
        return format!("{{ {} }}", pairs.join(" "));
    }
    format!("{v:?}")
}

#[test]
fn run_spec() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let spec_path = std::path::Path::new(manifest_dir).join("../../docs/spec-by-example.md");
    let markdown = match std::fs::read_to_string(&spec_path) {
        Ok(content) => content,
        Err(_) => {
            eprintln!("spec.md not found at {}, skipping spec tests", spec_path.display());
            return;
        }
    };

    let tests = parse_spec(&markdown);
    if tests.is_empty() {
        eprintln!("no tests found in spec.md");
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
