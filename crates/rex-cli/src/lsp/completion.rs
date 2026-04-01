use lsp_types::{CompletionItem, CompletionItemKind};
use rex_core::typecheck::DomainSchema;

use super::format_type;

/// Keywords in the Rex language.
const KEYWORDS: &[&str] = &[
    "when", "unless", "else", "while", "for", "do", "end", "break", "continue", "and", "or",
    "in", "of", "true", "false", "null", "none", "return", "delete", "type", "extern", "mut",
];

/// Build completions from a DomainSchema and optional prefix context.
pub fn completions(schema: &DomainSchema, prefix: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // If prefix contains a dot, complete from nested properties or functions
    if let Some(dot_pos) = prefix.rfind('.') {
        let namespace = &prefix[..dot_pos];
        // Check functions that start with namespace.
        let fn_prefix = format!("{namespace}.");
        for (name, sig) in &schema.functions {
            if name.starts_with(&fn_prefix) {
                let short = &name[fn_prefix.len()..];
                let args_str: Vec<String> = sig
                    .args
                    .iter()
                    .map(|(n, t)| format!("{n}: {}", format_type(t)))
                    .collect();
                let detail = format!("({}) -> {}", args_str.join(", "), format_type(&sig.returns));
                let mut item = CompletionItem::new_simple(short.to_string(), detail);
                item.kind = Some(CompletionItemKind::FUNCTION);
                item.documentation = sig.doc.as_ref().map(|d| {
                    lsp_types::Documentation::String(d.clone())
                });
                items.push(item);
            }
        }

        // Check global properties
        if let Some(entry) = schema.globals.get(namespace) {
            if let rex_core::typecheck::Type::Object { fields, .. } = &entry.ty {
                for (key, ty) in fields {
                    let mut item =
                        CompletionItem::new_simple(key.clone(), format_type(ty));
                    item.kind = Some(CompletionItemKind::PROPERTY);
                    items.push(item);
                }
            }
        }
    } else {
        // Top-level completions: globals, functions (namespace part), keywords
        for (name, entry) in &schema.globals {
            let mut item = CompletionItem::new_simple(name.clone(), format_type(&entry.ty));
            item.kind = Some(CompletionItemKind::VARIABLE);
            item.documentation = entry.doc.as_ref().map(|d| {
                lsp_types::Documentation::String(d.clone())
            });
            items.push(item);
        }

        // Function namespaces (unique prefixes before the dot)
        let mut namespaces = std::collections::HashSet::new();
        for name in schema.functions.keys() {
            if let Some(dot_pos) = name.find('.') {
                namespaces.insert(&name[..dot_pos]);
            }
        }
        for ns in namespaces {
            // Only add if not already a global
            if !schema.globals.contains_key(ns) {
                let mut item = CompletionItem::new_simple(ns.to_string(), "namespace".to_string());
                item.kind = Some(CompletionItemKind::MODULE);
                items.push(item);
            }
        }

        // Type aliases
        for (name, ty) in &schema.type_aliases {
            let mut item = CompletionItem::new_simple(name.clone(), format_type(ty));
            item.kind = Some(CompletionItemKind::CLASS);
            items.push(item);
        }

        // Keywords
        for kw in KEYWORDS {
            let mut item = CompletionItem::new_simple(kw.to_string(), "keyword".to_string());
            item.kind = Some(CompletionItemKind::KEYWORD);
            items.push(item);
        }
    }

    items
}
