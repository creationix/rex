use rex_core::typecheck::{self, DiagnosticKind, DomainSchema};
use serde_json::{json, Value};

pub fn list_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "rex_check",
            "description": "Type-check Rex source code and return diagnostics (errors and warnings).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Rex source code to check"
                    },
                    "domain": {
                        "type": "string",
                        "description": "Optional .rexd domain interface source"
                    }
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "rex_compile",
            "description": "Compile Rex source code to REXC bytecode.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Rex source code to compile"
                    },
                    "domain": {
                        "type": "string",
                        "description": "Optional .rexd domain interface source for shortcode rewriting"
                    }
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "rex_parse",
            "description": "Parse Rex source code and return parse errors.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Rex source code to parse"
                    }
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "rex_eval",
            "description": "Evaluate a Rex expression with optional variable bindings and return the result.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {
                        "type": "string",
                        "description": "Rex source code to evaluate"
                    },
                    "input": {
                        "type": "object",
                        "description": "Variable bindings as key-value pairs"
                    },
                    "domain": {
                        "type": "string",
                        "description": "Optional .rexd domain interface source"
                    }
                },
                "required": ["source"]
            }
        }),
    ]
}

pub fn call_tool(name: &str, args: &Value, default_schema: &DomainSchema) -> Value {
    match name {
        "rex_check" => tool_check(args, default_schema),
        "rex_compile" => tool_compile(args),
        "rex_parse" => tool_parse(args),
        "rex_eval" => tool_eval(args),
        _ => json!({
            "content": [{
                "type": "text",
                "text": format!("Unknown tool: {name}")
            }],
            "isError": true
        }),
    }
}

fn tool_check(args: &Value, default_schema: &DomainSchema) -> Value {
    let source = args.get("source").and_then(|s| s.as_str()).unwrap_or("");

    let schema = match args.get("domain").and_then(|d| d.as_str()) {
        Some(rexd_src) => typecheck::parse_rexd(rexd_src),
        None => default_schema.clone(),
    };

    // Parse errors
    let tokens = rex_core::lexer::lex(source);
    let (_, parse_errors) = rex_core::parser::parse(source, &tokens);

    let mut diagnostics = Vec::new();
    for e in &parse_errors {
        diagnostics.push(json!({
            "message": e.message,
            "start": e.span.start,
            "end": e.span.end,
            "severity": "error"
        }));
    }

    // Type-check errors
    let type_diags = typecheck::check_source(source, &schema);
    for d in &type_diags {
        diagnostics.push(json!({
            "message": d.message,
            "start": d.span.start,
            "end": d.span.end,
            "severity": match d.kind {
                DiagnosticKind::Error => "error",
                DiagnosticKind::Warning => "warning",
            }
        }));
    }

    let text = if diagnostics.is_empty() {
        "No errors found.".to_string()
    } else {
        serde_json::to_string_pretty(&diagnostics).unwrap_or_default()
    };

    json!({
        "content": [{ "type": "text", "text": text }]
    })
}

fn tool_compile(args: &Value) -> Value {
    let source = args.get("source").and_then(|s| s.as_str()).unwrap_or("");

    let bytecode = match args.get("domain").and_then(|d| d.as_str()) {
        Some(domain_src) => rex_core::compile_with_domain(source, domain_src),
        None => rex_core::compile(source),
    };

    json!({
        "content": [{ "type": "text", "text": bytecode }]
    })
}

fn tool_parse(args: &Value) -> Value {
    let source = args.get("source").and_then(|s| s.as_str()).unwrap_or("");

    let tokens = rex_core::lexer::lex(source);
    let (_, errors) = rex_core::parser::parse(source, &tokens);

    let error_list: Vec<Value> = errors
        .iter()
        .map(|e| {
            json!({
                "message": e.message,
                "start": e.span.start,
                "end": e.span.end
            })
        })
        .collect();

    let text = if error_list.is_empty() {
        "Parse successful. No errors.".to_string()
    } else {
        format!(
            "{} parse error(s):\n{}",
            error_list.len(),
            serde_json::to_string_pretty(&error_list).unwrap_or_default()
        )
    };

    json!({
        "content": [{ "type": "text", "text": text }]
    })
}

fn tool_eval(args: &Value) -> Value {
    let source = args.get("source").and_then(|s| s.as_str()).unwrap_or("");

    let bytecode = match args.get("domain").and_then(|d| d.as_str()) {
        Some(domain_src) => rex_core::compile_with_domain(source, domain_src),
        None => rex_core::compile(source),
    };

    let mut ctx = rex_core::interpret::Context::default();
    ctx.gas_limit = 10_000_000;

    // Load input bindings
    if let Some(input) = args.get("input").and_then(|i| i.as_object()) {
        for (key, val) in input {
            ctx.vars
                .insert(key.clone(), json_to_heap_value(val, &mut ctx.heap));
        }
    }

    match rex_core::interpret::run(&bytecode, ctx) {
        Ok(result) => {
            let text = format_value(result.value, &result.heap);
            json!({
                "content": [{ "type": "text", "text": text }]
            })
        }
        Err(e) => {
            json!({
                "content": [{ "type": "text", "text": format!("Runtime error: {e}") }],
                "isError": true
            })
        }
    }
}

fn json_to_heap_value(val: &Value, heap: &mut rex_core::heap::Heap) -> rex_core::heap::Value {
    use rex_core::heap::Value as HVal;
    match val {
        Value::Null => HVal::NULL,
        Value::Bool(b) => HVal::bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                HVal::int(i)
            } else if let Some(f) = n.as_f64() {
                heap.alloc_float(f)
            } else {
                HVal::NULL
            }
        }
        Value::String(s) => heap.intern_value(s),
        Value::Array(arr) => {
            let items: Vec<HVal> = arr.iter().map(|v| json_to_heap_value(v, heap)).collect();
            heap.alloc_array(items)
        }
        Value::Object(obj) => {
            let pairs: Vec<(u32, HVal)> = obj.iter()
                .map(|(k, v)| (heap.intern(k), json_to_heap_value(v, heap)))
                .collect();
            heap.alloc_object(pairs)
        }
    }
}

fn format_value(val: rex_core::heap::Value, heap: &rex_core::heap::Heap) -> String {
    if val.is_none() { return "none".to_string(); }
    if val.is_null() { return "null".to_string(); }
    if let Some(b) = val.as_bool() { return b.to_string(); }
    if let Some(n) = val.as_i64() { return n.to_string(); }
    if let Some(f) = val.as_f64(heap) { return f.to_string(); }
    if let Some(s) = val.as_str(heap) { return format!("{s:?}"); }
    if val.is_array() {
        let parts: Vec<String> = heap.array_items(val).iter()
            .map(|&item| format_value(item, heap))
            .collect();
        return format!("[{}]", parts.join(", "));
    }
    if val.is_object() {
        let parts: Vec<String> = heap.object_pairs(val).iter()
            .map(|&(k, v)| format!("{:?}: {}", heap.resolve_str(k), format_value(v, heap)))
            .collect();
        return format!("{{{}}}", parts.join(", "));
    }
    if let Some(idx) = val.host_id() { return format!("<host:{idx}>"); }
    format!("{val:?}")
}
