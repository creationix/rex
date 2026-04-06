mod completion;
mod definition;
mod diagnostics;
mod document;
mod hover;

use std::io;
use std::path::PathBuf;

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
    Notification as _, PublishDiagnostics,
};
use lsp_types::request::{Completion, Formatting, GotoDefinition, HoverRequest, Rename, SemanticTokensFullRequest, Request as _};
use lsp_types::{
    CompletionOptions, CompletionParams, GotoDefinitionResponse, HoverParams,
    HoverProviderCapability, InitializeParams, PublishDiagnosticsParams, ServerCapabilities,
    RenameParams, TextEdit, WorkspaceEdit,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
    SemanticTokenModifier, SemanticTokenType,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensServerCapabilities,
};
use rex_core::typecheck::{self, DomainSchema};

use self::document::DocumentStore;

/// Format a Rex Type as a human-readable string.
fn format_type(ty: &typecheck::Type) -> String {
    format_type_with_aliases(ty, &std::collections::HashMap::new())
}

/// Format a Rex Type with optional structural alias substitution.
///
/// Heuristic:
/// - Keep the top-level type structural.
/// - For nested positions, substitute an alias only when there is one exact match.
/// - If multiple aliases match the same nested type, keep structural output.
fn format_type_with_aliases(
    ty: &typecheck::Type,
    aliases: &std::collections::HashMap<String, typecheck::Type>,
) -> String {
    fn matching_alias(
        ty: &typecheck::Type,
        aliases: &std::collections::HashMap<String, typecheck::Type>,
    ) -> Option<String> {
        let mut matches: Vec<String> = aliases
            .iter()
            .filter_map(|(name, candidate)| if candidate == ty { Some(name.clone()) } else { None })
            .collect();
        matches.sort();
        if matches.len() == 1 {
            Some(matches[0].clone())
        } else {
            None
        }
    }

    fn format_inner(
        ty: &typecheck::Type,
        aliases: &std::collections::HashMap<String, typecheck::Type>,
        is_top_level: bool,
    ) -> String {
        use typecheck::Type;
        if !is_top_level {
            if let Some(alias) = matching_alias(ty, aliases) {
                return alias;
            }
        }

        match ty {
            Type::Some => "some".to_string(),
            Type::None => "none".to_string(),
            Type::Never => "never".to_string(),
            Type::Null => "null".to_string(),
            Type::Bool => "bool".to_string(),
            Type::Int => "int".to_string(),
            Type::Num => "num".to_string(),
            Type::Str => "str".to_string(),
            Type::LiteralStr(s) => format!("\"{s}\""),
            Type::Array(elem) => format!("[{}]", format_inner(elem, aliases, false)),
            Type::Object { fields, wildcard } => {
                let mut parts: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", format_inner(v, aliases, false)))
                    .collect();
                if let Some(w) = wildcard {
                    parts.push(format!("*: {}", format_inner(w, aliases, false)));
                }
                format!("{{{}}}", parts.join(", "))
            }
            Type::Union(types) => {
                let parts: Vec<String> = types
                    .iter()
                    .map(|t| format_inner(t, aliases, false))
                    .collect();
                parts.join(" | ")
            }
            Type::Intersection(types) => {
                let parts: Vec<String> = types
                    .iter()
                    .map(|t| format_inner(t, aliases, false))
                    .collect();
                parts.join(" & ")
            }
            Type::Ref(name) => name.clone(),
        }
    }

    format_inner(ty, aliases, true)
}

fn builtin_method_hover(name: &str) -> Option<&'static str> {
    match name {
        // Array methods
        "push"       => Some("array.push(val: some) -> array"),
        "pop"        => Some("array.pop() -> some | none"),
        "join"       => Some("array.join(sep: str) -> str"),
        "indexOf"    => Some("array.indexOf(val: some) -> int | none\nstr.indexOf(sub: str) -> int | none"),
        "contains"   => Some("array.contains(val: some) -> some | none\nstr.contains(sub: str) -> str | none"),
        "slice"      => Some("array.slice(start: int, end: int) -> array\nstr.slice(start: int, end: int) -> str"),
        // String methods
        "split"      => Some("str.split(sep: str) -> [str]"),
        "trim"       => Some("str.trim() -> str"),
        "upper"      => Some("str.upper() -> str"),
        "lower"      => Some("str.lower() -> str"),
        "replace"    => Some("str.replace(from: str, to: str) -> str"),
        "starts-with" => Some("str.starts-with(prefix: str) -> str | none"),
        "ends-with"  => Some("str.ends-with(suffix: str) -> str | none"),
        _ => None,
    }
}

fn is_type_name(word: &str) -> bool {
    matches!(word, "str" | "int" | "num" | "bool" | "some" | "none" | "null" | "unknown" | "never")
}

/// Extract a file path from a file:// URI string.
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let s = uri.as_str();
    if let Some(rest) = s.strip_prefix("file://") {
        // Decode percent-encoding
        let decoded = percent_decode(rest);
        Some(PathBuf::from(decoded))
    } else {
        None
    }
}

/// Create a file:// URI from a path.
fn path_to_uri(path: &std::path::Path) -> Option<Uri> {
    let s = format!("file://{}", path.display());
    s.parse().ok()
}

/// Simple percent-decoding for file URIs.
fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().unwrap_or(0);
            let h2 = chars.next().unwrap_or(0);
            if let (Some(d1), Some(d2)) = (hex_digit(h1), hex_digit(h2)) {
                result.push((d1 << 4 | d2) as char);
            } else {
                result.push('%');
                result.push(h1 as char);
                result.push(h2 as char);
            }
        } else {
            result.push(b as char);
        }
    }
    result
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

struct LspState {
    documents: DocumentStore,
    schema: DomainSchema,
    rexd_path: Option<PathBuf>,
    rexd_source: Option<String>,
    rexd_uri: Option<Uri>,
    /// Cached span→type map per document URI (updated on each diagnostics pass).
    span_types: std::collections::HashMap<Uri, Vec<(std::ops::Range<usize>, typecheck::Type)>>,
    /// Inline function signatures per document URI.
    inline_functions: std::collections::HashMap<Uri, std::collections::HashMap<String, typecheck::FunctionSig>>,
    /// Inline type aliases per document URI.
    inline_aliases: std::collections::HashMap<Uri, std::collections::HashMap<String, typecheck::Type>>,
}

