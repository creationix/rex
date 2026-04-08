//! Golden spec test runner.
//!
//! Parses `docs/spec-by-example.md` and runs each test case.
//!
//! ## Block types
//!
//! - `rex` — input: compile and run in the shared VM, preserving state
//! - `json` — output check: structural match against the last expression result
//!   (order-sensitive for arrays and objects)
//! - `json vars` — output check: structural match against all current variables
//! - `json types` — output check: structural match against inferred type spans
//! - `csv types` — output check: exact CSV snapshot of inferred type spans
//! - `rext` — output check: exact match against compiled bytecode of previous rex block
//!
//! Multiple blocks per test, interleaved freely. State carries across rex blocks.

use std::collections::HashMap;

#[derive(Debug)]
enum Step {
    Rex(String, Option<String>, usize),        // source, domain, line
    CompileOnly(String, Option<String>, usize), // source, domain, line
    CheckJson(String, usize, usize),            // expected, line, col
    CheckJsonVars(String, usize),
    CheckJsonTypes(String, usize),
    CheckCsvTypes(String, usize),
    CheckBytecode(String, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypeSnapshotRow {
    text: String,
    ty: String,
    line: Option<usize>,
    col: Option<usize>,
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

fn parse_spec(markdown: &str) -> (Vec<SpecTest>, Vec<String>) {
    let mut tests = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut steps: Vec<Step> = Vec::new();

    let mut in_fence = false;
    let mut fence_lang = String::new();
    let mut fence_body = String::new();
    let mut fence_start: usize = 0;
    let mut active_domain: Option<String> = None;
    let mut format_errors: Vec<String> = Vec::new();

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
                    "json types" => steps.push(Step::CheckJsonTypes(body, fence_start)),
                    "csv types" => steps.push(Step::CheckCsvTypes(body, fence_start)),
                    "rext" => steps.push(Step::CheckBytecode(body, fence_start)),
                    "rexc" => {
                        format_errors.push(format!(
                            "line {fence_start}: unsupported fence language `rexc`; use `rext`"
                        ));
                    }
                    "rexd" => {
                        if let Some(err) = check_format(&body, fence_start) {
                            // Collect format errors for rexd blocks but don't add as steps
                            format_errors.push(err);
                        }
                        active_domain = Some(body);
                    }
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
                    if names.iter().any(|n| n == "rex" || n == "rext" || n == "json" || n == "json types" || n == "rexd") {
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
                        let has_json_types = table_cols.iter().any(|n| n == "json types");
                        let json_with_col = get_with_col("json");
                        let json_types = get("json types");
                        let domain = get("rexd");

                        if (has_json && json_with_col.is_some()) || (has_json_types && json_types.is_some()) {
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
                        if let Some(types_json) = json_types {
                            steps.push(Step::CheckJsonTypes(types_json, line_num));
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

    (tests, format_errors)
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
        let indent = |s: &str| -> String {
            s.lines()
                .map(|l| format!("  {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Some(format!(
            "line {line}: format mismatch\n\x1b[31m  source:\x1b[0m\n{}\n\x1b[32m  formatted:\x1b[0m\n{}",
            indent(source),
            indent(formatted),
        ))
    } else {
        None
    }
}

fn run_test(test: &SpecTest) -> Vec<String> {
    let mut vars: HashMap<String, rex_core::heap::Value> = HashMap::new();
    let mut heap = rex_core::heap::Heap::new();
    let mut last_value = rex_core::heap::Value::NONE;
    let mut last_bytecode = String::new();
    let mut last_types_rows: Vec<TypeSnapshotRow> = Vec::new();
    let mut last_types_json = serde_json::Value::Array(Vec::new());
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
                last_types_rows = inferred_type_rows(source, domain.as_deref());
                last_types_json = type_rows_to_json(&last_types_rows);

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
                last_types_rows = inferred_type_rows(source, domain.as_deref());
                last_types_json = type_rows_to_json(&last_types_rows);
            }
            Step::CheckJson(expected, line, col) => {
                let actual = value_to_json(last_value, &heap);
                match serde_json::from_str::<serde_json::Value>(expected.trim()) {
                    Err(e) => {
                        let ecol = col + e.column() - 1;
                        errors.push(format!(
                            "line {line}:{ecol}: invalid JSON: {e}\nexpected:\n{expected}\nactual:\n{actual}"
                        ));
                    }
                    Ok(expected_json) => {
                        if !json_eq_ordered(&actual, &expected_json) {
                            errors.push(format!("line {line}:{col}: expected {expected_json}, got {actual}"));
                        }
                    }
                }
            }
            Step::CheckJsonVars(expected, line) => {
                let actual = vars_to_json(&vars, &heap);
                match serde_json::from_str::<serde_json::Value>(expected.trim()) {
                    Err(e) => errors.push(format!(
                        "line {line}: invalid JSON: {e}\nexpected:\n{expected}\nactual:\n{actual}"
                    )),
                    Ok(expected_json) => {
                        if !json_eq_ordered(&actual, &expected_json) {
                            errors.push(format!("line {line}: vars expected {expected_json}, got {actual}"));
                        }
                    }
                }
            }
            Step::CheckJsonTypes(expected, line) => {
                match serde_json::from_str::<serde_json::Value>(expected.trim()) {
                    Err(e) => errors.push(format!(
                        "line {line}: invalid JSON: {e}\nexpected:\n{expected}\nactual:\n{last_types_json}"
                    )),
                    Ok(expected_json) => {
                        if !json_types_match(&last_types_json, &expected_json) {
                            errors.push(format!(
                                "line {line}: types expected {expected_json}, got {last_types_json}"
                            ));
                        }
                    }
                }
            }
            Step::CheckCsvTypes(expected, line) => {
                match parse_csv_types(expected) {
                    Err(e) => errors.push(format!(
                        "line {line}: invalid csv types: {e}\nexpected:\n{expected}\nactual:\n{}",
                        format_csv_types(&last_types_rows, true)
                    )),
                    Ok(expected_rows) => {
                        if expected_rows != last_types_rows {
                            errors.push(format!(
                                "line {line}: csv types mismatch\nexpected:\n{}\ngot:\n{}",
                                format_csv_types(&expected_rows, true),
                                format_csv_types(&last_types_rows, true),
                            ));
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
    if v.is_none() { return serde_json::Value::Null; } // should not normally be called for none
    if v.is_null() { return serde_json::Value::Null; }
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
            rex_core::heap::FloatValue::Blob(id) => return serde_json::json!(format!("<blob {} bytes>", heap.blobs[*id].len())),
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
    // `json vars` snapshots are about variable state, not HashMap iteration order.
    // Keep output stable by sorting variable names before building the JSON object.
    let mut keys: Vec<&String> = vars.keys().collect();
    keys.sort();

    let mut map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for key in keys {
        if let Some(&value) = vars.get(key) {
            if value.is_none() { continue; } // none = absence, not a value
            map.insert(key.clone(), value_to_json(value, heap));
        }
    }
    serde_json::Value::Object(map)
}

fn inferred_type_rows(source: &str, domain: Option<&str>) -> Vec<TypeSnapshotRow> {
    let schema = match domain {
        Some(rexd) => rex_core::typecheck::parse_rexd(rexd),
        None => rex_core::typecheck::DomainSchema::default(),
    };
    let (_diags, span_types, _fns, _aliases) = rex_core::typecheck::check_source_with_types(source, &schema);

    let mut rows: Vec<(usize, usize, usize, usize, String, String)> = span_types
        .into_iter()
        .map(|(span, ty)| {
            let text = source.get(span.clone()).unwrap_or("").to_string();
            let (line, col) = offset_to_line_col_1(source, span.start);
            (span.start, span.end, line, col, text, ty.simplify().display())
        })
        .collect();

    rows.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.cmp(&b.3))
            .then_with(|| a.4.cmp(&b.4))
            .then_with(|| a.5.cmp(&b.5))
    });
    rows.dedup();

    rows
        .into_iter()
        .map(|(_start, _end, line, col, text, ty)| {
            TypeSnapshotRow {
                text,
                ty,
                line: Some(line),
                col: Some(col),
            }
        })
        .collect()
}

fn type_rows_to_json(rows: &[TypeSnapshotRow]) -> serde_json::Value {
    serde_json::Value::Array(rows.iter().map(|row| {
        let mut obj = serde_json::Map::new();
        obj.insert("text".to_string(), serde_json::json!(row.text));
        obj.insert("type".to_string(), serde_json::json!(row.ty));
        if let Some(line) = row.line {
            obj.insert("line".to_string(), serde_json::json!(line));
        }
        if let Some(col) = row.col {
            obj.insert("col".to_string(), serde_json::json!(col));
        }
        serde_json::Value::Object(obj)
    }).collect())
}

fn offset_to_line_col_1(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Matching for `json types` blocks.
///
/// Supported forms:
/// - exact snapshot: JSON array of span entries (order-sensitive)
/// - subset assertions: `{ "contains": [ {"text":"a", "type":"int"}, ... ] }`
///   Each `contains` item must match at least one actual span entry.
fn json_types_match(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    json_eq_ordered(actual, expected)
}

fn parse_csv_types(csv: &str) -> Result<Vec<TypeSnapshotRow>, String> {
    let mut lines = csv.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next().ok_or_else(|| "missing header".to_string())?;
    let cols = parse_csv_line(header)?;
    if cols.len() < 2 {
        return Err("header must include at least text,type".to_string());
    }

    let col_index = |name: &str| cols.iter().position(|c| c.trim() == name);
    let i_text = col_index("text").ok_or_else(|| "missing 'text' column".to_string())?;
    let i_type = col_index("type").ok_or_else(|| "missing 'type' column".to_string())?;
    let i_line = col_index("line");
    let i_col = col_index("col");

    let mut out = Vec::new();
    for (idx, line) in lines.enumerate() {
        let fields = parse_csv_line(line)?;
        let get = |i: usize| fields.get(i).map(|s| s.as_str()).unwrap_or("");

        let line_num = if let Some(i) = i_line {
            let v = get(i).trim();
            if v.is_empty() { None } else { Some(v.parse::<usize>().map_err(|_| format!("row {} invalid line", idx + 2))?) }
        } else { None };
        let col_num = if let Some(i) = i_col {
            let v = get(i).trim();
            if v.is_empty() { None } else { Some(v.parse::<usize>().map_err(|_| format!("row {} invalid col", idx + 2))?) }
        } else { None };

        out.push(TypeSnapshotRow {
            text: get(i_text).to_string(),
            ty: get(i_type).to_string(),
            line: line_num,
            col: col_num,
        });
    }
    Ok(out)
}

fn parse_csv_line(line: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        cur.push('"');
                    } else {
                        in_quotes = false;
                    }
                } else {
                    in_quotes = true;
                }
            }
            ',' if !in_quotes => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if in_quotes {
        return Err("unterminated quote".to_string());
    }
    out.push(cur.trim().to_string());
    Ok(out)
}

fn format_csv_types(rows: &[TypeSnapshotRow], aligned: bool) -> String {
    let mut table: Vec<[String; 4]> = Vec::new();
    table.push(["text".into(), "type".into(), "line".into(), "col".into()]);
    for r in rows {
        table.push([
            r.text.clone(),
            r.ty.clone(),
            r.line.map(|v| v.to_string()).unwrap_or_default(),
            r.col.map(|v| v.to_string()).unwrap_or_default(),
        ]);
    }

    if !aligned {
        return table.into_iter()
            .map(|r| r.into_iter().map(csv_escape).collect::<Vec<_>>().join(","))
            .collect::<Vec<_>>()
            .join("\n");
    }

    let mut widths = [0usize; 4];
    for row in &table {
        for i in 0..4 {
            widths[i] = widths[i].max(row[i].len());
        }
    }
    table.into_iter().map(|row| {
        (0..4).map(|i| {
            let cell = csv_escape(row[i].clone());
            format!("{cell:width$}", width = widths[i])
        }).collect::<Vec<_>>().join(", ")
    }).collect::<Vec<_>>().join("\n")
}

fn csv_escape(s: String) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s
    }
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

    let (tests, format_errors) = parse_spec(&markdown);

    let mut errors: Vec<String> = format_errors;
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
