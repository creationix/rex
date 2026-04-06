use lsp_types::{Location, Position, Range, Uri};
use rex_core::typecheck::DomainSchema;
use rex_core::{lexer, parser};
use rex_core::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

fn parse_root(source: &str) -> SyntaxNode {
    let tokens = lexer::lex(source);
    let (green, _errors) = parser::parse(source, &tokens);
    SyntaxNode::new_root(green)
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

fn location_from_offsets(uri: &Uri, source: &str, start: usize, end: usize) -> Location {
    Location {
        uri: uri.clone(),
        range: Range {
            start: offset_to_position(source, start),
            end: offset_to_position(source, end),
        },
    }
}

fn location_from_token(uri: &Uri, source: &str, token: &SyntaxToken) -> Location {
    let range = token.text_range();
    location_from_offsets(uri, source, range.start().into(), range.end().into())
}

fn assign_ident_and_value(node: &SyntaxNode) -> Option<(SyntaxToken, Option<SyntaxNode>)> {
    if node.kind() != SyntaxKind::AssignExpr {
        return None;
    }
    let children: Vec<_> = node
        .children_with_tokens()
        .filter(|child| child.as_token().map_or(true, |t| !t.kind().is_trivia()))
        .collect();
    let lhs = children.first()?.as_token().filter(|t| t.kind() == SyntaxKind::Ident)?.clone();

    let eq_idx = children.iter().position(|child| {
        matches!(
            child.as_token().map(|t| t.kind()),
            Some(SyntaxKind::Eq)
        )
    });

    if let Some(eq_idx) = eq_idx {
        let value = children.get(eq_idx + 1).and_then(|child| child.as_node().cloned());
        return Some((lhs, value));
    }

    let colon_idx = children.iter().position(|child| {
        matches!(child.as_token().map(|t| t.kind()), Some(SyntaxKind::Colon))
    })?;
    let eq_after_colon = children[colon_idx + 1..]
        .iter()
        .position(|child| matches!(child.as_token().map(|t| t.kind()), Some(SyntaxKind::Eq)))
        .map(|index| colon_idx + 1 + index);
    let value = eq_after_colon
        .and_then(|eq_idx| children.get(eq_idx + 1))
        .and_then(|child| child.as_node().cloned());
    Some((lhs, value))
}

fn extern_ident(node: &SyntaxNode) -> Option<SyntaxToken> {
    if node.kind() != SyntaxKind::ExternDecl {
        return None;
    }
    node.children_with_tokens()
        .filter(|child| child.as_token().map_or(true, |t| !t.kind().is_trivia()))
        .into_iter()
        .find_map(|child| child.as_token().filter(|t| t.kind() == SyntaxKind::Ident).cloned())
}

fn find_local_variable_definition(
    source: &str,
    name: &str,
    before_offset: usize,
) -> Option<(SyntaxToken, Option<SyntaxNode>)> {
    let root = parse_root(source);
    let mut found = None;

    for node in root.descendants() {
        let start: usize = node.text_range().start().into();
        if start > before_offset {
            continue;
        }

        if let Some((ident, value)) = assign_ident_and_value(&node) {
            if ident.text() == name {
                found = Some((ident, value));
            }
            continue;
        }

        if let Some(ident) = extern_ident(&node) {
            if ident.text() == name {
                found = Some((ident, None));
            }
        }
    }

    found
}

fn pair_key_and_value(node: &SyntaxNode) -> Option<(SyntaxToken, Option<SyntaxNode>)> {
    if node.kind() != SyntaxKind::Pair {
        return None;
    }
    let children: Vec<_> = node
        .children_with_tokens()
        .filter(|child| child.as_token().map_or(true, |t| !t.kind().is_trivia()))
        .collect();
    let colon_idx = children.iter().position(|child| {
        matches!(child.as_token().map(|t| t.kind()), Some(SyntaxKind::Colon))
    })?;
    let key = children.first()?.as_token().filter(|t| t.kind() == SyntaxKind::Ident)?.clone();
    let value = children.get(colon_idx + 1).and_then(|child| child.as_node().cloned());
    Some((key, value))
}

fn find_property_in_object(node: &SyntaxNode, key: &str) -> Option<(SyntaxToken, Option<SyntaxNode>)> {
    if !matches!(node.kind(), SyntaxKind::ObjectExpr | SyntaxKind::IndexedObjectExpr) {
        return None;
    }

    for child in node.children() {
        if let Some((pair_key, value)) = pair_key_and_value(&child) {
            if pair_key.text() == key {
                return Some((pair_key, value));
            }
        }
    }

    None
}

fn resolve_value_node(_source: &str, node: SyntaxNode, _before_offset: usize) -> Option<SyntaxNode> {
    match node.kind() {
        SyntaxKind::ObjectExpr | SyntaxKind::IndexedObjectExpr => Some(node),
        _ => None,
    }
}

fn dotted_word_start(source: &str, offset: usize) -> usize {
    source[..offset]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.')
        .map(|i| {
            let mut j = i + 1;
            while j < source.len() && !source.is_char_boundary(j) {
                j += 1;
            }
            j
        })
        .unwrap_or(0)
}

fn dotted_segment_index(source: &str, offset: usize, dot_word: &str) -> usize {
    let start = dotted_word_start(source, offset);
    let rel = offset.saturating_sub(start).min(dot_word.len().saturating_sub(1));
    let mut segment = 0usize;
    for (i, ch) in dot_word.char_indices() {
        if i >= rel {
            break;
        }
        if ch == '.' {
            segment += 1;
        }
    }
    segment
}

pub fn local_variable_definition(
    word: &str,
    uri: &Uri,
    source: &str,
    offset: usize,
) -> Option<Location> {
    let (ident, _) = find_local_variable_definition(source, word, offset)?;
    Some(location_from_token(uri, source, &ident))
}

pub fn local_nav_definition(
    dot_word: &str,
    uri: &Uri,
    source: &str,
    offset: usize,
) -> Option<Location> {
    let parts: Vec<&str> = dot_word.split('.').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }

    let segment_index = dotted_segment_index(source, offset, dot_word);
    let base = parts[0];
    if segment_index == 0 {
        return local_variable_definition(base, uri, source, offset);
    }

    let (_, mut current_value) = find_local_variable_definition(source, base, offset)?;
    for (index, part) in parts.iter().enumerate().skip(1) {
        let object_node = resolve_value_node(source, current_value?, offset)?;
        let (key_token, value_node) = find_property_in_object(&object_node, part)?;
        if index == segment_index {
            return Some(location_from_token(uri, source, &key_token));
        }
        current_value = value_node;
    }

    None
}