impl LspState {
    fn new(domain: Option<PathBuf>) -> Self {
        let (schema, rexd_path, rexd_source, rexd_uri) = match domain {
            Some(path) => load_rexd(&path),
            None => (DomainSchema::default(), None, None, None),
        };
        Self {
            documents: DocumentStore::new(),
            schema,
            rexd_path,
            rexd_source,
            rexd_uri,
            span_types: std::collections::HashMap::new(),
            inline_functions: std::collections::HashMap::new(),
            inline_aliases: std::collections::HashMap::new(),
        }
    }

    /// Try to discover and load a .rexd file from a document URI.
    fn discover_rexd(&mut self, doc_uri: &Uri) {
        if self.rexd_path.is_some() {
            return;
        }
        if let Some(path) = uri_to_path(doc_uri) {
            if let Some(rexd_path) = crate::find_rexd(&path) {
                let (schema, path, source, uri) = load_rexd(&rexd_path);
                self.schema = schema;
                self.rexd_path = path;
                self.rexd_source = source;
                self.rexd_uri = uri;
            }
        }
    }

    fn publish_diagnostics(&mut self, uri: &Uri, conn: &Connection) {
        let Some(source) = self.documents.get(uri) else {
            return;
        };
        let is_rexd = uri.as_str().ends_with(".rexd");
        let (diags, types, fns, aliases) = if is_rexd {
            (
                Vec::new(),
                Vec::new(),
                std::collections::HashMap::new(),
                std::collections::HashMap::new(),
            )
        } else {
            diagnostics::compute_diagnostics_with_types(source, &self.schema)
        };
        self.span_types.insert(uri.clone(), types);
        self.inline_functions.insert(uri.clone(), fns);
        self.inline_aliases.insert(uri.clone(), aliases);
        let params = PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: diags,
            version: None,
        };
        let notif = Notification::new(PublishDiagnostics::METHOD.to_string(), params);
        let _ = conn.sender.send(Message::Notification(notif));
    }

    fn reload_rexd(&mut self) {
        if let Some(path) = &self.rexd_path.clone() {
            let (schema, rexd_path, source, uri) = load_rexd(path);
            self.schema = schema;
            self.rexd_path = rexd_path;
            self.rexd_source = source;
            self.rexd_uri = uri;
        }
    }
}

fn load_rexd(
    path: &std::path::Path,
) -> (DomainSchema, Option<PathBuf>, Option<String>, Option<Uri>) {
    match std::fs::read_to_string(path) {
        Ok(source) => {
            let schema = typecheck::parse_rexd(&source);
            let uri = path_to_uri(path);
            (schema, Some(path.to_path_buf()), Some(source), uri)
        }
        Err(_) => (DomainSchema::default(), None, None, None),
    }
}

pub fn run(domain: Option<PathBuf>) -> io::Result<()> {
    let (conn, io_threads) = Connection::stdio();

    // Only classify identifiers — TM grammar handles keywords, operators,
    // literals, strings, comments (matching how the TS/JS LSP works).
    let token_types = vec![
        SemanticTokenType::VARIABLE,     // 0
        SemanticTokenType::PROPERTY,     // 1
        SemanticTokenType::FUNCTION,     // 2
        SemanticTokenType::TYPE,         // 3
        SemanticTokenType::PARAMETER,    // 4 — for-loop bindings
    ];
    let token_modifiers = vec![
        SemanticTokenModifier::DECLARATION,      // bit 0
        SemanticTokenModifier::DEFAULT_LIBRARY,  // bit 1
    ];

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
        rename_provider: Some(lsp_types::OneOf::Left(true)),
        document_formatting_provider: Some(lsp_types::OneOf::Left(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types,
                    token_modifiers,
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                ..Default::default()
            },
        )),
        ..Default::default()
    };

    let server_capabilities = serde_json::to_value(capabilities).unwrap();
    let init_params = conn
        .initialize(server_capabilities)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
    let init_params: InitializeParams = serde_json::from_value(init_params).unwrap();

    // Check initializationOptions for domain path
    let domain = domain.or_else(|| {
        init_params
            .initialization_options
            .as_ref()
            .and_then(|opts| opts.get("domain"))
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
    });

    // If no explicit domain, try auto-discovery from workspace folders or rootUri
    let domain = domain.or_else(|| {
        // Try workspace folders first
        if let Some(folders) = &init_params.workspace_folders {
            for folder in folders {
                if let Some(path) = uri_to_path(&folder.uri) {
                    if let Some(rexd) = crate::find_rexd(&path) {
                        return Some(rexd);
                    }
                }
            }
        }
        // Fallback to rootUri
        #[allow(deprecated)]
        init_params
            .root_uri
            .as_ref()
            .and_then(|uri| uri_to_path(uri))
            .and_then(|path| crate::find_rexd(&path))
    });

    let mut state = LspState::new(domain);

    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn
                    .handle_shutdown(&req)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?
                {
                    break;
                }
                handle_request(&mut state, &conn, req);
            }
            Message::Notification(notif) => {
                handle_notification(&mut state, &conn, notif);
            }
            Message::Response(_) => {}
        }
    }

    io_threads.join()?;
    Ok(())
}

