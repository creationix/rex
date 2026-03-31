use rex_core::interpret::{RexError, RexValue};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::refs::rex_value_to_string;

/// Build the opcodes map for a handler invocation.
pub fn build_opcodes(
    db: Arc<Mutex<rusqlite::Connection>>,
    project_root: PathBuf,
) -> HashMap<String, fn(&[RexValue]) -> Result<RexValue, RexError>> {
    DB_CONN.with(|cell| { *cell.borrow_mut() = Some(db); });
    let canonical = project_root.canonicalize().unwrap_or_else(|_| project_root.clone());
    PROJECT_ROOT.with(|cell| { *cell.borrow_mut() = Some(project_root); });
    PROJECT_ROOT_CANONICAL.with(|cell| { *cell.borrow_mut() = Some(canonical); });

    let mut opcodes = HashMap::new();

    // JSON
    opcodes.insert("jp".to_string(), op_json_parse as fn(&[RexValue]) -> Result<RexValue, RexError>);
    opcodes.insert("js".to_string(), op_json_stringify as fn(&[RexValue]) -> _);

    // Logging
    opcodes.insert("li".to_string(), op_log_info as fn(&[RexValue]) -> _);
    opcodes.insert("lw".to_string(), op_log_warning as fn(&[RexValue]) -> _);
    opcodes.insert("le".to_string(), op_log_error as fn(&[RexValue]) -> _);

    // Database
    opcodes.insert("dg".to_string(), op_db_get as fn(&[RexValue]) -> _);
    opcodes.insert("ds".to_string(), op_db_set as fn(&[RexValue]) -> _);
    opcodes.insert("dd".to_string(), op_db_delete as fn(&[RexValue]) -> _);
    opcodes.insert("dl".to_string(), op_db_list as fn(&[RexValue]) -> _);

    // Filesystem
    opcodes.insert("fr".to_string(), op_fs_read as fn(&[RexValue]) -> _);
    opcodes.insert("fg".to_string(), op_fs_glob as fn(&[RexValue]) -> _);
    opcodes.insert("fm".to_string(), op_fs_meta as fn(&[RexValue]) -> _);

    // Content transformation
    opcodes.insert("mr".to_string(), op_markdown_render as fn(&[RexValue]) -> _);
    opcodes.insert("tr".to_string(), op_template_render as fn(&[RexValue]) -> _);

    // Time
    opcodes.insert("tn".to_string(), op_time_now as fn(&[RexValue]) -> _);
    opcodes.insert("tu".to_string(), op_time_uuid as fn(&[RexValue]) -> _);

    // Crypto
    opcodes.insert("ch".to_string(), op_crypto_hash as fn(&[RexValue]) -> _);
    opcodes.insert("cm".to_string(), op_crypto_hmac as fn(&[RexValue]) -> _);
    opcodes.insert("cr".to_string(), op_crypto_random as fn(&[RexValue]) -> _);

    // Text
    opcodes.insert("he".to_string(), op_html_escape as fn(&[RexValue]) -> _);
    opcodes.insert("hl".to_string(), op_highlight_rex as fn(&[RexValue]) -> _);
    opcodes.insert("hh".to_string(), op_highlight_html as fn(&[RexValue]) -> _);
    opcodes.insert("ht".to_string(), op_html_tag as fn(&[RexValue]) -> _);
    opcodes.insert("hr".to_string(), op_html_raw as fn(&[RexValue]) -> _);

    opcodes
}

