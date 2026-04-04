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
    Rex(String, Option<String>, usize),        // source, domain, line
    CompileOnly(String, Option<String>, usize), // source, domain, line
    CheckJson(String, usize, usize),            // expected, line, col
    CheckJsonVars(String, usize),
    CheckBytecode(String, usize),
}

#[derive(Debug)]
struct SpecTest {
    steps: Vec<Step>,
}

/// Extract the content from a backtick-wrapped table cell.
/// Handles single (`` `foo` ``) and double (``` `` foo `` ```) backtick wrapping.
fn extract_backtick(cell: &str) -> Option<&str> {
    let cell = cell.trim();
    // Try double-backtick first: `` value ``
    if let Some(s) = cell.strip_prefix("`` ").and_then(|s| s.strip_suffix(" ``")) {
        return Some(s);
    }
    if let Some(s) = cell.strip_prefix("``").and_then(|s| s.strip_suffix("``")) {
        return Some(s.trim());
    }
    // Single backtick: `value`
    cell.strip_prefix('`').and_then(|s| s.strip_suffix('`'))
}

/// Parse a markdown table row into cells (splits on `|`, drops leading/trailing empties from outer pipes).
fn parse_table_row(line: &str) -> Vec<String> {
    let cells: Vec<String> = line.split('|')
        .map(|s| s.trim().to_string())
        .collect();
    // Drop first and last elements (empty strings from leading/trailing `|`)
    let start = if cells.first().map_or(false, |s| s.is_empty()) { 1 } else { 0 };
    let end = if cells.last().map_or(false, |s| s.is_empty()) { cells.len() - 1 } else { cells.len() };
    cells[start..end].to_vec()
}