fn handle_request(state: &mut LspState, conn: &Connection, req: Request) {
    if req.method == Completion::METHOD {
        let (id, params) = cast_request::<Completion>(req);
        let items = handle_completion(state, params);
        let result = serde_json::to_value(items).unwrap();
        send_response(conn, id, result);
    } else if req.method == HoverRequest::METHOD {
        let (id, params) = cast_request::<HoverRequest>(req);
        let result = handle_hover(state, params);
        let result = serde_json::to_value(result).unwrap();
        send_response(conn, id, result);
    } else if req.method == GotoDefinition::METHOD {
        let (id, params) = cast_request::<GotoDefinition>(req);
        let result = handle_definition(state, params);
        let result = serde_json::to_value(result).unwrap();
        send_response(conn, id, result);
    } else if req.method == Rename::METHOD {
        let (id, params) = cast_request::<Rename>(req);
        let result = handle_rename(state, params);
        let result = serde_json::to_value(result).unwrap();
        send_response(conn, id, result);
    } else if req.method == Formatting::METHOD {
        let (id, params) = cast_request::<Formatting>(req);
        let result = handle_format(state, params);
        let result = serde_json::to_value(result).unwrap();
        send_response(conn, id, result);
    } else if req.method == SemanticTokensFullRequest::METHOD {
        let (id, params) = cast_request::<SemanticTokensFullRequest>(req);
        let result = handle_semantic_tokens(state, params);
        let result = serde_json::to_value(result).unwrap();
        send_response(conn, id, result);
    }
}

fn handle_notification(state: &mut LspState, conn: &Connection, notif: Notification) {
    if notif.method == DidOpenTextDocument::METHOD {
        let params: lsp_types::DidOpenTextDocumentParams =
            serde_json::from_value(notif.params).unwrap();
        let uri = params.text_document.uri.clone();
        state.documents.open(uri.clone(), params.text_document.text);
        state.discover_rexd(&uri);
        state.publish_diagnostics(&uri, conn);
    } else if notif.method == DidChangeTextDocument::METHOD {
        let params: lsp_types::DidChangeTextDocumentParams =
            serde_json::from_value(notif.params).unwrap();
        let uri = params.text_document.uri.clone();
        // We use full sync, so take the last content change
        if let Some(change) = params.content_changes.into_iter().last() {
            state.documents.change(&uri, change.text);
        }
        state.publish_diagnostics(&uri, conn);
    } else if notif.method == DidSaveTextDocument::METHOD {
        let params: lsp_types::DidSaveTextDocumentParams =
            serde_json::from_value(notif.params).unwrap();
        let uri = params.text_document.uri;
        // Reload .rexd in case it changed on disk
        state.reload_rexd();
        state.publish_diagnostics(&uri, conn);
    } else if notif.method == DidCloseTextDocument::METHOD {
        let params: lsp_types::DidCloseTextDocumentParams =
            serde_json::from_value(notif.params).unwrap();
        state.documents.close(&params.text_document.uri);
    }
}

fn handle_completion(
    state: &LspState,
    params: CompletionParams,
) -> Vec<lsp_types::CompletionItem> {
    let uri = &params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;
    let Some(source) = state.documents.get(uri) else {
        return vec![];
    };

    let prefix = extract_word_before(source, pos);
    completion::completions(&state.schema, &prefix)
}

fn handle_hover(state: &LspState, params: HoverParams) -> Option<lsp_types::Hover> {
    let uri = &params.text_document_position_params.text_document.uri;
    let pos = params.text_document_position_params.position;
    let source = state.documents.get(uri)?;

    let word = extract_word_at(source, pos);
    if word.is_empty() {
        return None;
    }

    let is_rexd = uri.as_str().ends_with(".rexd");

    // Check if cursor is inside a type node by walking the CST
    let is_type_context = is_rexd || {
        let offset = position_to_offset(source, pos);
        is_in_type_node(source, offset)
    };

    // Try dotted word (e.g., "json.parse") — but only if cursor is past the dot,
    // so hovering on "json" doesn't show the info for "json.parse"
    let dot_word = extract_dotted_word_at(source, pos);
    if !dot_word.is_empty() && dot_word != word && dot_word.ends_with(&word) {
        if let result @ Some(_) = hover::hover(&state.schema, &dot_word, is_type_context) {
            return result;
        }
    }

    // Try domain schema lookup for the simple word
    if let result @ Some(_) = hover::hover(&state.schema, &word, is_type_context) {
        return result;
    }

    // In .rexd files, check if the word is a parameter name in a function signature
    if is_rexd {
        if let Some(hover) = hover_rexd_param(&state.schema, source, pos, &word) {
            return Some(hover);
        }
    }

    // Check built-in methods (only when cursor follows a dot)
    let offset = position_to_offset(source, pos);
    let word_start = source[..offset]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|i| i + 1)
        .unwrap_or(0);
    let after_dot = word_start > 0 && source.as_bytes()[word_start - 1] == b'.';
    if after_dot {
        if let Some(desc) = builtin_method_hover(&word) {
            let text = format!("```rex\n{desc}\n```");
            return Some(lsp_types::Hover {
                contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: text,
                }),
                range: None,
            });
        }
    }

    // Check inline function signatures from the current file
    if let Some(inline_fns) = state.inline_functions.get(uri) {
        if let Some(sig) = inline_fns.get(&word) {
            let args_str: Vec<String> = sig.args.iter()
                .map(|(n, t)| format!("{n}: {}", format_type(t)))
                .collect();
            let text = format!(
                "```rex\nextern {word}({}) -> {}\n```",
                args_str.join(", "),
                format_type(&sig.returns)
            );
            return Some(lsp_types::Hover {
                contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: text,
                }),
                range: None,
            });
        }
    }

    // Fall back to inferred type from span→type map
    if let Some(span_types) = state.span_types.get(uri) {
        let offset = position_to_offset(source, pos);

        // Compute the exact byte range of the word under cursor
        let word_start = source[..offset]
            .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .map(|i| next_char_boundary(source, i))
            .unwrap_or(0);
        let word_end = source[offset..]
            .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .map(|i| i + offset)
            .unwrap_or(source.len());

        // If cursor is on a namespace prefix (word followed by `.`), prefer
        // domain namespace hover when it exists, but otherwise continue to the
        // inferred span fallback so local navigation segments still hover.
        if word_end < source.len() && source.as_bytes()[word_end] == b'.' {
            // Try domain namespace hover first
            if let result @ Some(_) = hover::hover(&state.schema, &word, is_type_context) {
                return result;
            }
        }

        // Find span that exactly matches the word's byte range, or smallest enclosing
        let word_range = word_start..word_end;
        let mut exact: Option<&(std::ops::Range<usize>, typecheck::Type)> = None;
        let mut smallest: Option<&(std::ops::Range<usize>, typecheck::Type)> = None;
        for entry in span_types {
            // Keep the first exact match. Later entries can be broader or widened
            // re-inferences for the same span; the earliest token-level match is
            // the most specific for identifier hover.
            if exact.is_none() && entry.0 == word_range {
                exact = Some(entry);
            }
            if entry.0.contains(&offset) {
                if let Some(prev) = smallest {
                    if entry.0.len() < prev.0.len() {
                        smallest = Some(entry);
                    }
                } else {
                    smallest = Some(entry);
                }
            }
        }

        let selected = exact.or(smallest);
        if let Some((selected_range, ty)) = selected {
            let label = &source[selected_range.clone()];
            let mut aliases = state.schema.type_aliases.clone();
            if let Some(local_aliases) = state.inline_aliases.get(uri) {
                aliases.extend(local_aliases.clone());
            }
            let type_str = format_type_with_aliases(ty, &aliases);
            // If the word IS a type keyword and its type is itself, show `: type`
            let display_type = if type_str == label && is_type_name(label) {
                "type".to_string()
            } else {
                type_str
            };
            let content = lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: format!("```rex\n{label}: {display_type}\n```"),
            });
            return Some(lsp_types::Hover { contents: content, range: None });
        }
    }

    None
}

