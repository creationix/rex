use lsp_types::{Location, Position, Range, Uri};
use rex_core::typecheck::DomainSchema;

/// Find the definition of a symbol in the .rexd source.
/// Returns a location pointing to the line in the .rexd file where the symbol is declared.
pub fn definition(
    schema: &DomainSchema,
    word: &str,
    rexd_uri: Option<&Uri>,
    rexd_source: Option<&str>,
) -> Option<Location> {
    let uri = rexd_uri?;
    let source = rexd_source?;

    // The symbol must exist in the schema
    let is_known = schema.globals.contains_key(word)
        || schema.functions.contains_key(word)
        || schema.type_aliases.contains_key(word);
    if !is_known {
        return None;
    }

    // Search the .rexd source for the declaration line
    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("extern ") || trimmed.starts_with("extern\t") {
            let rest = trimmed["extern ".len()..].trim();
            let rest = if rest.starts_with("mut ") {
                rest["mut ".len()..].trim()
            } else {
                rest
            };
            let ident_end = rest
                .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                .unwrap_or(rest.len());
            let ident = &rest[..ident_end];
            if ident == word || ident.starts_with(&format!("{word}.")) {
                let col = line.find(word).unwrap_or(0);
                return Some(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position::new(line_num as u32, col as u32),
                        end: Position::new(line_num as u32, (col + word.len()) as u32),
                    },
                });
            }
        }
        if trimmed.starts_with("type ") || trimmed.starts_with("type\t") {
            let rest = trimmed["type ".len()..].trim();
            let ident_end = rest
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .unwrap_or(rest.len());
            let ident = &rest[..ident_end];
            if ident == word {
                let col = line.find(word).unwrap_or(0);
                return Some(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: Position::new(line_num as u32, col as u32),
                        end: Position::new(line_num as u32, (col + word.len()) as u32),
                    },
                });
            }
        }
    }

    None
}
