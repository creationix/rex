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
use lsp_types::request::{Completion, GotoDefinition, HoverRequest, Request as _};
use lsp_types::{
    CompletionOptions, CompletionParams, GotoDefinitionResponse, HoverParams,
    HoverProviderCapability, InitializeParams, PublishDiagnosticsParams, ServerCapabilities,
    TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use rex_core::typecheck::{self, DomainSchema};

use self::document::DocumentStore;

/// Format a Rex Type as a human-readable string.
fn format_type(ty: &typecheck::Type) -> String {
    use typecheck::Type;
    match ty {
        Type::Some => "some".to_string(),
        Type::None => "none".to_string(),
        Type::Never => "never".to_string(),
        Type::Null => "null".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Int => "int".to_string(),
        Type::Number => "number".to_string(),
        Type::Str => "str".to_string(),
        Type::LiteralStr(s) => format!("\"{s}\""),
        Type::Array(elem) => format!("[{}]", format_type(elem)),
        Type::Object { fields, wildcard } => {
            let mut parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", format_type(v)))
                .collect();
            if let Some(w) = wildcard {
                parts.push(format!("*: {}", format_type(w)));
            }
            format!("{{{}}}", parts.join(", "))
        }
        Type::Union(types) => {
            let parts: Vec<String> = types.iter().map(format_type).collect();
            parts.join(" | ")
        }
        Type::Intersection(types) => {
            let parts: Vec<String> = types.iter().map(format_type).collect();
            parts.join(" & ")
        }
        Type::Ref(name) => name.clone(),
    }
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
        let (diags, types) = diagnostics::compute_diagnostics_with_types(source, &self.schema);
        self.span_types.insert(uri.clone(), types);
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

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_string()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(lsp_types::OneOf::Left(true)),
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

    // Try dotted word first (e.g., "json.parse") — domain schema lookup
    let dot_word = extract_dotted_word_at(source, pos);
    if !dot_word.is_empty() && dot_word != word {
        if let result @ Some(_) = hover::hover(&state.schema, &dot_word) {
            return result;
        }
    }

    // Try domain schema lookup for the simple word
    if let result @ Some(_) = hover::hover(&state.schema, &word) {
        return result;
    }

    // Fall back to inferred type from span→type map
    if let Some(span_types) = state.span_types.get(uri) {
        let offset = position_to_offset(source, pos);
        // Find the smallest span containing the cursor
        let mut best: Option<&(std::ops::Range<usize>, typecheck::Type)> = None;
        for entry in span_types {
            if entry.0.contains(&offset) {
                if let Some(prev) = best {
                    if entry.0.len() < prev.0.len() {
                        best = Some(entry);
                    }
                } else {
                    best = Some(entry);
                }
            }
        }
        if let Some((_, ty)) = best {
            let type_str = format_type(ty);
            if type_str != "some" && type_str != "none" {
                let content = lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: format!("```rex\n{word}: {type_str}\n```"),
                });
                return Some(lsp_types::Hover { contents: content, range: None });
            }
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

    // Try dotted word first
    let dot_word = extract_dotted_word_at(source, pos);
    if !dot_word.is_empty() {
        if let Some(loc) = definition::definition(
            &state.schema,
            &dot_word,
            state.rexd_uri.as_ref(),
            state.rexd_source.as_deref(),
        ) {
            return Some(GotoDefinitionResponse::Scalar(loc));
        }
    }

    let loc = definition::definition(
        &state.schema,
        &word,
        state.rexd_uri.as_ref(),
        state.rexd_source.as_deref(),
    )?;
    Some(GotoDefinitionResponse::Scalar(loc))
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn cast_request<R: lsp_types::request::Request>(req: Request) -> (RequestId, R::Params) {
    let (id, params) = req.extract::<R::Params>(R::METHOD).unwrap();
    (id, params)
}

fn send_response(conn: &Connection, id: RequestId, result: serde_json::Value) {
    let resp = Response::new_ok(id, result);
    let _ = conn.sender.send(Message::Response(resp));
}

/// Extract the word (with dots) immediately before the cursor for completions.
fn extract_word_before(source: &str, pos: lsp_types::Position) -> String {
    let offset = position_to_offset(source, pos);
    let before = &source[..offset];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].to_string()
}

/// Extract the word at the cursor position (no dots).
fn extract_word_at(source: &str, pos: lsp_types::Position) -> String {
    let offset = position_to_offset(source, pos);
    let start = source[..offset]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = source[offset..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + offset)
        .unwrap_or(source.len());
    source[start..end].to_string()
}

/// Extract a dotted identifier at the cursor (e.g., "json.parse").
fn extract_dotted_word_at(source: &str, pos: lsp_types::Position) -> String {
    let offset = position_to_offset(source, pos);
    let start = source[..offset]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = source[offset..]
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + offset)
        .unwrap_or(source.len());
    source[start..end].to_string()
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