fn handle_definition(
    state: &LspState,
    params: lsp_types::GotoDefinitionParams,
) -> Option<GotoDefinitionResponse> {
    let uri = &params.text_document_position_params.text_document.uri;
    let pos = params.text_document_position_params.position;
    let source = state.documents.get(uri)?;

    let word = extract_word_at(source, pos);
    if word.is_empty() {
        return None;
    }

    if let Some(loc) = definition::local_type_alias_definition(&word, uri, source) {
        return Some(GotoDefinitionResponse::Scalar(loc));
    }
    if let Some(loc) = definition::local_type_property_definition(&word, uri, source, position_to_offset(source, pos)) {
        return Some(GotoDefinitionResponse::Scalar(loc));
    }

    // Try dotted word first
    let dot_word = extract_dotted_word_at(source, pos);
    if !dot_word.is_empty() {
        if let Some(loc) = definition::local_nav_definition(&dot_word, uri, source, position_to_offset(source, pos)) {
            return Some(GotoDefinitionResponse::Scalar(loc));
        }
        if let Some(loc) = definition::definition(
            &state.schema,
            &dot_word,
            state.rexd_uri.as_ref(),
            state.rexd_source.as_deref(),
        ) {
            return Some(GotoDefinitionResponse::Scalar(loc));
        }
    }

    if let Some(loc) = definition::local_variable_definition(&word, uri, source, position_to_offset(source, pos)) {
        return Some(GotoDefinitionResponse::Scalar(loc));
    }

    let loc = definition::definition(
        &state.schema,
        &word,
        state.rexd_uri.as_ref(),
        state.rexd_source.as_deref(),
    )?;
    Some(GotoDefinitionResponse::Scalar(loc))
}

fn handle_rename(state: &LspState, params: RenameParams) -> Option<WorkspaceEdit> {
    let uri = &params.text_document_position.text_document.uri;
    let pos = params.text_document_position.position;
    let source = state.documents.get(uri)?;
    let new_name = params.new_name;

    if new_name.trim().is_empty() {
        return None;
    }

    let word = extract_word_at(source, pos);
    if word.is_empty() {
        return None;
    }

    let cursor_offset = position_to_offset(source, pos);
    let word_start = source[..cursor_offset]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|i| next_char_boundary(source, i))
        .unwrap_or(0);
    let is_property_segment = word_start > 0 && source.as_bytes()[word_start - 1] == b'.';

    let mut edits: Vec<TextEdit> = Vec::new();
    let tokens = rex_core::lexer::lex(source);

    if is_property_segment {
        let dot_word = extract_dotted_word_at(source, pos);
        let target_loc = definition::local_nav_definition(&dot_word, uri, source, cursor_offset)?;

        for tok in &tokens {
            if tok.kind != rex_core::lexer::TokenKind::Ident {
                continue;
            }
            let tok_text = &source[tok.span.clone()];
            if tok_text != word {
                continue;
            }

            let is_decl = {
                let start = tok.span.start;
                let end = tok.span.end;
                let def_start = position_to_offset(source, target_loc.range.start);
                let def_end = position_to_offset(source, target_loc.range.end);
                start == def_start && end == def_end
            };

            let matches_symbol = if is_decl {
                true
            } else {
                let token_pos = offset_to_position(source, tok.span.start);
                let token_dot = extract_dotted_word_at(source, token_pos);
                if token_dot.is_empty() {
                    false
                } else {
                    definition::local_nav_definition(&token_dot, uri, source, tok.span.start)
                        .map_or(false, |loc| loc.range == target_loc.range && loc.uri == target_loc.uri)
                }
            };

            if matches_symbol {
                edits.push(TextEdit {
                    range: lsp_types::Range {
                        start: offset_to_position(source, tok.span.start),
                        end: offset_to_position(source, tok.span.end),
                    },
                    new_text: new_name.clone(),
                });
            }
        }
    } else {
        let target_loc = definition::local_variable_definition(&word, uri, source, cursor_offset)
            .or_else(|| definition::local_type_alias_definition(&word, uri, source))
            .or_else(|| definition::local_type_property_definition(&word, uri, source, cursor_offset))?;

        for tok in &tokens {
            if tok.kind != rex_core::lexer::TokenKind::Ident {
                continue;
            }
            let tok_text = &source[tok.span.clone()];
            if tok_text != word {
                continue;
            }

            let resolved = definition::local_variable_definition(&word, uri, source, tok.span.start)
                .or_else(|| definition::local_type_alias_definition(&word, uri, source))
                .or_else(|| definition::local_type_property_definition(&word, uri, source, tok.span.start));
            if resolved.map_or(false, |loc| loc.range == target_loc.range && loc.uri == target_loc.uri) {
                edits.push(TextEdit {
                    range: lsp_types::Range {
                        start: offset_to_position(source, tok.span.start),
                        end: offset_to_position(source, tok.span.end),
                    },
                    new_text: new_name.clone(),
                });
            }
        }
    }

    if edits.is_empty() {
        return None;
    }

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), edits);
    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