fn parse_spec(markdown: &str) -> Vec<SpecTest> {
    let mut tests = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut steps: Vec<Step> = Vec::new();

    let mut in_fence = false;
    let mut fence_lang = String::new();
    let mut fence_body = String::new();
    let mut fence_start: usize = 0;
    let mut active_domain: Option<String> = None;

    // Table state
    let mut table_cols: Vec<String> = Vec::new(); // column names from header row
    let mut table_state = 0u8; // 0=none, 1=saw header, 2=saw separator (active)

    for (lineno, line) in markdown.lines().enumerate() {
        let line_num = lineno + 1; // 1-based

        if in_fence {
            if line.starts_with("```") {
                let body = fence_body.trim().to_string();
                match fence_lang.as_str() {
                    "rex" => steps.push(Step::Rex(body, active_domain.clone(), fence_start)),
                    "json" => steps.push(Step::CheckJson(body, fence_start, 1)),
                    "json vars" => steps.push(Step::CheckJsonVars(body, fence_start)),
                    "rexc" | "rext" => steps.push(Step::CheckBytecode(body, fence_start)),
                    "rexd" => { active_domain = Some(body); }
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
            table_state = 0;
            in_fence = true;
            fence_lang = rest.trim().to_string();
            fence_body.clear();
            fence_start = line_num;
            continue;
        }

        // Table parsing
        if line.contains('|') {
            let cells = parse_table_row(line);
            match table_state {
                0 => {
                    // Potential header row — check if cells look like column names
                    let names: Vec<String> = cells.iter().map(|c| c.trim().to_lowercase()).collect();
                    if names.iter().any(|n| n == "rex" || n == "rext" || n == "json" || n == "rexd") {
                        table_cols = names;
                        table_state = 1;
                    }
                }
                1 => {
                    // Expect separator row (|---|---|)
                    if cells.iter().all(|c| c.trim().chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')) {
                        table_state = 2;
                    } else {
                        table_state = 0;
                    }
                }
                2 => {
                    // Data row — extract backtick values by column name
                    if cells.len() < table_cols.len() {
                        table_state = 0;
                        continue;
                    }
                    // Find a column's backtick value and its 1-based column in the line
                    let get_with_col = |name: &str| -> Option<(String, usize)> {
                        let idx = table_cols.iter().position(|n| n == name)?;
                        let val = extract_backtick(cells.get(idx)?)?.to_string();
                        // Find column offset: count through pipe-separated cells
                        let col = line.match_indices('|').nth(idx)
                            .map(|(pos, _)| pos + 4) // skip `| ` ` `
                            .unwrap_or(1);
                        Some((val, col))
                    };
                    let get = |name: &str| -> Option<String> {
                        get_with_col(name).map(|(v, _)| v)
                    };

                    if let Some(rex_src) = get("rex") {
                        let has_json = table_cols.iter().any(|n| n == "json");
                        let json_with_col = get_with_col("json");
                        let domain = get("rexd");

                        if has_json && json_with_col.is_some() {
                            steps.push(Step::Rex(rex_src, domain, line_num));
                        } else {
                            steps.push(Step::CompileOnly(rex_src, domain, line_num));
                        }

                        if let Some(rext) = get("rext") {
                            steps.push(Step::CheckBytecode(rext, line_num));
                        }
                        if let Some((json_str, col)) = json_with_col {
                            let json_str = match json_str.as_str() {
                                "none" => "null".to_string(),
                                other => other.to_string(),
                            };
                            steps.push(Step::CheckJson(json_str, line_num, col));
                        }
                    }
                }
                _ => {}
            }
            continue;
        } else {
            table_state = 0;
        }

        if line.starts_with('#') {
            // Flush previous test
            if !steps.is_empty() {
                tests.push(SpecTest {
                    steps: std::mem::take(&mut steps),
                });
            }

            let level = line.chars().take_while(|&c| c == '#').count();
            let text = line[level..].trim().to_string();
            while headers.len() >= level { headers.pop(); }
            headers.push(text);
        }
    }

    // Flush last test
    if !steps.is_empty() {
        tests.push(SpecTest {
            steps,
        });
    }

    tests
}

// ── Host environment for spec tests ──────────────────────────────────

fn spec_opcodes() -> HashMap<String, fn(&[rex_core::heap::Value], &mut rex_core::heap::Heap) -> Result<rex_core::heap::Value, rex_core::interpret::RexError>> {
    use rex_core::heap::{Value, Heap};
    use rex_core::interpret::RexError;

    let mut map: HashMap<String, fn(&[Value], &mut Heap) -> Result<Value, RexError>> = HashMap::new();

    // H — html tagged template: escapes interpolated values for safe HTML
    map.insert("H".into(), |args: &[Value], heap: &mut Heap| {
        // args[0] = string parts array, args[1..] = interpolated values
        if args.is_empty() { return Ok(Value::NONE); }
        let parts_val = args[0];
        let parts: Vec<String> = if parts_val.is_array() {
            heap.array_items(parts_val).iter()
                .map(|v| v.as_str(heap).unwrap_or("").to_string())
                .collect()
        } else { return Ok(Value::NONE); };

        let values = &args[1..];
        let mut out = String::new();
        for (i, part) in parts.iter().enumerate() {
            out.push_str(part);
            if i < values.len() {
                // HTML-escape the interpolated value
                let s = values[i].as_str(heap)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format_value(values[i], heap));
                for c in s.chars() {
                    match c {
                        '&' => out.push_str("&amp;"),
                        '<' => out.push_str("&lt;"),
                        '>' => out.push_str("&gt;"),
                        '"' => out.push_str("&quot;"),
                        '\'' => out.push_str("&#39;"),
                        _ => out.push(c),
                    }
                }
            }
        }
        Ok(heap.intern_value(&out))
    });

    // Jp — json.parse: parse a JSON string into a Rex value
    map.insert("Jp".into(), |args: &[Value], heap: &mut Heap| {
        let text = args.first().and_then(|v| v.as_str(heap)).unwrap_or("");
        match serde_json::from_str::<serde_json::Value>(text) {
            Ok(json) => Ok(json_to_value(&json, heap)),
            Err(_) => Ok(Value::NONE),
        }
    });

    // Js — json.stringify: convert a Rex value to a JSON string
    map.insert("Js".into(), |args: &[Value], heap: &mut Heap| {
        let val = args.first().copied().unwrap_or(Value::NONE);
        let json = value_to_json(val, heap);
        let s = json.to_string();
        Ok(heap.intern_value(&s))
    });

    // Mf — math.floor: floor a number to an integer
    map.insert("Mf".into(), |args: &[Value], heap: &mut Heap| {
        let n = args.first().and_then(|v| v.as_f64(heap)).unwrap_or(0.0);
        Ok(Value::int(n.floor() as i64))
    });

    map
}

fn spec_refs(heap: &mut rex_core::heap::Heap) -> HashMap<String, rex_core::heap::Value> {
    let mut refs = HashMap::new();

    // E — env: a simple key-value map
    let name = heap.intern_value("Rex");
    let version = heap.intern_value("1.0");
    let k_name = heap.intern("name");
    let k_version = heap.intern("version");
    let env = heap.alloc_object(vec![(k_name, name), (k_version, version)]);
    refs.insert("E".into(), env);

    refs
}

/// Convert a serde_json::Value to a Rex heap value.
fn json_to_value(json: &serde_json::Value, heap: &mut rex_core::heap::Heap) -> rex_core::heap::Value {
    use rex_core::heap::Value;
    match json {
        serde_json::Value::Null => Value::NULL,
        serde_json::Value::Bool(b) => if *b { Value::TRUE } else { Value::FALSE },
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Value::int(i) }
            else if let Some(f) = n.as_f64() { heap.alloc_float(f) }
            else { Value::NONE }
        }
        serde_json::Value::String(s) => heap.intern_value(s),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value> = arr.iter().map(|v| json_to_value(v, heap)).collect();
            heap.alloc_array(items)
        }
        serde_json::Value::Object(obj) => {
            let pairs: Vec<(u32, Value)> = obj.iter()
                .map(|(k, v)| (heap.intern(k), json_to_value(v, heap)))
                .collect();
            heap.alloc_object(pairs)
        }
    }
}