thread_local! {
    static DB_CONN: std::cell::RefCell<Option<Arc<Mutex<rusqlite::Connection>>>> =
        const { std::cell::RefCell::new(None) };
    static PROJECT_ROOT: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
    /// Pre-canonicalized project root — computed once, reused for every sandbox check.
    static PROJECT_ROOT_CANONICAL: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

fn with_db<F, R>(f: F) -> Result<R, RexError>
where F: FnOnce(&rusqlite::Connection) -> Result<R, RexError> {
    DB_CONN.with(|cell| {
        let borrow = cell.borrow();
        let db = borrow.as_ref().ok_or_else(|| RexError::HostError("no database".into()))?;
        let conn = db.lock().map_err(|e| RexError::HostError(format!("db lock: {e}")))?;
        f(&conn)
    })
}

fn with_root<F, R>(f: F) -> Result<R, RexError>
where F: FnOnce(&Path) -> Result<R, RexError> {
    PROJECT_ROOT.with(|cell| {
        let borrow = cell.borrow();
        let root = borrow.as_ref().ok_or_else(|| RexError::HostError("no project root".into()))?;
        f(root)
    })
}

fn arg_str(args: &[RexValue], idx: usize) -> Result<&str, RexError> {
    args.get(idx)
        .and_then(|v| v.as_str())
        .ok_or_else(|| RexError::HostError(format!("expected string argument at position {idx}")))
}

/// Call a registered opcode by name. Used by HostObject::call for tagged templates.
pub fn call_opcode(name: &str, args: &[RexValue]) -> Result<RexValue, RexError> {
    match name {
        "ht" => op_html_tag(args),
        _ => Err(RexError::HostError(format!("unknown tag opcode: {name}"))),
    }
}

// ── JSON ──────────────────────────────────────────────────────────────

fn op_json_parse(args: &[RexValue]) -> Result<RexValue, RexError> {
    let text = arg_str(args, 0)?;
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| RexError::HostError(format!("json.parse: {e}")))?;
    Ok(json_value_to_rex(&v))
}

fn op_json_stringify(args: &[RexValue]) -> Result<RexValue, RexError> {
    let value = args.first().unwrap_or(&RexValue::RexNone);
    let json = crate::refs::rex_value_to_json(value);
    Ok(RexValue::Str(json.to_string()))
}

fn json_value_to_rex(v: &serde_json::Value) -> RexValue {
    match v {
        serde_json::Value::Null => RexValue::Null,
        serde_json::Value::Bool(b) => RexValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { RexValue::Int(i) }
            else { RexValue::Float(n.as_f64().unwrap_or(0.0)) }
        }
        serde_json::Value::String(s) => RexValue::Str(s.clone()),
        serde_json::Value::Array(arr) => RexValue::Array(arr.iter().map(json_value_to_rex).collect()),
        serde_json::Value::Object(map) => {
            RexValue::Object(map.iter().map(|(k, v)| (k.clone(), json_value_to_rex(v))).collect())
        }
    }
}

// ── Logging ───────────────────────────────────────────────────────────

fn op_log_info(args: &[RexValue]) -> Result<RexValue, RexError> {
    let msg = args.first().map(rex_value_to_string).unwrap_or_default();
    tracing::info!("{msg}");
    Ok(RexValue::RexNone)
}

fn op_log_warning(args: &[RexValue]) -> Result<RexValue, RexError> {
    let msg = args.first().map(rex_value_to_string).unwrap_or_default();
    tracing::warn!("{msg}");
    Ok(RexValue::RexNone)
}

fn op_log_error(args: &[RexValue]) -> Result<RexValue, RexError> {
    let msg = args.first().map(rex_value_to_string).unwrap_or_default();
    tracing::error!("{msg}");
    Ok(RexValue::RexNone)
}

// ── Database ──────────────────────────────────────────────────────────

fn op_db_get(args: &[RexValue]) -> Result<RexValue, RexError> {
    let key = arg_str(args, 0)?;
    with_db(|conn| {
        let mut stmt = conn.prepare_cached("SELECT value FROM kv WHERE key = ?1")
            .map_err(|e| RexError::HostError(format!("db.get: {e}")))?;
        let result: Result<String, _> = stmt.query_row([key], |row| row.get(0));
        match result {
            Ok(val) => Ok(RexValue::Str(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(RexValue::RexNone),
            Err(e) => Err(RexError::HostError(format!("db.get: {e}"))),
        }
    })
}

fn op_db_set(args: &[RexValue]) -> Result<RexValue, RexError> {
    let key = arg_str(args, 0)?;
    let value = args.get(1).map(rex_value_to_string).unwrap_or_default();
    with_db(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
            [key, &value],
        ).map_err(|e| RexError::HostError(format!("db.set: {e}")))?;
        Ok(RexValue::Bool(true))
    })
}

