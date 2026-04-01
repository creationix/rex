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
                .insert(key.clone(), json_to_rex_value(val));
        }
    }

    match rex_core::interpret::run(&bytecode, ctx) {
        Ok(result) => {
            let text = format_rex_value(&result.value);
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

fn json_to_rex_value(val: &Value) -> rex_core::interpret::RexValue {
    use rex_core::interpret::RexValue;
    match val {
        Value::Null => RexValue::Null,
        Value::Bool(b) => RexValue::Bool(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                RexValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                RexValue::Float(f)
            } else {
                RexValue::Null
            }
        }
        Value::String(s) => RexValue::Str(s.clone()),
        Value::Array(arr) => RexValue::Array(arr.iter().map(json_to_rex_value).collect()),
        Value::Object(obj) => RexValue::Object(
            obj.iter()
                .map(|(k, v)| (k.clone(), json_to_rex_value(v)))
                .collect(),
        ),
    }
}

fn format_rex_value(val: &rex_core::interpret::RexValue) -> String {
    use rex_core::interpret::RexValue;
    match val {
        RexValue::RexNone => "none".to_string(),
        RexValue::Null => "null".to_string(),
        RexValue::Bool(b) => b.to_string(),
        RexValue::Int(n) => n.to_string(),
        RexValue::Float(n) => n.to_string(),
        RexValue::Decimal { sig, exp } => format!("{sig}e{exp}"),
        RexValue::Str(s) => format!("{s:?}"),
        RexValue::Array(items) => {
            let parts: Vec<String> = items.iter().map(format_rex_value).collect();
            format!("[{}]", parts.join(", "))
        }
        RexValue::Object(pairs) => {
            let parts: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{k:?}: {}", format_rex_value(v)))
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        RexValue::Host(idx) => format!("<host:{idx}>"),
    }
}