/// Format a value as a plain string (for html escaping of non-string values).
fn format_value(v: rex_core::heap::Value, heap: &rex_core::heap::Heap) -> String {
    if v.is_none() { return "none".into(); }
    if v.is_null() { return "null".into(); }
    if let Some(b) = v.as_bool() { return b.to_string(); }
    if let Some(n) = v.as_i64() { return n.to_string(); }
    if let Some(f) = v.as_f64(heap) { return f.to_string(); }
    value_to_json(v, heap).to_string()
}

fn check_format(source: &str, line: usize) -> Option<String> {
    let formatted = rex_core::format(source);
    let formatted = formatted.trim();
    let source = source.trim();
    if formatted != source {
        Some(format!("line {line}: format mismatch\n  source:    {source}\n  formatted: {formatted}"))
    } else {
        None
    }
}

fn run_test(test: &SpecTest) -> Vec<String> {
    let mut vars: HashMap<String, rex_core::heap::Value> = HashMap::new();
    let mut heap = rex_core::heap::Heap::new();
    let mut last_value = rex_core::heap::Value::NONE;
    let mut last_bytecode = String::new();
    let mut errors = Vec::new();

    for step in &test.steps {
        match step {
            Step::Rex(source, domain, line) => {
                if let Some(err) = check_format(source, *line) {
                    errors.push(err);
                }
                let bytecode = match domain {
                    Some(d) => rex_core::compile_with_domain(source, d),
                    None => rex_core::compile(source),
                };
                last_bytecode = bytecode.clone();

                let mut ctx = rex_core::interpret::Context::default();
                ctx.vars = std::mem::take(&mut vars);
                ctx.heap = std::mem::take(&mut heap);
                ctx.gas_limit = 1_000_000;
                if domain.is_some() {
                    ctx.opcodes = spec_opcodes();
                    ctx.refs = spec_refs(&mut ctx.heap);
                }

                match rex_core::interpret::run(&bytecode, ctx) {
                    Ok(result) => {
                        last_value = result.value;
                        vars = result.vars;
                        heap = result.heap;
                    }
                    Err(e) => {
                        errors.push(format!("line {line}: runtime error: {e}"));
                    }
                }
            }
            Step::CompileOnly(source, domain, _line) => {
                if let Some(err) = check_format(source, *_line) {
                    errors.push(err);
                }
                last_bytecode = match domain {
                    Some(d) => rex_core::compile_with_domain(source, d),
                    None => rex_core::compile(source),
                };
            }
            Step::CheckJson(expected, line, col) => {
                let actual = value_to_json(last_value, &heap);
                match serde_json::from_str::<serde_json::Value>(expected.trim()) {
                    Err(e) => {
                        let ecol = col + e.column() - 1;
                        errors.push(format!("line {line}:{ecol}: invalid JSON `{expected}`"));
                    }
                    Ok(expected_json) => {
                        if !json_eq_ordered(&actual, &expected_json) {
                            errors.push(format!("line {line}:{col}: expected {expected_json}, got {actual}"));
                        }
                    }
                }
            }
            Step::CheckJsonVars(expected, line) => {
                match serde_json::from_str::<serde_json::Value>(expected.trim()) {
                    Err(e) => errors.push(format!("line {line}: invalid JSON: {e}")),
                    Ok(expected_json) => {
                        let actual = vars_to_json(&vars, &heap);
                        if !json_eq_ordered(&actual, &expected_json) {
                            errors.push(format!("line {line}: vars expected {expected_json}, got {actual}"));
                        }
                    }
                }
            }
            Step::CheckBytecode(expected, line) => {
                let (expected, actual) = (expected.trim(), last_bytecode.trim());
                if actual != expected {
                    errors.push(format!("line {line}: expected {expected}, got {actual}"));
                }
            }
        }
    }
    errors
}