fn op_db_delete(args: &[RexValue]) -> Result<RexValue, RexError> {
    let key = arg_str(args, 0)?;
    with_db(|conn| {
        conn.execute("DELETE FROM kv WHERE key = ?1", [key])
            .map_err(|e| RexError::HostError(format!("db.delete: {e}")))?;
        Ok(RexValue::Bool(true))
    })
}

fn op_db_list(args: &[RexValue]) -> Result<RexValue, RexError> {
    let prefix = arg_str(args, 0)?;
    with_db(|conn| {
        let mut stmt = conn.prepare_cached(
            "SELECT key, value FROM kv WHERE key LIKE ?1 ORDER BY key"
        ).map_err(|e| RexError::HostError(format!("db.list: {e}")))?;

        let pattern = format!("{prefix}%");
        let rows: Vec<RexValue> = stmt.query_map([&pattern], |row| {
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok(RexValue::Object(vec![
                ("key".into(), RexValue::Str(key)),
                ("value".into(), RexValue::Str(value)),
            ]))
        })
        .map_err(|e| RexError::HostError(format!("db.list: {e}")))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(RexValue::Array(rows))
    })
}

// ── Filesystem ────────────────────────────────────────────────────────

fn op_fs_read(args: &[RexValue]) -> Result<RexValue, RexError> {
    let path_str = arg_str(args, 0)?;
    with_root(|root| {
        match sandbox_path(root, path_str) {
            Ok(resolved) => {
                match std::fs::read_to_string(&resolved) {
                    Ok(content) => Ok(RexValue::Str(content)),
                    Err(_) => Ok(RexValue::RexNone),
                }
            }
            Err(_) => Ok(RexValue::RexNone), // file not found or traversal denied
        }
    })
}

fn op_fs_glob(args: &[RexValue]) -> Result<RexValue, RexError> {
    let pattern = arg_str(args, 0)?;
    with_root(|root| {
        let full_pattern = root.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();
        let paths: Vec<RexValue> = glob::glob(&pattern_str)
            .unwrap_or_else(|_| glob::glob("__nonexistent__").unwrap())
            .filter_map(|entry| entry.ok())
            .filter_map(|path| {
                path.strip_prefix(root).ok()
                    .map(|rel| RexValue::Str(rel.to_string_lossy().to_string()))
            })
            .collect();
        Ok(RexValue::Array(paths))
    })
}

fn op_fs_meta(args: &[RexValue]) -> Result<RexValue, RexError> {
    let path_str = arg_str(args, 0)?;
    with_root(|root| {
        let resolved = sandbox_path(root, path_str)?;
        match std::fs::metadata(&resolved) {
            Ok(meta) => {
                let modified = meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                Ok(RexValue::Object(vec![
                    ("size".into(), RexValue::Int(meta.len() as i64)),
                    ("modified".into(), RexValue::Int(modified)),
                ]))
            }
            Err(_) => Ok(RexValue::RexNone),
        }
    })
}

fn sandbox_path(root: &Path, user_path: &str) -> Result<PathBuf, RexError> {
    // Use cached canonical root — no syscall needed
    let root_canonical = PROJECT_ROOT_CANONICAL.with(|cell| {
        cell.borrow().clone()
    }).ok_or_else(|| RexError::HostError("no project root".into()))?;

    let resolved = root.join(user_path);
    match resolved.canonicalize() {
        Ok(canonical) => {
            if !canonical.starts_with(&root_canonical) {
                return Err(RexError::HostError(format!("path traversal denied: {user_path}")));
            }
            Ok(canonical)
        }
        Err(_) => {
            // File doesn't exist — normalize manually for traversal check
            let mut normalized = root_canonical.clone();
            for component in std::path::Path::new(user_path).components() {
                match component {
                    std::path::Component::Normal(c) => normalized.push(c),
                    std::path::Component::ParentDir => { normalized.pop(); }
                    _ => {}
                }
            }
            if !normalized.starts_with(&root_canonical) {
                return Err(RexError::HostError(format!("path traversal denied: {user_path}")));
            }
            Err(RexError::HostError(format!("file not found: {user_path}")))
        }
    }
}

