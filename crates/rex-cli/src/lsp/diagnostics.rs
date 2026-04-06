use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use rex_core::typecheck::{self, DiagnosticKind, DomainSchema};

/// Run parse + typecheck and return both LSP diagnostics and a span→type map.
pub fn compute_diagnostics_with_types(
    source: &str,
    schema: &DomainSchema,
) -> (
    Vec<Diagnostic>,
    Vec<(std::ops::Range<usize>, typecheck::Type)>,
    std::collections::HashMap<String, typecheck::FunctionSig>,
    std::collections::HashMap<String, typecheck::Type>,
) {
    let mut diagnostics = Vec::new();

    let tokens = rex_core::lexer::lex(source);
    let (_, parse_errors) = rex_core::parser::parse(source, &tokens);
    for e in &parse_errors {
        diagnostics.push(Diagnostic {
            range: span_to_range(source, e.span.start, e.span.end),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("rex".to_string()),
            message: e.message.clone(),
            ..Default::default()
        });
    }

    let (type_diags, span_types, inline_fns, inline_aliases) = typecheck::check_source_with_types(source, schema);
    for d in &type_diags {
        let severity = match d.kind {
            DiagnosticKind::Error => DiagnosticSeverity::ERROR,
            DiagnosticKind::Warning => DiagnosticSeverity::WARNING,
        };
        diagnostics.push(Diagnostic {
            range: span_to_range(source, d.span.start, d.span.end),
            severity: Some(severity),
            source: Some("rex".to_string()),
            message: d.message.clone(),
            ..Default::default()
        });
    }

    (diagnostics, span_types, inline_fns, inline_aliases)
}

/// Convert byte offsets to an LSP Range (0-indexed line/col).
fn span_to_range(source: &str, start: usize, end: usize) -> Range {
    Range {
        start: offset_to_position(source, start),
        end: offset_to_position(source, end),
    }
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position::new(line, col)
}
