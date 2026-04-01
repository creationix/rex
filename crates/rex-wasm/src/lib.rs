use wasm_bindgen::prelude::*;

/// Parse + typecheck a Rex source string and return diagnostics as a JS array.
/// Each element: `{ message: string, start: number, end: number, severity: "error" | "warning" }`.
/// Pass an empty string for `rexd_source` to skip type checking.
#[wasm_bindgen]
pub fn check(source: &str, rexd_source: &str) -> Result<JsValue, JsValue> {
    let tokens = rex_core::lexer::lex(source);
    let (_, parse_errors) = rex_core::parser::parse(source, &tokens);

    let mut diagnostics: Vec<DiagnosticJs> = parse_errors
        .iter()
        .map(|e| DiagnosticJs {
            message: e.message.clone(),
            start: e.span.start,
            end: e.span.end,
            severity: "error".to_string(),
        })
        .collect();

    // Run type checker if domain source is provided
    if !rexd_source.is_empty() {
        let schema = rex_core::typecheck::parse_rexd(rexd_source);
        let type_diags = rex_core::typecheck::check_source(source, &schema);
        for d in type_diags {
            diagnostics.push(DiagnosticJs {
                message: d.message,
                start: d.span.start,
                end: d.span.end,
                severity: match d.kind {
                    rex_core::typecheck::DiagnosticKind::Error => "error".to_string(),
                    rex_core::typecheck::DiagnosticKind::Warning => "warning".to_string(),
                },
            });
        }
    }

    serde_wasm_bindgen::to_value(&diagnostics).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Compile a Rex source string to REXC bytecode, returned as a hex string.
/// Pass an empty string for `rexd_source` to compile without domain-aware rewriting.
#[wasm_bindgen]
pub fn compile(source: &str, rexd_source: &str) -> Result<String, JsValue> {
    if rexd_source.is_empty() {
        Ok(rex_core::compile(source))
    } else {
        Ok(rex_core::compile_with_domain(source, rexd_source))
    }
}

/// Parse a Rex source string and return the AST as a JSON string.
#[wasm_bindgen]
pub fn parse(source: &str) -> Result<JsValue, JsValue> {
    let tokens = rex_core::lexer::lex(source);
    let (green, errors) = rex_core::parser::parse(source, &tokens);
    let root = rex_core::syntax::SyntaxNode::new_root(green);
    let value = rex_core::lower::lower(&root);

    let result = ParseResult {
        ast: format!("{value:?}"),
        errors: errors
            .iter()
            .map(|e| DiagnosticJs {
                message: e.message.clone(),
                start: e.span.start,
                end: e.span.end,
                severity: "error".to_string(),
            })
            .collect(),
    };

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[derive(serde::Serialize)]
struct DiagnosticJs {
    message: String,
    start: usize,
    end: usize,
    severity: String,
}

#[derive(serde::Serialize)]
struct ParseResult {
    ast: String,
    errors: Vec<DiagnosticJs>,
}