// ── JSON conversion ───────────────────────────────────────────────────

fn value_to_json(v: rex_core::heap::Value, heap: &rex_core::heap::Heap) -> serde_json::Value {
    if v.is_none() || v.is_null() { return serde_json::Value::Null; }
    if let Some(b) = v.as_bool() { return serde_json::Value::Bool(b); }
    if let Some(n) = v.as_i64() { return serde_json::json!(n); }
    // Preserve sig*10^exp representation for decimals
    if let Some(fid) = v.float_id() {
        match &heap.floats[fid as usize] {
            rex_core::heap::FloatValue::Decimal { sig, exp } => {
                let s = format!("{sig}e{exp}");
                return serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!(0));
            }
            rex_core::heap::FloatValue::Float(f) => return serde_json::json!(f),
        }
    }
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
        // Compare numbers by f64 value so 12e3 == 12000.0 == 1.2e4
        (serde_json::Value::Number(an), serde_json::Value::Number(bn)) => {
            an.as_f64() == bn.as_f64()
        }
        _ => a == b,
    }
}

// ── Test entry point ──────────────────────────────────────────────────

const SPEC_FILE: &str = "docs/spec-by-example.md";

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let spec_path = std::path::Path::new(manifest_dir).join("../../").join(SPEC_FILE);
    let markdown = std::fs::read_to_string(&spec_path)
        .unwrap_or_else(|_| { eprintln!("{SPEC_FILE} not found"); std::process::exit(1); });

    let tests = parse_spec(&markdown);

    let mut errors: Vec<String> = Vec::new();
    for test in &tests {
        errors.extend(run_test(test));
    }

    if errors.is_empty() {
        eprintln!("\x1b[32m✓ {} specs passed\x1b[0m", tests.len());
    } else {
        eprintln!(); // blank line after [Running: ...] header
        for err in errors.iter().take(30) {
            if let Some(rest) = err.strip_prefix("line ") {
                if let Some((loc, detail)) = rest.split_once(": ") {
                    eprintln!("\x1b[38;5;208m{SPEC_FILE}:{loc}\x1b[0m {detail}");
                    continue;
                }
            }
            eprintln!("{err}");
        }
        if errors.len() > 4 {
            eprintln!("  ... and {} more", errors.len() - 4);
        }
        eprintln!("\n\x1b[31m✗ {} errors\x1b[0m", errors.len());
        std::process::exit(1);
    }
}