// ── Content Transformation ────────────────────────────────────────────

fn op_markdown_render(args: &[RexValue]) -> Result<RexValue, RexError> {
    use pulldown_cmark::{Parser, Event, Tag, TagEnd, CodeBlockKind};

    let text = arg_str(args, 0)?;
    let parser = Parser::new(text);

    let mut html = String::new();
    let mut in_rex_code_block = false;
    let mut code_buf = String::new();

    // Walk events, intercepting rex code blocks for syntax highlighting
    let events: Vec<Event<'_>> = parser.collect();
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang))) => {
                let lang_str = lang.as_ref().trim();
                if lang_str == "rex" {
                    in_rex_code_block = true;
                    code_buf.clear();
                    html.push_str("<pre><code class=\"language-rex\">");
                } else {
                    // Let default rendering handle non-rex blocks
                    pulldown_cmark::html::push_html(
                        &mut html,
                        std::iter::once(events[i].clone()),
                    );
                }
            }
            Event::End(TagEnd::CodeBlock) if in_rex_code_block => {
                // Highlight the accumulated Rex code
                html.push_str(&highlight_rex_source(&code_buf));
                html.push_str("</code></pre>\n");
                in_rex_code_block = false;
            }
            Event::Text(text) if in_rex_code_block => {
                code_buf.push_str(text.as_ref());
            }
            other => {
                if !in_rex_code_block {
                    pulldown_cmark::html::push_html(
                        &mut html,
                        std::iter::once(other.clone()),
                    );
                }
            }
        }
        i += 1;
    }

    Ok(RexValue::Str(html))
}

fn op_template_render(args: &[RexValue]) -> Result<RexValue, RexError> {
    let template = arg_str(args, 0)?;
    let data = args.get(1).unwrap_or(&RexValue::RexNone);
    let result = render_template(template, data);
    Ok(RexValue::Str(result))
}