/// Find a local `type Name = ...` declaration in a Rex source document.
pub fn local_type_alias_definition(word: &str, uri: &Uri, source: &str) -> Option<Location> {
    for (line_num, line) in source.lines().enumerate() {
        let trimmed = line.trim();
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

pub fn local_type_property_definition(
    word: &str,
    uri: &Uri,
    source: &str,
    offset: usize,
) -> Option<Location> {
    let root = parse_root(source);
    let mut found = None;

    for node in root.descendants() {
        if node.kind() != SyntaxKind::TypePair {
            continue;
        }
        let start: usize = node.text_range().start().into();
        if start > offset {
            continue;
        }

        let children: Vec<_> = node
            .children_with_tokens()
            .filter(|child| child.as_token().map_or(true, |t| !t.kind().is_trivia()))
            .collect();
        let key = match children.first().and_then(|child| child.as_token()) {
            Some(t) if t.kind() == SyntaxKind::Ident && t.text() == word => t,
            _ => continue,
        };
        found = Some(location_from_token(uri, source, key));
    }

    found
}

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
        if let Some(loc) = local_type_alias_definition(word, uri, line) {
            return Some(Location {
                uri: loc.uri,
                range: Range {
                    start: Position::new(line_num as u32, loc.range.start.character),
                    end: Position::new(line_num as u32, loc.range.end.character),
                },
            });
        }
    }

    None
}