fn handle_format(
    state: &LspState,
    params: lsp_types::DocumentFormattingParams,
) -> Option<Vec<lsp_types::TextEdit>> {
    let source = state.documents.get(&params.text_document.uri)?;
    let formatted = rex_core::format(source);
    if formatted == *source {
        return Some(vec![]);
    }
    // Replace the entire document
    let line_count = source.lines().count().max(1);
    let last_line = source.lines().last().unwrap_or("");
    Some(vec![lsp_types::TextEdit {
        range: lsp_types::Range {
            start: lsp_types::Position { line: 0, character: 0 },
            end: lsp_types::Position {
                line: line_count as u32,
                character: last_line.len() as u32,
            },
        },
        new_text: formatted,
    }])
}

fn handle_semantic_tokens(
    state: &LspState,
    params: lsp_types::SemanticTokensParams,
) -> Option<lsp_types::SemanticTokensResult> {
    let uri = &params.text_document.uri;
    let source = state.documents.get(uri)?;
    let tokens = rex_core::lexer::lex(source);

    use rex_core::lexer::TokenKind;

    // Token type indices — must match legend order
    const TT_VARIABLE: u32 = 0;
    const TT_PROPERTY: u32 = 1;
    const TT_FUNCTION: u32 = 2;
    const TT_TYPE: u32 = 3;
    const TT_PARAMETER: u32 = 4;

    // Modifier bits
    const MOD_DEFAULT_LIBRARY: u32 = 1 << 1;

    let mut data = Vec::new();
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    // Track whether we're between `for` and `in`/`of` to detect loop bindings
    let mut in_for_binding = false;

    for (i, tok) in tokens.iter().enumerate() {
        // Track for-loop binding state across all tokens
        match tok.kind {
            TokenKind::KwFor => { in_for_binding = true; continue; }
            TokenKind::KwIn | TokenKind::KwOf => { in_for_binding = false; continue; }
            // Commas between bindings are fine, everything else exits
            _ if in_for_binding && tok.kind != TokenKind::Ident
                && tok.kind != TokenKind::Whitespace
                && tok.kind != TokenKind::Comma
                && tok.kind != TokenKind::Colon => {
                in_for_binding = false;
            }
            _ => {}
        }

        // Only classify identifiers — TM grammar handles everything else
        if tok.kind != TokenKind::Ident {
            continue;
        }

        let text = &source[tok.span.clone()];

        // Skip whitespace to find contextual neighbors
        let next_nonws = tokens[i + 1..].iter()
            .find(|t| t.kind != TokenKind::Whitespace).map(|t| t.kind);
        let prev_nonws = tokens[..i].iter().rev()
            .find(|t| t.kind != TokenKind::Whitespace).map(|t| t.kind);

        // Skip `mut` — TM grammar handles it as storage.modifier
        if text == "mut" { continue; }

        let (token_type, modifiers) = match text {
            // Built-in type predicates → function + defaultLibrary (like console.log in TS)
            "isString" | "isNumber" | "isInteger" | "isBoolean"
            | "isArray" | "isObject" => (TT_FUNCTION, MOD_DEFAULT_LIBRARY),

            // Built-in type names → type
            "str" | "int" | "num" | "bool" | "some"
            | "never" | "unknown" => (TT_TYPE, 0),

            // Everything else: classify by context
            _ => {
                if in_for_binding {
                    // Between `for` and `in`/`of`: loop binding parameter
                    (TT_PARAMETER, 0)
                } else if prev_nonws == Some(TokenKind::Dot) {
                    // After dot: property access (obj.field)
                    (TT_PROPERTY, 0)
                } else if next_nonws == Some(TokenKind::LParen) {
                    // Before paren: function call
                    (TT_FUNCTION, 0)
                } else if next_nonws == Some(TokenKind::Colon) {
                    // Before tight colon (key:value) inside {}: property
                    // Before spaced colon (name: Type): variable/type annotation
                    let colon_tok = tokens[i + 1..].iter()
                        .find(|t| t.kind == TokenKind::Colon);
                    let tight = colon_tok.map_or(false, |c| {
                        let after = c.span.end;
                        after < source.len() && source.as_bytes()[after] != b' '
                    });
                    if tight && is_in_braces(source, tok.span.start) {
                        (TT_PROPERTY, 0)
                    } else {
                        (TT_VARIABLE, 0)
                    }
                } else {
                    (TT_VARIABLE, 0)
                }
            }
        };

        let (line, col) = offset_to_line_col_0(source, tok.span.start);
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 { col - prev_start } else { col };

        data.push(lsp_types::SemanticToken {
            delta_line,
            delta_start,
            length: text.len() as u32,
            token_type,
            token_modifiers_bitset: modifiers,
        });

        prev_line = line;
        prev_start = col;
    }

    Some(lsp_types::SemanticTokensResult::Tokens(lsp_types::SemanticTokens {
        result_id: None,
        data,
    }))
}

