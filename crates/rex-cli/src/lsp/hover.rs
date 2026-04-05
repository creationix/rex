use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use rex_core::typecheck::DomainSchema;

use super::format_type;

/// Produce hover info for a word at the cursor position.
/// `is_type_context` is true for .rexd files or type annotation positions.
pub fn hover(schema: &DomainSchema, word: &str, is_type_context: bool) -> Option<Hover> {
    // Check globals
    if let Some(entry) = schema.globals.get(word) {
        let mut text = format!("```\nextern {word}: {}\n```", format_type(&entry.ty));
        if let Some(doc) = &entry.doc {
            text.push_str("\n\n---\n\n");
            text.push_str(doc);
        }
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text,
            }),
            range: None,
        });
    }

    // Check functions (exact match or namespace.method)
    if let Some(sig) = schema.functions.get(word) {
        let args_str: Vec<String> = sig
            .args
            .iter()
            .map(|(n, t)| format!("{n}: {}", format_type(t)))
            .collect();
        let mut text = format!(
            "```\nextern {word}({}) -> {}\n```",
            args_str.join(", "),
            format_type(&sig.returns)
        );
        if let Some(doc) = &sig.doc {
            text.push_str("\n\n---\n\n");
            text.push_str(doc);
        }
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text,
            }),
            range: None,
        });
    }

    // Check type aliases
    if let Some(ty) = schema.type_aliases.get(word) {
        let text = format!("```\ntype {word} = {}\n```", format_type(ty));
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text,
            }),
            range: None,
        });
    }

    // Check built-in type keywords (only in type context — inside TypeExpr CST nodes)
    let builtin_desc = if !is_type_context { None } else { match word {
        "str" => Some("Built-in string type"),
        "int" => Some("Built-in integer type (whole numbers)"),
        "num" => Some("Built-in number type (integer or decimal)"),
        "bool" => Some("Built-in boolean type (true or false)"),
        "null" => Some("The null value"),
        "none" => Some("The absence of a value — only `none` is falsy in Rex"),
        "some" => Some("Any defined value — must narrow before use"),
        "unknown" => Some("Any value or none — alias for `some | none`"),
        "never" => Some("Unreachable — function doesn't return"),
        _ => None,
    }};
    if let Some(desc) = builtin_desc {
        let text = format!("```\ntype {word}\n```\n\n---\n\n{desc}");
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text,
            }),
            range: None,
        });
    }

    // Check if word is a namespace prefix (e.g., "log" matches "log.info", "log.warn")
    let prefix = format!("{word}.");
    let mut members: Vec<String> = Vec::new();
    for (name, sig) in &schema.functions {
        if let Some(method) = name.strip_prefix(&prefix) {
            let args_str: Vec<String> = sig
                .args
                .iter()
                .map(|(n, t)| format!("{n}: {}", format_type(t)))
                .collect();
            let line = format!(
                "{word}.{method}({}) -> {}",
                args_str.join(", "),
                format_type(&sig.returns)
            );
            members.push(line);
        }
    }
    if !members.is_empty() {
        members.sort();
        let body = members.join("\n");
        let text = format!("```\n{body}\n```");
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text,
            }),
            range: None,
        });
    }

    None
}