fn render_template(template: &str, data: &RexValue) -> String {
    let mut result = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if i + 2 < bytes.len() && bytes[i] == b'{' && bytes[i+1] == b'{' {
            // Check for triple brace (unescaped)
            let unescaped = i + 2 < bytes.len() && bytes[i+2] == b'{';
            let start = if unescaped { i + 3 } else { i + 2 };

            // Find closing braces
            let closing = if unescaped { "}}}" } else { "}}" };
            if let Some(end) = template[start..].find(closing) {
                let key = template[start..start+end].trim();
                let value = lookup_template_key(data, key);
                if unescaped {
                    result.push_str(&value);
                } else {
                    result.push_str(&html_escape(&value));
                }
                i = start + end + closing.len();
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }

    result
}

fn lookup_template_key(data: &RexValue, key: &str) -> String {
    match data {
        RexValue::Object(pairs) => {
            for (k, v) in pairs {
                if k == key {
                    return rex_value_to_string(v);
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn html_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#x27;"),
            _ => result.push(c),
        }
    }
    result
}

// ── Time ──────────────────────────────────────────────────────────────

fn op_time_now(args: &[RexValue]) -> Result<RexValue, RexError> {
    let _ = args;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    Ok(RexValue::Int(now))
}

fn op_time_uuid(args: &[RexValue]) -> Result<RexValue, RexError> {
    let _ = args;
    let id = uuid::Uuid::now_v7();
    Ok(RexValue::Str(id.to_string()))
}

// ── Crypto ────────────────────────────────────────────────────────────

fn op_crypto_hash(args: &[RexValue]) -> Result<RexValue, RexError> {
    let algo = arg_str(args, 0)?;
    let data = arg_str(args, 1)?;
    match algo {
        "sha256" => {
            use sha2::{Sha256, Digest};
            let mut hasher = Sha256::new();
            hasher.update(data.as_bytes());
            Ok(RexValue::Str(hex::encode(hasher.finalize())))
        }
        _ => Err(RexError::HostError(format!("unsupported hash algorithm: {algo}"))),
    }
}

fn op_crypto_hmac(args: &[RexValue]) -> Result<RexValue, RexError> {
    let algo = arg_str(args, 0)?;
    let key = arg_str(args, 1)?;
    let data = arg_str(args, 2)?;
    match algo {
        "sha256" => {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;
            type HmacSha256 = Hmac<Sha256>;
            let mut mac = HmacSha256::new_from_slice(key.as_bytes())
                .map_err(|e| RexError::HostError(format!("hmac: {e}")))?;
            mac.update(data.as_bytes());
            Ok(RexValue::Str(hex::encode(mac.finalize().into_bytes())))
        }
        _ => Err(RexError::HostError(format!("unsupported hmac algorithm: {algo}"))),
    }
}

fn op_crypto_random(args: &[RexValue]) -> Result<RexValue, RexError> {
    let n = args.first()
        .and_then(|v| v.to_i64())
        .unwrap_or(16) as usize;
    use rand::RngCore;
    let mut buf = vec![0u8; n];
    rand::rng().fill_bytes(&mut buf);
    Ok(RexValue::Str(hex::encode(&buf)))
}

// ── Text ──────────────────────────────────────────────────────────────

fn op_html_escape(args: &[RexValue]) -> Result<RexValue, RexError> {
    let text = arg_str(args, 0)?;
    Ok(RexValue::Str(html_escape(text)))
}

/// Tagged template: html`<p>${user_input}</p>`
/// Receives (["<p>", "</p>"], user_input) — auto-escapes interpolated values.
/// Tagged template: html`<p>${user_input}</p>`
/// Receives (["<p>", "</p>"], user_input) — auto-escapes interpolated values.
/// Pass `html.raw(value)` to skip escaping for pre-rendered HTML.
fn op_html_tag(args: &[RexValue]) -> Result<RexValue, RexError> {
    let parts = match args.first() {
        Some(RexValue::Array(parts)) => parts,
        _ => return Err(RexError::HostError("html tag: first argument must be string parts array".into())),
    };
    let values = &args[1..];
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if let RexValue::Str(s) = part {
            out.push_str(s);
        }
        if i < values.len() {
            // Check for {raw: "..."} marker — pass through unescaped
            if let Some(raw_str) = extract_raw(&values[i]) {
                out.push_str(&raw_str);
            } else {
                let val_str = rex_value_to_string(&values[i]);
                out.push_str(&html_escape(&val_str));
            }
        }
    }
    Ok(RexValue::Str(out))
}

/// Check if a value is a {raw: "..."} marker object.
fn extract_raw(value: &RexValue) -> Option<String> {
    if let RexValue::Object(pairs) = value {
        if pairs.len() == 1 && pairs[0].0 == "raw" {
            if let RexValue::Str(s) = &pairs[0].1 {
                return Some(s.clone());
            }
        }
    }
    None
}

/// html.raw(value) — wraps a string as {raw: value} so html`` won't escape it.
fn op_html_raw(args: &[RexValue]) -> Result<RexValue, RexError> {
    let value = args.first().unwrap_or(&RexValue::RexNone);
    let s = rex_value_to_string(value);
    Ok(RexValue::Object(vec![("raw".into(), RexValue::Str(s))]))
}

fn op_highlight_rex(args: &[RexValue]) -> Result<RexValue, RexError> {
    let source = arg_str(args, 0)?;
    Ok(RexValue::Str(highlight_rex_source(source)))
}

fn highlight_rex_source(source: &str) -> String {
    use rex_core::lexer::{self, TokenKind};

    let tokens = lexer::lex(source);
    let mut out = String::with_capacity(source.len() * 2);

    // Helper: find the next non-whitespace token kind after position i
    let next_nonws = |i: usize| -> Option<TokenKind> {
        tokens[i+1..].iter()
            .find(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
    };

    // Helper: find the previous non-whitespace token kind before position i
    let prev_nonws = |i: usize| -> Option<TokenKind> {
        tokens[..i].iter().rev()
            .find(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
    };

    for (i, token) in tokens.iter().enumerate() {
        let text = &source[token.span.clone()];
        let escaped = html_escape(text);
        let class = match token.kind {
            // Keywords
            TokenKind::KwWhen | TokenKind::KwUnless | TokenKind::KwDo
            | TokenKind::KwEnd | TokenKind::KwElse | TokenKind::KwFor
            | TokenKind::KwIn | TokenKind::KwOf | TokenKind::KwWhile
            | TokenKind::KwAnd | TokenKind::KwOr | TokenKind::KwNor
            | TokenKind::KwNot | TokenKind::KwDelete
            | TokenKind::KwBreak | TokenKind::KwContinue
            | TokenKind::KwReturn => Some("kw"),

            // Literals: true/false/null/none/nan/inf
            TokenKind::KwTrue | TokenKind::KwFalse => Some("bl"),
            TokenKind::KwNull | TokenKind::KwNone
            | TokenKind::KwNan | TokenKind::KwInf => Some("ct"),

            // Type predicates used as calls: string(), number(), etc.
            TokenKind::KwString | TokenKind::KwNumber | TokenKind::KwBoolean
            | TokenKind::KwArray | TokenKind::KwObject => Some("ty"),

            // Self
            TokenKind::KwSelf => Some("ct"),

            // Strings
            TokenKind::DoubleString | TokenKind::SingleString => Some("st"),

            // Template literals — highlight internals
            TokenKind::TemplateLiteral => {
                out.push_str(&highlight_template_literal(text));
                continue;
            }

            // Numbers
            TokenKind::DecimalNumber | TokenKind::HexNumber
            | TokenKind::BinaryNumber => Some("nm"),

            // Comments
            TokenKind::LineComment | TokenKind::BlockComment => Some("cm"),

            // Operators
            TokenKind::Plus | TokenKind::Minus | TokenKind::Star
            | TokenKind::Slash | TokenKind::Percent
            | TokenKind::Amp | TokenKind::Pipe | TokenKind::Caret
            | TokenKind::Tilde | TokenKind::Eq | TokenKind::Gt
            | TokenKind::Lt | TokenKind::DotDot
            | TokenKind::ColonEq | TokenKind::EqEq | TokenKind::BangEq
            | TokenKind::GtEq | TokenKind::LtEq
            | TokenKind::PlusEq | TokenKind::MinusEq
            | TokenKind::StarEq | TokenKind::SlashEq | TokenKind::PercentEq
            | TokenKind::AmpEq | TokenKind::PipeEq | TokenKind::CaretEq => Some("op"),

            // Punctuation
            TokenKind::Dot | TokenKind::DotParen | TokenKind::Comma
            | TokenKind::Colon | TokenKind::At
            | TokenKind::LParen | TokenKind::RParen
            | TokenKind::LBracket | TokenKind::RBracket
            | TokenKind::LBrace | TokenKind::RBrace => Some("pn"),

            // Identifiers — contextual coloring
            TokenKind::Ident => {
                if next_nonws(i) == Some(TokenKind::Colon) {
                    // Object key: `slug:` `title:` `body:`
                    Some("ky")
                } else if prev_nonws(i) == Some(TokenKind::Dot) {
                    // Property access after dot: `res.headers`, `db.get`
                    Some("pr")
                } else if next_nonws(i) == Some(TokenKind::LParen) {
                    // Function call: `markdown.render(...)`
                    Some("fn")
                } else {
                    None
                }
            }

            // Declaration keywords
            TokenKind::KwExtern | TokenKind::KwType => Some("kw"),

            // Whitespace, errors
            TokenKind::Whitespace | TokenKind::Error => None,
        };

        match class {
            Some(c) => {
                out.push_str("<span class=\"hl-");
                out.push_str(c);
                out.push_str("\">");
                out.push_str(&escaped);
                out.push_str("</span>");
            }
            None => out.push_str(&escaped),
        }
    }

    out
}

/// Highlight a template literal token, breaking it into string parts,
/// `${`/`}` delimiters, and recursively highlighted interpolated expressions.
fn highlight_template_literal(text: &str) -> String {
    let mut out = String::new();
    let bytes = text.as_bytes();

    // Opening backtick (or tag`  if preceded by identifier — but the lexer
    // emits the tag as a separate Ident token, so text starts with `)
    if bytes.is_empty() { return out; }

    let mut i = 0;

    // Opening backtick
    out.push_str("<span class=\"hl-op\">`</span>");
    i += 1;

    while i < bytes.len() {
        if bytes[i] == b'`' {
            // Closing backtick
            out.push_str("<span class=\"hl-op\">`</span>");
            break;
        } else if bytes[i] == b'\\' && i + 1 < bytes.len() {
            // Escape sequence inside template
            let esc: String = text[i..i+2].to_string();
            out.push_str("<span class=\"hl-st\">");
            out.push_str(&html_escape(&esc));
            out.push_str("</span>");
            i += 2;
        } else if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // Interpolation: ${expr}
            out.push_str("<span class=\"hl-op\">${</span>");
            i += 2;

            // Find matching closing brace, tracking depth
            let expr_start = i;
            let mut depth: u32 = 1;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    b'`' => {
                        // Nested template literal — skip it
                        i += 1;
                        while i < bytes.len() && bytes[i] != b'`' {
                            if bytes[i] == b'\\' { i += 1; }
                            i += 1;
                        }
                    }
                    b'\'' | b'"' => {
                        // Skip string literals inside interpolation
                        let quote = bytes[i];
                        i += 1;
                        while i < bytes.len() && bytes[i] != quote {
                            if bytes[i] == b'\\' { i += 1; }
                            i += 1;
                        }
                    }
                    b'\\' => { i += 1; }
                    _ => {}
                }
                if depth > 0 { i += 1; }
            }

            // The expression is text[expr_start..i] (i is at the closing })
            let expr = &text[expr_start..i];
            if !expr.is_empty() {
                // Recursively highlight the expression as Rex code
                out.push_str(&highlight_rex_source(expr));
            }

            out.push_str("<span class=\"hl-op\">}</span>");
            if depth == 0 { i += 1; } // skip past }
        } else {
            // Static string content — collect until next ${ or ` or \
            let start = i;
            while i < bytes.len()
                && bytes[i] != b'`'
                && bytes[i] != b'\\'
                && !(bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{')
            {
                i += 1;
            }
            let chunk = &text[start..i];
            if !chunk.is_empty() {
                out.push_str("<span class=\"hl-st\">");
                out.push_str(&html_escape(chunk));
                out.push_str("</span>");
            }
        }
    }

    out
}

fn op_highlight_html(args: &[RexValue]) -> Result<RexValue, RexError> {
    let source = arg_str(args, 0)?;
    Ok(RexValue::Str(highlight_html_source(source)))
}

fn highlight_html_source(source: &str) -> String {
    let mut out = String::with_capacity(source.len() * 2);
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // HTML comment: <!-- ... -->
        if i + 3 < chars.len() && chars[i] == '<' && chars[i+1] == '!' && chars[i+2] == '-' && chars[i+3] == '-' {
            let start = i;
            i += 4;
            while i + 2 < chars.len() && !(chars[i] == '-' && chars[i+1] == '-' && chars[i+2] == '>') {
                i += 1;
            }
            if i + 2 < chars.len() { i += 3; }
            let text: String = chars[start..i].iter().collect();
            out.push_str("<span class=\"hl-cm\">");
            out.push_str(&html_escape(&text));
            out.push_str("</span>");
            continue;
        }

        // Tag: < ... >
        if chars[i] == '<' {
            out.push_str("<span class=\"hl-pn\">&lt;</span>");
            i += 1;

            // Closing tag slash
            if i < chars.len() && chars[i] == '/' {
                out.push_str("<span class=\"hl-pn\">/</span>");
                i += 1;
            }

            // Tag name
            let name_start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '-' || chars[i] == '!' || chars[i] == ':') {
                i += 1;
            }
            if i > name_start {
                let name: String = chars[name_start..i].iter().collect();
                out.push_str("<span class=\"hl-kw\">");
                out.push_str(&html_escape(&name));
                out.push_str("</span>");
            }

            // Attributes and rest of tag
            while i < chars.len() && chars[i] != '>' {
                // Whitespace
                if chars[i].is_whitespace() {
                    out.push(chars[i]);
                    i += 1;
                    continue;
                }

                // Self-closing slash
                if chars[i] == '/' {
                    out.push_str("<span class=\"hl-pn\">/</span>");
                    i += 1;
                    continue;
                }

                // Attribute value (quoted)
                if chars[i] == '"' || chars[i] == '\'' {
                    let quote = chars[i];
                    let val_start = i;
                    i += 1;
                    while i < chars.len() && chars[i] != quote {
                        i += 1;
                    }
                    if i < chars.len() { i += 1; }
                    let val: String = chars[val_start..i].iter().collect();
                    out.push_str("<span class=\"hl-st\">");
                    out.push_str(&html_escape(&val));
                    out.push_str("</span>");
                    continue;
                }

                // = sign
                if chars[i] == '=' {
                    out.push_str("<span class=\"hl-op\">=</span>");
                    i += 1;
                    continue;
                }

                // Attribute name
                let attr_start = i;
                while i < chars.len() && chars[i] != '=' && chars[i] != '>' && chars[i] != '/' && !chars[i].is_whitespace() {
                    i += 1;
                }
                if i > attr_start {
                    let attr: String = chars[attr_start..i].iter().collect();
                    out.push_str("<span class=\"hl-ty\">");
                    out.push_str(&html_escape(&attr));
                    out.push_str("</span>");
                }
            }

            // Closing >
            if i < chars.len() && chars[i] == '>' {
                out.push_str("<span class=\"hl-pn\">&gt;</span>");
                i += 1;
            }
            continue;
        }

        // Mustache: {{ or {{{
        if i + 1 < chars.len() && chars[i] == '{' && chars[i+1] == '{' {
            let triple = i + 2 < chars.len() && chars[i+2] == '{';
            let open = if triple { "{{{" } else { "{{" };
            let close = if triple { "}}}" } else { "}}" };
            i += open.len();
            let key_start = i;
            while i + close.len() <= chars.len() {
                let window: String = chars[i..i+close.len()].iter().collect();
                if window == close { break; }
                i += 1;
            }
            let key: String = chars[key_start..i].iter().collect();
            i += close.len();
            out.push_str("<span class=\"hl-op\">");
            out.push_str(&html_escape(open));
            out.push_str("</span>");
            out.push_str("<span class=\"hl-nm\">");
            out.push_str(&html_escape(key.trim()));
            out.push_str("</span>");
            out.push_str("<span class=\"hl-op\">");
            out.push_str(&html_escape(close));
            out.push_str("</span>");
            continue;
        }

        // Plain text
        let text_start = i;
        while i < chars.len() && chars[i] != '<' && !(i + 1 < chars.len() && chars[i] == '{' && chars[i+1] == '{') {
            i += 1;
        }
        let text: String = chars[text_start..i].iter().collect();
        out.push_str(&html_escape(&text));
    }

    out
}

// ── Database Init ─────────────────────────────────────────────────────

pub fn init_db(path: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path)
        .expect("failed to open database");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL)"
    ).expect("failed to create kv table");
    conn
}