/// Check if a byte offset is inside curly braces (for distinguishing object keys from typed vars).
fn is_in_braces(source: &str, offset: usize) -> bool {
    let mut depth = 0i32;
    for ch in source[..offset].chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

/// Convert byte offset to 0-indexed (line, col) pair.
fn offset_to_line_col_0(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if i >= offset { break; }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Check if a byte offset is inside a type expression node in the CST.
fn is_in_type_node(source: &str, offset: usize) -> bool {
    use rex_core::syntax::SyntaxKind as SK;

    let tokens = rex_core::lexer::lex(source);
    let (green, _errors) = rex_core::parser::parse(source, &tokens);
    let root = rex_core::syntax::SyntaxNode::new_root(green);

    fn is_type_kind(kind: SK) -> bool {
        matches!(kind,
            SK::TypeExpr | SK::TypeArray | SK::TypeObject
            | SK::TypePair | SK::TypeUnion | SK::TypeIntersection
            | SK::TypeGroup
        )
    }

    // Walk the CST tree to find the deepest node containing the offset,
    // checking if any ancestor is a type node.
    fn check(node: &rex_core::syntax::SyntaxNode, offset: usize) -> bool {
        let range = node.text_range();
        let start: usize = range.start().into();
        let end: usize = range.end().into();
        if offset < start || offset >= end { return false; }

        if is_type_kind(node.kind()) {
            return true;
        }

        for child in node.children() {
            if check(&child, offset) {
                return true;
            }
        }
        false
    }

    check(&root, offset)
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Look up a parameter name in a .rexd function declaration.
/// Searches all function signatures for a parameter matching `word`.
fn hover_rexd_param(
    schema: &DomainSchema,
    source: &str,
    pos: lsp_types::Position,
    word: &str,
) -> Option<lsp_types::Hover> {
    // Get the current line to find the function name
    let line_text = source.lines().nth(pos.line as usize)?;

    // Look for "extern name.method(" or "extern name(" pattern
    let extern_prefix = line_text.trim_start().strip_prefix("extern ")?;
    let paren_pos = extern_prefix.find('(')?;
    let func_name_part = extern_prefix[..paren_pos].trim();
    // Strip "mut " prefix if present
    let func_name = func_name_part.strip_prefix("mut ").unwrap_or(func_name_part);

    // Look up the function in the schema
    let sig = schema.functions.get(func_name)?;

    // Find the parameter matching the word
    for (param_name, param_type) in &sig.args {
        if param_name == word {
            let type_str = format_type(param_type);
            let content = lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: format!("```rex\n{word}: {type_str}\n```\n\n---\n\nParameter of `{func_name}`"),
            });
            return Some(lsp_types::Hover { contents: content, range: None });
        }
    }

    // Check rest parameter
    if let Some((rest_name, rest_type)) = &sig.rest {
        if rest_name == word {
            let type_str = format_type(rest_type);
            let content = lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                kind: lsp_types::MarkupKind::Markdown,
                value: format!("```rex\n{word}: {type_str}\n```\n\n---\n\nRest parameter of `{func_name}`"),
            });
            return Some(lsp_types::Hover { contents: content, range: None });
        }
    }

    None
}

fn cast_request<R: lsp_types::request::Request>(req: Request) -> (RequestId, R::Params) {
    let (id, params) = req.extract::<R::Params>(R::METHOD).unwrap();
    (id, params)
}

fn send_response(conn: &Connection, id: RequestId, result: serde_json::Value) {
    let resp = Response::new_ok(id, result);
    let _ = conn.sender.send(Message::Response(resp));
}

/// Advance a byte index past the current character to the next char boundary.
fn next_char_boundary(s: &str, i: usize) -> usize {
    let mut j = i + 1;
    while j < s.len() && !s.is_char_boundary(j) {
        j += 1;
    }
    j
}

/// Extract the word (with dots) immediately before the cursor for completions.
fn extract_word_before(source: &str, pos: lsp_types::Position) -> String {
    let offset = position_to_offset(source, pos);
    let before = &source[..offset];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.')
        .map(|i| next_char_boundary(source, i))
        .unwrap_or(0);
    before[start..].to_string()
}

/// Extract the word at the cursor position (no dots).
fn extract_word_at(source: &str, pos: lsp_types::Position) -> String {
    let offset = position_to_offset(source, pos);
    let start = source[..offset]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|i| next_char_boundary(source, i))
        .unwrap_or(0);
    let end = source[offset..]
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .map(|i| i + offset)
        .unwrap_or(source.len());
    source[start..end].to_string()
}

/// Extract a dotted identifier at the cursor (e.g., "json.parse").
fn extract_dotted_word_at(source: &str, pos: lsp_types::Position) -> String {
    let offset = position_to_offset(source, pos);
    let start = source[..offset]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.')
        .map(|i| next_char_boundary(source, i))
        .unwrap_or(0);
    let end = source[offset..]
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.')
        .map(|i| i + offset)
        .unwrap_or(source.len());
    source[start..end].to_string()
}

fn offset_to_position(source: &str, offset: usize) -> lsp_types::Position {
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
    lsp_types::Position::new(line, col)
}

