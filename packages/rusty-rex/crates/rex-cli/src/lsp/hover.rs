use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use rex_core::typecheck::DomainSchema;

use super::format_type;

/// Produce hover info for a word at the cursor position.
pub fn hover(schema: &DomainSchema, word: &str) -> Option<Hover> {
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

    None
}
