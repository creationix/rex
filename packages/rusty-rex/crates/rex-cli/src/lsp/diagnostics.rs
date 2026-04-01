use lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use rex_core::typecheck::{self, DiagnosticKind, DomainSchema};

/// Run parse + typecheck on source and return LSP diagnostics.
pub fn compute_diagnostics(source: &str, schema: &DomainSchema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Parse errors
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

    // Type-check errors (only if parsing produced something)
    let type_diags = typecheck::check_source(source, schema);
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

    diagnostics
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