fn position_to_offset(source: &str, pos: lsp_types::Position) -> usize {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if line == pos.line && col == pos.character {
            return i;
        }
        if ch == '\n' {
            if line == pos.line {
                return i;
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    source.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use lsp_types::{
        GotoDefinitionParams, HoverContents, HoverParams, MarkupKind, Position,
        RenameParams,
        TextDocumentIdentifier, TextDocumentPositionParams,
    };
    use rex_core::typecheck::Type;

    fn setup_state_with_doc(source: &str) -> (LspState, Uri) {
        let mut state = LspState::new(None);
        let uri: Uri = "file:///test.rex".parse().expect("valid test uri");
        state.documents.open(uri.clone(), source.to_string());
        let (_diags, span_types, inline_fns, inline_aliases) =
            diagnostics::compute_diagnostics_with_types(source, &state.schema);
        state.span_types.insert(uri.clone(), span_types);
        state.inline_functions.insert(uri.clone(), inline_fns);
        state.inline_aliases.insert(uri.clone(), inline_aliases);
        (state, uri)
    }
    #[test]
    fn hover_prefers_nested_aliases_for_top_level_structural_type() {
        let source = r#"type Person = { name: str color: int }
db: { bob: Person tim: Person } = {
  bob:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}
db"#;
        let (state, uri) = setup_state_with_doc(source);
        let offset = source.rfind("db").expect("expected trailing db");
        let text = hover_markdown(&state, &uri, source, offset);

        assert!(
            text.contains("db: {bob: Person, tim: Person}"),
            "unexpected hover text: {text}"
        );
        assert!(
            !text.contains("db: {bob: {name: str, color: int}, tim: {name: str, color: int}}"),
            "unexpected hover text: {text}"
        );
    }

    #[test]
    fn hover_db_segment_in_navigation_chain_has_tooltip() {
        let source = r#"type Person = { name: str color: int }
db: { bob: Person tim: Person } = {
  bob:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}
tim-color = db.tim.color"#;
        let (state, uri) = setup_state_with_doc(source);
        let offset = source.find("db.tim.color").expect("expected navigation chain");
        let text = hover_markdown(&state, &uri, source, offset);

        assert!(
            text.contains("db: {bob: Person, tim: Person}"),
            "unexpected hover text: {text}"
        );
    }

    #[test]
    fn hover_tim_segment_in_navigation_chain_has_tooltip() {
        let source = r#"type Person = { name: str color: int }
db: { bob: Person tim: Person } = {
  bob:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}
tim-color = db.tim.color"#;
        let (state, uri) = setup_state_with_doc(source);
        let offset = source.find("db.tim.color").expect("expected navigation chain") + 3;
        let text = hover_markdown(&state, &uri, source, offset);

        assert!(
            text.contains("db.tim: {name: str, color: int}"),
            "unexpected hover text: {text}"
        );
    }

    #[test]
    fn hover_color_segment_in_navigation_chain_still_has_final_type() {
        let source = r#"type Person = { name: str color: int }
db: { bob: Person tim: Person } = {
  bob:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}
tim-color = db.tim.color"#;
        let (state, uri) = setup_state_with_doc(source);
        let offset = source.find("db.tim.color").expect("expected navigation chain") + 7;
        let text = hover_markdown(&state, &uri, source, offset);

        assert!(text.contains("db.tim.color: int"), "unexpected hover text: {text}");
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

    fn hover_markdown(state: &LspState, uri: &Uri, source: &str, offset: usize) -> String {
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: offset_to_position(source, offset),
            },
            work_done_progress_params: Default::default(),
        };

        let hover = handle_hover(state, params).expect("hover should exist");
        match hover.contents {
            HoverContents::Markup(content) if content.kind == MarkupKind::Markdown => content.value,
            other => panic!("unexpected hover payload: {other:?}"),
        }
    }

    fn definition_location(
        state: &LspState,
        uri: &Uri,
        source: &str,
        offset: usize,
    ) -> lsp_types::Location {
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: offset_to_position(source, offset),
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };

        let def = handle_definition(state, params).expect("definition should exist");
        match def {
            lsp_types::GotoDefinitionResponse::Scalar(loc) => loc,
            other => panic!("unexpected definition payload: {other:?}"),
        }
    }

    fn location_text(source: &str, loc: &lsp_types::Location) -> String {
        let start = position_to_offset(source, loc.range.start);
        let end = position_to_offset(source, loc.range.end);
        source[start..end].to_string()
    }

    fn rename_ranges(
        state: &LspState,
        uri: &Uri,
        source: &str,
        offset: usize,
        new_name: &str,
    ) -> Vec<lsp_types::Range> {
        let params = RenameParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: uri.clone() },
                position: offset_to_position(source, offset),
            },
            new_name: new_name.to_string(),
            work_done_progress_params: Default::default(),
        };

        let edit = handle_rename(state, params).expect("rename should exist");
        let mut ranges = edit
            .changes
            .expect("changes should be present")
            .get(uri)
            .expect("uri edits should exist")
            .iter()
            .map(|e| e.range)
            .collect::<Vec<_>>();
        ranges.sort_by_key(|r| (r.start.line, r.start.character, r.end.line, r.end.character));
        ranges
    }

    #[test]
    fn hover_while_condition_variable_stays_int() {
        let source = r#"max = 100
a = 1
b = 1
while a <= max do
  c = a + b
  a = b
  b = c
end"#;
        let (state, uri) = setup_state_with_doc(source);
        let offset = source.find("a <= max").expect("expected condition text");
        let text = hover_markdown(&state, &uri, source, offset);

        assert!(text.contains("a: int"), "unexpected hover text: {text}");
        assert!(!text.contains("a: some | none"), "unexpected hover text: {text}");
    }

    #[test]
    fn hover_inline_var_infers_int() {
        let source = "x = 41\ny = x + 1\ny";
        let (state, uri) = setup_state_with_doc(source);
        let offset = source.find("x + 1").expect("expected expression text");
        let text = hover_markdown(&state, &uri, source, offset);

        assert!(text.contains("x: int"), "unexpected hover text: {text}");
    }

    #[test]
    fn format_type_with_aliases_keeps_top_level_structural() {
        let person = Type::Object {
            fields: vec![
                ("name".to_string(), Type::Str),
                ("color".to_string(), Type::Int),
            ],
            wildcard: None,
        };
        let root = Type::Object {
            fields: vec![
                ("bob".to_string(), person.clone()),
                ("tim".to_string(), person.clone()),
            ],
            wildcard: None,
        };
        let mut aliases = HashMap::new();
        aliases.insert("Person".to_string(), person);

        let got = format_type_with_aliases(&root, &aliases);
        assert_eq!(got, "{bob: Person, tim: Person}");
    }

    #[test]
    fn format_type_with_aliases_uses_unique_nested_match_only() {
        let person = Type::Object {
            fields: vec![
                ("name".to_string(), Type::Str),
                ("color".to_string(), Type::Int),
            ],
            wildcard: None,
        };
        let mut aliases = HashMap::new();
        aliases.insert("Person".to_string(), person.clone());
        aliases.insert("User".to_string(), person.clone());

        let root = Type::Object {
            fields: vec![("bob".to_string(), person)],
            wildcard: None,
        };

        let got = format_type_with_aliases(&root, &aliases);
        assert_eq!(got, "{bob: {name: str, color: int}}");
    }

    #[test]
    fn format_type_with_aliases_substitutes_in_nested_unions() {
        let person = Type::Object {
            fields: vec![
                ("name".to_string(), Type::Str),
                ("color".to_string(), Type::Int),
            ],
            wildcard: None,
        };
        let ty = Type::Union(vec![person.clone(), Type::None]);
        let mut aliases = HashMap::new();
        aliases.insert("Person".to_string(), person);

        let got = format_type_with_aliases(&ty, &aliases);
        assert_eq!(got, "Person | none");
    }

    #[test]
    fn goto_definition_resolves_inline_type_alias_usage() {
        let source = r#"type Person = { name: str color: int }
db: { bob: Person tim: Person } = { bob:{ name:"Bob" color:1 } tim:{ name:"Tim" color:2 } }
v = db.bob"#;
        let (state, uri) = setup_state_with_doc(source);
        let usage_offset = source.find("bob: Person").expect("alias usage should exist") + "bob: ".len();
        let loc = definition_location(&state, &uri, source, usage_offset);

        assert_eq!(loc.uri, uri);
        assert_eq!(loc.range.start.line, 0);
        assert_eq!(loc.range.start.character, 5);
    }

    #[test]
    fn goto_definition_resolves_inline_type_alias_usage_in_value_annotation() {
        let source = r#"type Person = { name: str color: int }
x: Person = { name:"Ada" color:7 }
x"#;
        let (state, uri) = setup_state_with_doc(source);
        let usage_offset = source.find("x: Person").expect("alias usage should exist") + "x: ".len();
        let loc = definition_location(&state, &uri, source, usage_offset);

        assert_eq!(loc.uri, uri);
        assert_eq!(loc.range.start.line, 0);
        assert_eq!(loc.range.start.character, 5);
    }

    #[test]
    fn goto_definition_resolves_local_variable_usage() {
        let source = "x = 41\ny = x + 1\ny";
        let (state, uri) = setup_state_with_doc(source);
        let usage_offset = source.find("x + 1").expect("usage should exist");
        let loc = definition_location(&state, &uri, source, usage_offset);

        assert_eq!(loc.uri, uri);
        assert_eq!(location_text(source, &loc), "x");
        assert_eq!(loc.range.start.line, 0);
        assert_eq!(loc.range.start.character, 0);
    }

    #[test]
    fn goto_definition_resolves_static_navigation_base_variable() {
        let source = r#"db = {
  bob:{ name:"Bob" color:1 }
  tim:{ name:"Tim" color:2 }
}
v = db.tim.color"#;
        let (state, uri) = setup_state_with_doc(source);
        let usage_offset = source.find("db.tim.color").expect("nav should exist");
        let loc = definition_location(&state, &uri, source, usage_offset);

        assert_eq!(loc.uri, uri);
        assert_eq!(location_text(source, &loc), "db");
        assert_eq!(loc.range.start.line, 0);
        assert_eq!(loc.range.start.character, 0);
    }

    #[test]
    fn goto_definition_resolves_static_navigation_property_segments() {
        let source = r#"db = {
  bob:{ name:"Bob" color:1 }
  tim:{ name:"Tim" color:2 }
}
v = db.tim.color"#;
        let (state, uri) = setup_state_with_doc(source);

        let tim_offset = source.find("db.tim.color").expect("nav should exist") + 3;
        let tim_loc = definition_location(&state, &uri, source, tim_offset);
        assert_eq!(tim_loc.uri, uri);
        assert_eq!(location_text(source, &tim_loc), "tim");
        assert_eq!(tim_loc.range.start.line, 2);
        assert_eq!(tim_loc.range.start.character, 2);

        let color_offset = source.find("db.tim.color").expect("nav should exist") + 7;
        let color_loc = definition_location(&state, &uri, source, color_offset);
        assert_eq!(color_loc.uri, uri);
        assert_eq!(location_text(source, &color_loc), "color");
        assert_eq!(color_loc.range.start.line, 2);
        assert_eq!(color_loc.range.start.character, 19);
    }

    #[test]
    fn rename_local_variable_updates_definition_and_usages() {
        let source = "x = 41\ny = x + 1\nx";
        let (state, uri) = setup_state_with_doc(source);
        let offset = source.find("x + 1").expect("usage should exist");
        let ranges = rename_ranges(&state, &uri, source, offset, "value");

        assert_eq!(ranges.len(), 3);
    }

    #[test]
    fn rename_static_navigation_property_updates_definition_and_accesses() {
        let source = r#"db = {
  bob:{ name:"Bob" color:1 }
  tim:{ name:"Tim" color:2 }
}
v = db.tim.color
w = db.bob.color"#;
        let (state, uri) = setup_state_with_doc(source);
        let offset = source.find("db.tim.color").expect("nav should exist") + 7;
        let ranges = rename_ranges(&state, &uri, source, offset, "hue");

        assert_eq!(ranges.len(), 2);
    }

    #[test]
    fn rename_type_object_key_does_not_fail() {
        let source = r#"type Person = { name: str color: int }

db: { bob: Person tim: Person } = {
  boba:{ name:"Bob" color:0x44ff44 }
  tim:{ name:"Tim" color:0x0088ff }
}

first-name = db.boba.name"#;
        let (state, uri) = setup_state_with_doc(source);
        let offset = source.find("bob: Person").expect("type key should exist");
        let ranges = rename_ranges(&state, &uri, source, offset, "boba");

        assert!(!ranges.is_empty());
    }

        #[test]
        fn hover_fibonacci_while_condition_a_is_int() {
                let source = r#"// Calculate the fibonacci numbers up to max (default 100)
// rex run examples/fibonacci.rex
// rex run examples/fibonacci.rex max=200
extern max: int | none
max = max or 100

// Declare an external function to print the results
extern "P" print(val: some) -> some

// Imperative: build with push
fibs = []
a = 1
b = 1
while a <= max do
    fibs.push(a)
    c = a + b
    a = b
    b = c
end
fibs

print(fibs)

// Functional: while comprehension
a = 1; b = 1
fibs2 = [ v = a; c = a + b; a = b; b = c; v while a <= max ]

print(fibs2)

// Verify both methods give the same result
when fibs == fibs2 do
    "fibs and fibs2 are the same"
else
    "fibs and fibs2 are different"
end"#;
                let (state, uri) = setup_state_with_doc(source);
                let offset = source.find("while a <= max do").expect("expected while condition") + "while ".len();
                let text = hover_markdown(&state, &uri, source, offset);

                assert!(text.contains("a: int"), "unexpected hover text: {text}");
                assert!(!text.contains("a: int | none"), "unexpected hover text: {text}");
                assert!(!text.contains("a: some | none"), "unexpected hover text: {text}");
        }
}
