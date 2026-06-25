use rex_core::heap::{Value, Heap};
use rex_core::interpret::RexError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::refs::value_to_string;

/// Build the opcodes map for a handler invocation.
pub fn build_opcodes(
    db: Arc<Mutex<rusqlite::Connection>>,
    upstash: Option<Arc<crate::storage::UpstashClient>>,
    project_root: PathBuf,
    kv: Arc<std::sync::Mutex<crate::kv::KvStore>>,
) -> HashMap<String, fn(&[Value], &mut Heap) -> Result<Value, RexError>> {
    DB_CONN.with(|cell| { *cell.borrow_mut() = Some(db); });
    UPSTASH.with(|cell| { *cell.borrow_mut() = upstash; });
    let canonical = project_root.canonicalize().unwrap_or_else(|_| project_root.clone());
    PROJECT_ROOT.with(|cell| { *cell.borrow_mut() = Some(project_root); });
    PROJECT_ROOT_CANONICAL.with(|cell| { *cell.borrow_mut() = Some(canonical); });
    KV_STORE.with(|cell| { *cell.borrow_mut() = Some(kv); });

    let mut opcodes: HashMap<String, fn(&[Value], &mut Heap) -> Result<Value, RexError>> = HashMap::new();

    // JSON
    opcodes.insert("jp".into(), op_json_parse);
    opcodes.insert("js".into(), op_json_stringify);

    // Logging
    opcodes.insert("li".into(), op_log_info);
    opcodes.insert("lw".into(), op_log_warning);
    opcodes.insert("le".into(), op_log_error);

    // Database
    opcodes.insert("dg".into(), op_db_get);
    opcodes.insert("ds".into(), op_db_set);
    opcodes.insert("dd".into(), op_db_delete);
    opcodes.insert("dl".into(), op_db_list);

    // Filesystem
    opcodes.insert("fr".into(), op_fs_read);
    opcodes.insert("fg".into(), op_fs_glob);
    opcodes.insert("fm".into(), op_fs_meta);

    // Content transformation
    opcodes.insert("mr".into(), op_markdown_render);
    opcodes.insert("tr".into(), op_template_render);

    // Time
    opcodes.insert("tn".into(), op_time_now);
    opcodes.insert("tu".into(), op_time_uuid);

    // Crypto
    opcodes.insert("ch".into(), op_crypto_hash);
    opcodes.insert("cm".into(), op_crypto_hmac);
    opcodes.insert("cr".into(), op_crypto_random);

    // KV store
    opcodes.insert("kg".into(), op_kv_get);
    opcodes.insert("ks".into(), op_kv_set);
    opcodes.insert("kd".into(), op_kv_delete);
    opcodes.insert("kk".into(), op_kv_keys);
    opcodes.insert("ki".into(), op_kv_incr);
    opcodes.insert("kp".into(), op_kv_publish);

    // Text
    opcodes.insert("he".into(), op_html_escape);
    opcodes.insert("hl".into(), op_highlight_rex);
    opcodes.insert("hh".into(), op_highlight_html);
    opcodes.insert("ht".into(), op_html_tag);
    opcodes.insert("hr".into(), op_html_raw);

    // Database: compare-and-swap
    opcodes.insert("dc".into(), op_db_cas);

    // Content-addressable store
    opcodes.insert("cp".into(), op_cas_put);
    opcodes.insert("cg".into(), op_cas_get);
    opcodes.insert("cx".into(), op_cas_has);

    // Git objects
    opcodes.insert("gd".into(), op_git_decode);
    opcodes.insert("gc".into(), op_git_children);
    opcodes.insert("gv".into(), op_git_verify);
    opcodes.insert("ga".into(), op_git_is_ancestor);
    opcodes.insert("ge".into(), op_git_encode);
    opcodes.insert("gB".into(), op_git_encode_blob);

    // HTTP
    opcodes.insert("hf".into(), op_http_fetch);

    opcodes
}

thread_local! {
    static DB_CONN: std::cell::RefCell<Option<Arc<Mutex<rusqlite::Connection>>>> =
        const { std::cell::RefCell::new(None) };
    static UPSTASH: std::cell::RefCell<Option<Arc<crate::storage::UpstashClient>>> =
        const { std::cell::RefCell::new(None) };
    static PROJECT_ROOT: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
    static PROJECT_ROOT_CANONICAL: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
    static KV_STORE: std::cell::RefCell<Option<Arc<std::sync::Mutex<crate::kv::KvStore>>>> =
        const { std::cell::RefCell::new(None) };
}

fn upstash() -> Option<Arc<crate::storage::UpstashClient>> {
    UPSTASH.with(|cell| cell.borrow().clone())
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

fn arg_str<'a>(args: &'a [Value], idx: usize, heap: &'a Heap) -> Result<&'a str, RexError> {
    args.get(idx)
        .and_then(|v| v.as_str(heap))
        .ok_or_else(|| RexError::HostError(format!("expected string argument at position {idx}")))
}

/// Call a registered opcode by name. Used by HostObject::call for tagged templates.
pub fn call_opcode(name: &str, args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    match name {
        "ht" => op_html_tag(args, heap),
        _ => Err(RexError::HostError(format!("unknown tag opcode: {name}"))),
    }
}

// ── JSON ──────────────────────────────────────────────────────────────

fn op_json_parse(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let text = arg_str(args, 0, heap)?;
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| RexError::HostError(format!("json.parse: {e}")))?;
    Ok(crate::refs::json_to_value(&v, heap))
}

fn op_json_stringify(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let value = args.first().copied().unwrap_or(Value::NONE);
    let json = crate::refs::value_to_json(value, heap);
    Ok(heap.intern_value(&json.to_string()))
}

// ── Logging ───────────────────────────────────────────────────────────

fn op_log_info(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let msg = args.first().map(|v| value_to_string(*v, heap)).unwrap_or_default();
    tracing::info!("{msg}");
    Ok(Value::NONE)
}

fn op_log_warning(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let msg = args.first().map(|v| value_to_string(*v, heap)).unwrap_or_default();
    tracing::warn!("{msg}");
    Ok(Value::NONE)
}

fn op_log_error(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let msg = args.first().map(|v| value_to_string(*v, heap)).unwrap_or_default();
    tracing::error!("{msg}");
    Ok(Value::NONE)
}

// ── Database ──────────────────────────────────────────────────────────

fn op_db_get(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let key = arg_str(args, 0, heap)?.to_string();
    let value = if let Some(upstash) = upstash() {
        upstash.get(&key)
            .map_err(|e| RexError::HostError(format!("db.get: {e}")))?
    } else {
        with_db(|conn| {
            let mut stmt = conn.prepare_cached("SELECT value FROM kv WHERE key = ?1")
                .map_err(|e| RexError::HostError(format!("db.get: {e}")))?;
            let result: Result<String, _> = stmt.query_row([&key], |row| row.get(0));
            match result {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(RexError::HostError(format!("db.get: {e}"))),
            }
        })?
    };

    Ok(value.map_or(Value::NONE, |value| heap.intern_value(&value)))
}

fn op_db_set(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let key = arg_str(args, 0, heap)?.to_string();
    let value = args.get(1).map(|v| value_to_string(*v, heap)).unwrap_or_default();
    if let Some(upstash) = upstash() {
        upstash.set(&key, &value)
            .map_err(|e| RexError::HostError(format!("db.set: {e}")))?;
    } else {
        with_db(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
                [&key, &value],
            ).map_err(|e| RexError::HostError(format!("db.set: {e}")))?;
            Ok(())
        })?;
    }
    Ok(heap.intern_value(&value))
}

fn op_db_delete(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let key = arg_str(args, 0, heap)?.to_string();
    let old_value = if let Some(upstash) = upstash() {
        upstash.delete(&key)
            .map_err(|e| RexError::HostError(format!("db.delete: {e}")))?
    } else {
        // SQLite lacks GETDEL, so fetch and delete under the same connection lock.
        with_db(|conn| {
            let result: Result<String, _> = conn
                .prepare_cached("SELECT value FROM kv WHERE key = ?1")
                .map_err(|e| RexError::HostError(format!("db.delete: {e}")))?
                .query_row([&key], |row| row.get(0));
            match result {
                Ok(value) => {
                    conn.execute("DELETE FROM kv WHERE key = ?1", [&key])
                        .map_err(|e| RexError::HostError(format!("db.delete: {e}")))?;
                    Ok(Some(value))
                }
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(RexError::HostError(format!("db.delete: {e}"))),
            }
        })?
    };

    Ok(old_value.map_or(Value::NONE, |value| heap.intern_value(&value)))
}

fn op_db_list(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let prefix = arg_str(args, 0, heap)?.to_string();
    let rows: Vec<(String, String)> = if let Some(upstash) = upstash() {
        upstash.list(&prefix)
            .map_err(|e| RexError::HostError(format!("db.list: {e}")))?
    } else {
        with_db(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT key, value FROM kv WHERE key LIKE ?1 ORDER BY key"
            ).map_err(|e| RexError::HostError(format!("db.list: {e}")))?;

            let pattern = format!("{prefix}%");
            let rows: Vec<(String, String)> = stmt.query_map([&pattern], |row| {
                let key: String = row.get(0)?;
                let value: String = row.get(1)?;
                Ok((key, value))
            })
            .map_err(|e| RexError::HostError(format!("db.list: {e}")))?
            .filter_map(|r| r.ok())
            .collect();

            Ok(rows)
        })?
    };

    let k_key = heap.intern("key");
    let k_val = heap.intern("value");
    let items: Vec<Value> = rows.iter().map(|(key, value)| {
        let vk = heap.intern_value(key);
        let vv = heap.intern_value(value);
        heap.alloc_object(vec![(k_key, vk), (k_val, vv)])
    }).collect();
    Ok(heap.alloc_array(items))
}

// ── Filesystem ────────────────────────────────────────────────────────

fn op_fs_read(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let path_str = arg_str(args, 0, heap)?.to_string();
    with_root(|root| {
        match sandbox_path(root, &path_str) {
            Ok(resolved) => {
                match std::fs::read_to_string(&resolved) {
                    Ok(content) => Ok(content),
                    Err(_) => Ok(String::new()),
                }
            }
            Err(_) => Ok(String::new()),
        }
    }).map(|content| {
        if content.is_empty() { Value::NONE } else { heap.intern_value(&content) }
    })
}

fn op_fs_glob(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let pattern = arg_str(args, 0, heap)?.to_string();
    let paths: Vec<String> = with_root(|root| {
        let full_pattern = root.join(&pattern);
        let pattern_str = full_pattern.to_string_lossy();
        let paths: Vec<String> = glob::glob(&pattern_str)
            .unwrap_or_else(|_| glob::glob("__nonexistent__").unwrap())
            .filter_map(|entry| entry.ok())
            .filter_map(|path| {
                path.strip_prefix(root).ok()
                    .map(|rel| rel.to_string_lossy().to_string())
            })
            .collect();
        Ok(paths)
    })?;

    let items: Vec<Value> = paths.iter().map(|p| heap.intern_value(p)).collect();
    Ok(heap.alloc_array(items))
}

fn op_fs_meta(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let path_str = arg_str(args, 0, heap)?.to_string();
    with_root(|root| {
        let resolved = sandbox_path(root, &path_str)?;
        match std::fs::metadata(&resolved) {
            Ok(meta) => {
                let modified = meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                Ok(Some((meta.len() as i64, modified)))
            }
            Err(_) => Ok(None),
        }
    }).map(|result| {
        match result {
            Some((size, modified)) => {
                let k_size = heap.intern("size");
                let k_modified = heap.intern("modified");
                heap.alloc_object(vec![
                    (k_size, Value::int(size)),
                    (k_modified, Value::int(modified)),
                ])
            }
            None => Value::NONE,
        }
    })
}

fn sandbox_path(root: &Path, user_path: &str) -> Result<PathBuf, RexError> {
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

fn op_markdown_render(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    use pulldown_cmark::{Parser, Event, Tag, TagEnd, CodeBlockKind};

    let text = arg_str(args, 0, heap)?.to_string();
    let parser = Parser::new(&text);

    let mut html = String::new();
    let mut in_rex_code_block = false;
    let mut code_buf = String::new();

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
                    pulldown_cmark::html::push_html(
                        &mut html,
                        std::iter::once(events[i].clone()),
                    );
                }
            }
            Event::End(TagEnd::CodeBlock) if in_rex_code_block => {
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

    Ok(heap.intern_value(&html))
}

fn op_template_render(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let template = arg_str(args, 0, heap)?.to_string();
    let data = args.get(1).copied().unwrap_or(Value::NONE);
    let result = render_template(&template, data, heap);
    Ok(heap.intern_value(&result))
}

fn render_template(template: &str, data: Value, heap: &Heap) -> String {
    let mut result = String::with_capacity(template.len());
    let mut i = 0;

    while i < template.len() {
        if template[i..].starts_with("{{") {
            let unescaped = template[i..].starts_with("{{{");
            let start = if unescaped { i + 3 } else { i + 2 };

            let closing = if unescaped { "}}}" } else { "}}" };
            if let Some(end) = template[start..].find(closing) {
                let key = template[start..start+end].trim();
                let value = lookup_template_key(data, key, heap);
                if unescaped {
                    result.push_str(&value);
                } else {
                    result.push_str(&html_escape(&value));
                }
                i = start + end + closing.len();
                continue;
            }
        }
        // Advance by one UTF-8 character
        let ch = &template[i..];
        let c = ch.chars().next().unwrap();
        result.push(c);
        i += c.len_utf8();
    }

    result
}

fn lookup_template_key(data: Value, key: &str, heap: &Heap) -> String {
    if data.is_object() {
        for &(k, v) in heap.object_pairs(data) {
            if heap.resolve_str(k) == key {
                return value_to_string(v, heap);
            }
        }
    }
    String::new()
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

fn op_time_now(_args: &[Value], _heap: &mut Heap) -> Result<Value, RexError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    Ok(Value::int(now))
}

fn op_time_uuid(_args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let id = uuid::Uuid::now_v7();
    Ok(heap.intern_value(&id.to_string()))
}

// ── Crypto ────────────────────────────────────────────────────────────

fn op_crypto_hash(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let algo = arg_str(args, 0, heap)?.to_string();
    let data = arg_str(args, 1, heap)?.to_string();
    match algo.as_str() {
        "sha256" => {
            use sha2::{Sha256, Digest};
            let mut hasher = Sha256::new();
            hasher.update(data.as_bytes());
            Ok(heap.intern_value(&hex::encode(hasher.finalize())))
        }
        _ => Err(RexError::HostError(format!("unsupported hash algorithm: {algo}"))),
    }
}

fn op_crypto_hmac(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let algo = arg_str(args, 0, heap)?.to_string();
    let key = arg_str(args, 1, heap)?.to_string();
    let data = arg_str(args, 2, heap)?.to_string();
    match algo.as_str() {
        "sha256" => {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;
            type HmacSha256 = Hmac<Sha256>;
            let mut mac = HmacSha256::new_from_slice(key.as_bytes())
                .map_err(|e| RexError::HostError(format!("hmac: {e}")))?;
            mac.update(data.as_bytes());
            Ok(heap.intern_value(&hex::encode(mac.finalize().into_bytes())))
        }
        _ => Err(RexError::HostError(format!("unsupported hmac algorithm: {algo}"))),
    }
}

fn op_crypto_random(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let n = args.first()
        .and_then(|v| v.as_i64())
        .unwrap_or(16) as usize;
    use rand::RngCore;
    let mut buf = vec![0u8; n];
    rand::rng().fill_bytes(&mut buf);
    Ok(heap.intern_value(&hex::encode(&buf)))
}

// ── HTTP ──────────────────────────────────────────────────────────────

fn op_http_fetch(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let url = arg_str(args, 0, heap)?.to_string();
    let options = args.get(1).copied().unwrap_or(Value::NONE);

    // Extract method, headers, body from options object
    let mut method = "GET".to_string();
    let mut body_str: Option<String> = None;
    let mut custom_headers: Vec<(String, String)> = Vec::new();

    if options.is_object() {
        for &(k, v) in heap.object_pairs(options) {
            match heap.resolve_str(k) {
                "method" => { if let Some(s) = v.as_str(heap) { method = s.to_string(); } }
                "body" => { if let Some(s) = v.as_str(heap) { body_str = Some(s.to_string()); } }
                "headers" => {
                    if v.is_object() {
                        for &(hk, hv) in heap.object_pairs(v) {
                            let key = heap.resolve_str(hk).to_string();
                            if let Some(val) = hv.as_str(heap) {
                                custom_headers.push((key, val.to_string()));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Use async reqwest — safe to block_on here because Rex runs on spawn_blocking
    let handle = tokio::runtime::Handle::current();
    let (status, resp_header_pairs, body_text) = handle.block_on(async {
        let client = reqwest::Client::new();
        let mut req = match method.as_str() {
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "DELETE" => client.delete(&url),
            "PATCH" => client.patch(&url),
            "HEAD" => client.head(&url),
            _ => client.get(&url),
        };
        for (k, v) in &custom_headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(b) = &body_str {
            req = req.body(b.clone());
        }
        let resp = req.send().await
            .map_err(|e| RexError::HostError(format!("http.fetch: {e}")))?;
        let status = resp.status().as_u16();
        let headers: Vec<(String, String)> = resp.headers().iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = resp.text().await
            .map_err(|e| RexError::HostError(format!("http.fetch: {e}")))?;
        Ok::<_, RexError>((status, headers, body))
    })?;

    // Build response headers object
    let mut resp_headers: Vec<(u32, Value)> = Vec::new();
    for (name, val) in &resp_header_pairs {
        let key = heap.intern(name);
        let v = heap.intern_value(val);
        resp_headers.push((key, v));
    }
    let headers_obj = heap.alloc_object(resp_headers);

    // Build {status: int, headers: {...}, body: str}
    let k_status = heap.intern("status");
    let k_headers = heap.intern("headers");
    let k_body = heap.intern("body");
    let body_val = heap.intern_value(&body_text);
    let result = heap.alloc_object(vec![
        (k_status, Value::int(status as i64)),
        (k_headers, headers_obj),
        (k_body, body_val),
    ]);
    Ok(result)
}

// ── Text ──────────────────────────────────────────────────────────────

fn op_html_escape(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let text = arg_str(args, 0, heap)?.to_string();
    Ok(heap.intern_value(&html_escape(&text)))
}

fn op_html_tag(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let parts_val = args.first().copied().unwrap_or(Value::NONE);
    if !parts_val.is_array() {
        return Err(RexError::HostError("html tag: first argument must be string parts array".into()));
    }
    let parts: Vec<Value> = heap.array_items(parts_val).to_vec();
    let values = &args[1..];
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if let Some(s) = part.as_str(heap) {
            out.push_str(s);
        }
        if i < values.len() {
            if let Some(raw_str) = extract_raw(values[i], heap) {
                out.push_str(&raw_str);
            } else {
                let val_str = value_to_string(values[i], heap);
                out.push_str(&html_escape(&val_str));
            }
        }
    }
    Ok(heap.intern_value(&out))
}

fn extract_raw(value: Value, heap: &Heap) -> Option<String> {
    if value.is_object() {
        let pairs = heap.object_pairs(value);
        if pairs.len() == 1 && heap.resolve_str(pairs[0].0) == "raw" {
            if let Some(s) = pairs[0].1.as_str(heap) {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn op_html_raw(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let value = args.first().copied().unwrap_or(Value::NONE);
    let s = value_to_string(value, heap);
    let k_raw = heap.intern("raw");
    let v_raw = heap.intern_value(&s);
    Ok(heap.alloc_object(vec![(k_raw, v_raw)]))
}

fn op_highlight_rex(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let source = arg_str(args, 0, heap)?.to_string();
    Ok(heap.intern_value(&highlight_rex_source(&source)))
}

fn highlight_rex_source(source: &str) -> String {
    use rex_core::lexer::{self, TokenKind};

    let tokens = lexer::lex(source);
    let mut out = String::with_capacity(source.len() * 2);

    let next_nonws = |i: usize| -> Option<TokenKind> {
        tokens[i+1..].iter()
            .find(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
    };

    let prev_nonws = |i: usize| -> Option<TokenKind> {
        tokens[..i].iter().rev()
            .find(|t| t.kind != TokenKind::Whitespace)
            .map(|t| t.kind)
    };

    for (i, token) in tokens.iter().enumerate() {
        let text = &source[token.span.clone()];
        let escaped = html_escape(text);
        let class = match token.kind {
            TokenKind::KwWhen | TokenKind::KwUnless | TokenKind::KwDo
            | TokenKind::KwEnd | TokenKind::KwElse | TokenKind::KwFor
            | TokenKind::KwIn | TokenKind::KwOf | TokenKind::KwWhile
            | TokenKind::KwAnd | TokenKind::KwOr
            | TokenKind::KwDelete
            | TokenKind::KwBreak | TokenKind::KwContinue
            | TokenKind::KwReturn => Some("kw"),

            TokenKind::KwTrue | TokenKind::KwFalse => Some("bl"),
            TokenKind::KwNull | TokenKind::KwNone
            | TokenKind::KwNan | TokenKind::KwInf => Some("ct"),

            TokenKind::DoubleString | TokenKind::SingleString => Some("st"),

            TokenKind::TemplateLiteral => {
                out.push_str(&highlight_template_literal(text));
                continue;
            }

            TokenKind::DecimalNumber | TokenKind::HexNumber
            | TokenKind::BinaryNumber => Some("nm"),

            TokenKind::LineComment | TokenKind::BlockComment => Some("cm"),

            TokenKind::Plus | TokenKind::Minus | TokenKind::Star
            | TokenKind::Slash | TokenKind::Percent
            | TokenKind::Amp | TokenKind::Pipe | TokenKind::Caret
            | TokenKind::Tilde | TokenKind::Eq | TokenKind::Gt
            | TokenKind::Lt | TokenKind::DotDot | TokenKind::DotDotDot
            | TokenKind::ColonEq | TokenKind::EqEq | TokenKind::BangEq
            | TokenKind::GtEq | TokenKind::LtEq
            | TokenKind::PlusEq | TokenKind::MinusEq
            | TokenKind::StarEq | TokenKind::SlashEq | TokenKind::PercentEq
            | TokenKind::AmpEq | TokenKind::PipeEq | TokenKind::CaretEq
            | TokenKind::Arrow => Some("op"),

            TokenKind::Dot | TokenKind::DotParen | TokenKind::Comma
            | TokenKind::Colon | TokenKind::At | TokenKind::Hash
            | TokenKind::LParen | TokenKind::RParen
            | TokenKind::LBracket | TokenKind::RBracket
            | TokenKind::LBrace | TokenKind::RBrace | TokenKind::Semicolon => Some("pn"),

            TokenKind::Ident => {
                if next_nonws(i) == Some(TokenKind::Colon) {
                    Some("ky")
                } else if prev_nonws(i) == Some(TokenKind::Dot) {
                    Some("pr")
                } else if next_nonws(i) == Some(TokenKind::LParen) {
                    Some("fn")
                } else {
                    None
                }
            }

            TokenKind::KwExtern | TokenKind::KwType => Some("kw"),

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

fn highlight_template_literal(text: &str) -> String {
    let mut out = String::new();
    let bytes = text.as_bytes();

    if bytes.is_empty() { return out; }

    let mut i = 0;

    out.push_str("<span class=\"hl-op\">`</span>");
    i += 1;

    while i < bytes.len() {
        if bytes[i] == b'`' {
            out.push_str("<span class=\"hl-op\">`</span>");
            break;
        } else if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let esc: String = text[i..i+2].to_string();
            out.push_str("<span class=\"hl-st\">");
            out.push_str(&html_escape(&esc));
            out.push_str("</span>");
            i += 2;
        } else if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            out.push_str("<span class=\"hl-op\">${</span>");
            i += 2;

            let expr_start = i;
            let mut depth: u32 = 1;
            while i < bytes.len() && depth > 0 {
                match bytes[i] {
                    b'{' => depth += 1,
                    b'}' => depth -= 1,
                    b'`' => {
                        i += 1;
                        while i < bytes.len() && bytes[i] != b'`' {
                            if bytes[i] == b'\\' { i += 1; }
                            i += 1;
                        }
                    }
                    b'\'' | b'"' => {
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

            let expr = &text[expr_start..i];
            if !expr.is_empty() {
                out.push_str(&highlight_rex_source(expr));
            }

            out.push_str("<span class=\"hl-op\">}</span>");
            if depth == 0 { i += 1; }
        } else {
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

fn op_highlight_html(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let source = arg_str(args, 0, heap)?.to_string();
    Ok(heap.intern_value(&highlight_html_source(&source)))
}

fn highlight_html_source(source: &str) -> String {
    let mut out = String::with_capacity(source.len() * 2);
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;

    while i < chars.len() {
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

        if chars[i] == '<' {
            out.push_str("<span class=\"hl-pn\">&lt;</span>");
            i += 1;

            if i < chars.len() && chars[i] == '/' {
                out.push_str("<span class=\"hl-pn\">/</span>");
                i += 1;
            }

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

            while i < chars.len() && chars[i] != '>' {
                if chars[i].is_whitespace() {
                    out.push(chars[i]);
                    i += 1;
                    continue;
                }

                if chars[i] == '/' {
                    out.push_str("<span class=\"hl-pn\">/</span>");
                    i += 1;
                    continue;
                }

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

                if chars[i] == '=' {
                    out.push_str("<span class=\"hl-op\">=</span>");
                    i += 1;
                    continue;
                }

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

            if i < chars.len() && chars[i] == '>' {
                out.push_str("<span class=\"hl-pn\">&gt;</span>");
                i += 1;
            }
            continue;
        }

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

        let text_start = i;
        while i < chars.len() && chars[i] != '<' && !(i + 1 < chars.len() && chars[i] == '{' && chars[i+1] == '{') {
            i += 1;
        }
        let text: String = chars[text_start..i].iter().collect();
        out.push_str(&html_escape(&text));
    }

    out
}

// ── KV Store ──────────────────────────────────────────────────────────

fn with_kv<F, R>(f: F) -> Result<R, RexError>
where F: FnOnce(&mut crate::kv::KvStore) -> Result<R, RexError> {
    KV_STORE.with(|cell| {
        let borrow = cell.borrow();
        let kv = borrow.as_ref().ok_or_else(|| RexError::HostError("no kv store".into()))?;
        let mut store = kv.lock().map_err(|e| RexError::HostError(format!("kv lock: {e}")))?;
        f(&mut store)
    })
}

fn op_kv_get(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let key = arg_str(args, 0, heap)?.to_string();
    let val = with_kv(|store| {
        Ok(store.get(&key).map(|v| v.to_string()))
    })?;
    match val {
        Some(v) => Ok(heap.intern_value(&v)),
        None => Ok(Value::NONE),
    }
}

fn op_kv_set(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let key = arg_str(args, 0, heap)?.to_string();
    let value = args.get(1).map(|v| value_to_string(*v, heap)).unwrap_or_default();
    let ttl = args.get(2).and_then(|v| v.as_i64()).map(|t| t as u64);
    with_kv(|store| {
        store.set(key, value.clone(), ttl);
        Ok(())
    })?;
    Ok(heap.intern_value(&value))
}

fn op_kv_delete(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let key = arg_str(args, 0, heap)?.to_string();
    let old = with_kv(|store| {
        Ok(store.delete(&key))
    })?;
    match old {
        Some(val) => Ok(heap.intern_value(&val)),
        None => Ok(Value::NONE),
    }
}

fn op_kv_keys(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let prefix = arg_str(args, 0, heap)?.to_string();
    let keys = with_kv(|store| {
        Ok(store.keys(&prefix))
    })?;
    let items: Vec<Value> = keys.into_iter().map(|k| heap.intern_value(&k)).collect();
    Ok(heap.alloc_array(items))
}

fn op_kv_incr(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let key = arg_str(args, 0, heap)?.to_string();
    with_kv(|store| {
        Ok(Value::int(store.incr(&key)))
    })
}

fn op_kv_publish(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let channel = arg_str(args, 0, heap)?.to_string();
    let data = args.get(1).map(|v| value_to_string(*v, heap)).unwrap_or_default();
    with_kv(|store| {
        let count = store.publish(&channel, &data);
        Ok(Value::int(count as i64))
    })
}

// ── Database: Compare-and-Swap ────────────────────────────────────────

fn op_db_cas(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let key = arg_str(args, 0, heap)?.to_string();
    let old = arg_str(args, 1, heap)?.to_string();
    let new = arg_str(args, 2, heap)?.to_string();
    if let Some(upstash) = upstash() {
        return match upstash.compare_and_swap(&key, &old, &new)
            .map_err(|e| RexError::HostError(format!("db.cas: {e}")))?
        {
            None => Ok(Value::NONE),
            Some(actual) => Ok(heap.intern_value(&actual)),
        };
    }
    with_db(|conn| {
        // Atomic CAS: check current value, update only if it matches old
        let current: Result<String, _> = conn
            .prepare_cached("SELECT value FROM kv WHERE key = ?1")
            .map_err(|e| RexError::HostError(format!("db.cas: {e}")))?
            .query_row([&key], |row| row.get(0));

        match current {
            Ok(actual) if actual == old => {
                conn.execute(
                    "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
                    [&key, &new],
                )
                .map_err(|e| RexError::HostError(format!("db.cas: {e}")))?;
                Ok(Value::NONE) // success
            }
            Ok(actual) => Ok(heap.intern_value(&actual)), // conflict
            Err(rusqlite::Error::QueryReturnedNoRows) if old.is_empty() => {
                // Key doesn't exist, old is empty → create it
                conn.execute(
                    "INSERT OR REPLACE INTO kv (key, value) VALUES (?1, ?2)",
                    [&key, &new],
                )
                .map_err(|e| RexError::HostError(format!("db.cas: {e}")))?;
                Ok(Value::NONE)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Ok(heap.intern_value("")) // conflict: key doesn't exist
            }
            Err(e) => Err(RexError::HostError(format!("db.cas: {e}"))),
        }
    })
}

// ── Content-Addressable Store ────────────────────────────────────────

fn op_cas_put(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let data = heap
        .blob_data(args[0])
        .ok_or_else(|| RexError::HostError("cas.put: expected binary argument".into()))?;

    use sha1::Digest;
    let hash = hex::encode(sha1::Sha1::digest(data));
    let data = data.to_vec();

    with_db(|conn| {
        conn.execute(
            "INSERT OR IGNORE INTO cas (hash, data) VALUES (?1, ?2)",
            rusqlite::params![&hash, &data],
        )
        .map_err(|e| RexError::HostError(format!("cas.put: {e}")))?;
        Ok(())
    })?;

    Ok(heap.intern_value(&hash))
}

fn op_cas_get(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let hash = arg_str(args, 0, heap)?.to_string();
    let data: Option<Vec<u8>> = with_db(|conn| {
        let result: Result<Vec<u8>, _> = conn
            .prepare_cached("SELECT data FROM cas WHERE hash = ?1")
            .map_err(|e| RexError::HostError(format!("cas.get: {e}")))?
            .query_row([&hash], |row| row.get(0));
        match result {
            Ok(data) => Ok(Some(data)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(RexError::HostError(format!("cas.get: {e}"))),
        }
    })?;

    match data {
        Some(bytes) => Ok(heap.alloc_blob(bytes)),
        None => Ok(Value::NONE),
    }
}

fn op_cas_has(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let hash = arg_str(args, 0, heap)?.to_string();
    let exists = with_db(|conn| {
        let result: Result<String, _> = conn
            .prepare_cached("SELECT hash FROM cas WHERE hash = ?1")
            .map_err(|e| RexError::HostError(format!("cas.has: {e}")))?
            .query_row([&hash], |row| row.get(0));
        match result {
            Ok(_) => Ok(true),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(RexError::HostError(format!("cas.has: {e}"))),
        }
    })?;

    if exists {
        Ok(heap.intern_value(&hash))
    } else {
        Ok(Value::NONE)
    }
}

// ── Git Objects ──────────────────────────────────────────────────────

fn op_git_decode(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    // Copy data out to release the borrow on heap
    let data = heap
        .blob_data(args[0])
        .ok_or_else(|| RexError::HostError("git.decode: expected binary".into()))?
        .to_vec();

    // Parse git object format: "{type} {size}\0{content}"
    let nul_pos = data
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| RexError::HostError("git.decode: no null byte in object".into()))?;

    let header =
        std::str::from_utf8(&data[..nul_pos]).map_err(|_| RexError::HostError("git.decode: invalid header".into()))?;

    let (obj_type, _size_str) = header
        .split_once(' ')
        .ok_or_else(|| RexError::HostError("git.decode: invalid header format".into()))?;

    let content = &data[nul_pos + 1..];

    match obj_type {
        "commit" => decode_commit(content, heap),
        "tree" => decode_tree(content, heap),
        "blob" => {
            let k_type = heap.intern("type");
            let k_size = heap.intern("size");
            let v_type = heap.intern_value("blob");
            let v_size = Value::int(content.len() as i64);
            Ok(heap.alloc_object(vec![(k_type, v_type), (k_size, v_size)]))
        }
        "tag" => decode_tag(content, heap),
        _ => Err(RexError::HostError(format!("git.decode: unknown type: {obj_type}"))),
    }
}

fn decode_ident(line: &str, heap: &mut Heap) -> Value {
    // Parse "Name <email> timestamp tz"
    let k_name = heap.intern("name");
    let k_email = heap.intern("email");
    let k_time = heap.intern("time");
    let k_tz = heap.intern("tz");

    let (name, rest) = line.split_once(" <").unwrap_or((line, ">"));
    let (email, rest) = rest.split_once("> ").unwrap_or((rest.trim_end_matches('>'), "0 +0000"));
    let mut parts = rest.splitn(2, ' ');
    let timestamp: i64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let tz = parts.next().unwrap_or("+0000");

    let v_name = heap.intern_value(name);
    let v_email = heap.intern_value(email);
    let v_time = Value::int(timestamp);
    let v_tz = heap.intern_value(tz);

    heap.alloc_object(vec![
        (k_name, v_name),
        (k_email, v_email),
        (k_time, v_time),
        (k_tz, v_tz),
    ])
}

fn decode_commit(content: &[u8], heap: &mut Heap) -> Result<Value, RexError> {
    let text = std::str::from_utf8(content)
        .map_err(|_| RexError::HostError("git.decode: invalid commit utf8".into()))?;

    let k_type = heap.intern("type");
    let k_tree = heap.intern("tree");
    let k_parents = heap.intern("parents");
    let k_message = heap.intern("message");
    let k_author = heap.intern("author");
    let k_committer = heap.intern("committer");

    let v_type = heap.intern_value("commit");
    let mut tree = Value::NONE;
    let mut parents = Vec::new();
    let mut author = Value::NONE;
    let mut committer = Value::NONE;
    let mut message_start = 0;

    for (i, line) in text.lines().enumerate() {
        if line.is_empty() {
            // Everything after the first blank line is the message
            let header_end = text
                .lines()
                .take(i + 1)
                .map(|l| l.len() + 1)
                .sum::<usize>();
            message_start = header_end;
            break;
        }
        if let Some(hash) = line.strip_prefix("tree ") {
            tree = heap.intern_value(hash);
        } else if let Some(hash) = line.strip_prefix("parent ") {
            parents.push(heap.intern_value(hash));
        } else if let Some(rest) = line.strip_prefix("author ") {
            author = decode_ident(rest, heap);
        } else if let Some(rest) = line.strip_prefix("committer ") {
            committer = decode_ident(rest, heap);
        }
    }

    let message = if message_start < text.len() {
        &text[message_start..]
    } else {
        ""
    };
    let v_message = heap.intern_value(message);
    let v_tree = tree;
    let v_parents = heap.alloc_array(parents);

    Ok(heap.alloc_object(vec![
        (k_type, v_type),
        (k_tree, v_tree),
        (k_parents, v_parents),
        (k_message, v_message),
        (k_author, author),
        (k_committer, committer),
    ]))
}

fn decode_tree(content: &[u8], heap: &mut Heap) -> Result<Value, RexError> {
    let k_type = heap.intern("type");
    let k_entries = heap.intern("entries");
    let k_name = heap.intern("name");
    let k_mode = heap.intern("mode");
    let k_hash = heap.intern("hash");

    let v_type = heap.intern_value("tree");
    let mut entries = Vec::new();
    let mut pos = 0;

    while pos < content.len() {
        // Format: "{mode} {name}\0{20-byte hash}"
        let space = content[pos..]
            .iter()
            .position(|&b| b == b' ')
            .ok_or_else(|| RexError::HostError("git.decode: invalid tree entry".into()))?;
        let mode = std::str::from_utf8(&content[pos..pos + space])
            .map_err(|_| RexError::HostError("git.decode: invalid tree mode".into()))?;
        pos += space + 1;

        let nul = content[pos..]
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| RexError::HostError("git.decode: invalid tree entry".into()))?;
        let name = std::str::from_utf8(&content[pos..pos + nul])
            .map_err(|_| RexError::HostError("git.decode: invalid tree name".into()))?;
        pos += nul + 1;

        if pos + 20 > content.len() {
            return Err(RexError::HostError("git.decode: truncated tree hash".into()));
        }
        let hash = hex::encode(&content[pos..pos + 20]);
        pos += 20;

        let v_name = heap.intern_value(name);
        let v_mode = heap.intern_value(mode);
        let v_hash = heap.intern_value(&hash);
        entries.push(heap.alloc_object(vec![
            (k_name, v_name),
            (k_mode, v_mode),
            (k_hash, v_hash),
        ]));
    }

    let v_entries = heap.alloc_array(entries);
    Ok(heap.alloc_object(vec![(k_type, v_type), (k_entries, v_entries)]))
}

fn decode_tag(content: &[u8], heap: &mut Heap) -> Result<Value, RexError> {
    let text = std::str::from_utf8(content)
        .map_err(|_| RexError::HostError("git.decode: invalid tag utf8".into()))?;

    let k_type = heap.intern("type");
    let k_object = heap.intern("object");
    let k_tag_type = heap.intern("tag-type");
    let k_tag = heap.intern("tag");
    let k_tagger = heap.intern("tagger");
    let k_message = heap.intern("message");

    let v_type = heap.intern_value("tag");
    let mut object = Value::NONE;
    let mut tag_type = Value::NONE;
    let mut tag_name = Value::NONE;
    let mut tagger = Value::NONE;
    let mut message_start = 0;

    for (i, line) in text.lines().enumerate() {
        if line.is_empty() {
            let header_end = text
                .lines()
                .take(i + 1)
                .map(|l| l.len() + 1)
                .sum::<usize>();
            message_start = header_end;
            break;
        }
        if let Some(hash) = line.strip_prefix("object ") {
            object = heap.intern_value(hash);
        } else if let Some(t) = line.strip_prefix("type ") {
            tag_type = heap.intern_value(t);
        } else if let Some(name) = line.strip_prefix("tag ") {
            tag_name = heap.intern_value(name);
        } else if let Some(rest) = line.strip_prefix("tagger ") {
            tagger = decode_ident(rest, heap);
        }
    }

    let message = if message_start < text.len() {
        &text[message_start..]
    } else {
        ""
    };
    let v_message = heap.intern_value(message);

    Ok(heap.alloc_object(vec![
        (k_type, v_type),
        (k_object, object),
        (k_tag_type, tag_type),
        (k_tag, tag_name),
        (k_tagger, tagger),
        (k_message, v_message),
    ]))
}

fn op_git_children(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let data = heap
        .blob_data(args[0])
        .ok_or_else(|| RexError::HostError("git.children: expected binary".into()))?
        .to_vec();

    let nul_pos = data
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| RexError::HostError("git.children: no null byte".into()))?;

    let header = std::str::from_utf8(&data[..nul_pos])
        .map_err(|_| RexError::HostError("git.children: invalid header".into()))?;

    let (obj_type, _) = header.split_once(' ').unwrap_or((header, ""));
    let content = &data[nul_pos + 1..];

    let mut hashes = Vec::new();

    match obj_type {
        "commit" => {
            let text = std::str::from_utf8(content).unwrap_or("");
            for line in text.lines() {
                if line.is_empty() {
                    break;
                }
                if let Some(hash) = line.strip_prefix("tree ") {
                    hashes.push(heap.intern_value(hash));
                } else if let Some(hash) = line.strip_prefix("parent ") {
                    hashes.push(heap.intern_value(hash));
                }
            }
        }
        "tree" => {
            let mut pos = 0;
            while pos < content.len() {
                // Skip mode
                while pos < content.len() && content[pos] != b' ' {
                    pos += 1;
                }
                pos += 1; // skip space
                // Skip name
                while pos < content.len() && content[pos] != 0 {
                    pos += 1;
                }
                pos += 1; // skip null
                // Read 20-byte hash
                if pos + 20 <= content.len() {
                    hashes.push(heap.intern_value(&hex::encode(&content[pos..pos + 20])));
                    pos += 20;
                } else {
                    break;
                }
            }
        }
        "tag" => {
            let text = std::str::from_utf8(content).unwrap_or("");
            for line in text.lines() {
                if line.is_empty() {
                    break;
                }
                if let Some(hash) = line.strip_prefix("object ") {
                    hashes.push(heap.intern_value(hash));
                }
            }
        }
        _ => {} // blobs have no children
    }

    Ok(heap.alloc_array(hashes))
}

fn op_git_verify(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let expected = arg_str(args, 0, heap)?.to_string();
    let data = heap
        .blob_data(args[1])
        .ok_or_else(|| RexError::HostError("git.verify: expected binary".into()))?;

    use sha1::Digest;
    let actual = hex::encode(sha1::Sha1::digest(data));

    if actual == expected {
        // Drop the borrow before mutating heap
        let hash = actual;
        Ok(heap.intern_value(&hash))
    } else {
        Ok(Value::NONE)
    }
}

fn op_git_is_ancestor(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let ancestor = arg_str(args, 0, heap)?.to_string();
    let descendant = arg_str(args, 1, heap)?.to_string();

    // Walk the commit graph from descendant back to find ancestor
    let current = descendant.clone();
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(current.clone());

    while let Some(hash) = queue.pop_front() {
        if hash == ancestor {
            return Ok(heap.intern_value(&descendant));
        }
        if !visited.insert(hash.clone()) {
            continue;
        }

        // Load commit from CAS
        let data: Option<Vec<u8>> = with_db(|conn| {
            let result: Result<Vec<u8>, _> = conn
                .prepare_cached("SELECT data FROM cas WHERE hash = ?1")
                .map_err(|e| RexError::HostError(format!("git.is-ancestor: {e}")))?
                .query_row([&hash], |row| row.get(0));
            match result {
                Ok(data) => Ok(Some(data)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(RexError::HostError(format!("git.is-ancestor: {e}"))),
            }
        })?;

        if let Some(data) = data {
            if let Some(nul_pos) = data.iter().position(|&b| b == 0) {
                let header = std::str::from_utf8(&data[..nul_pos]).unwrap_or("");
                if header.starts_with("commit ") {
                    let content = std::str::from_utf8(&data[nul_pos + 1..]).unwrap_or("");
                    for line in content.lines() {
                        if line.is_empty() {
                            break;
                        }
                        if let Some(parent) = line.strip_prefix("parent ") {
                            queue.push_back(parent.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(Value::NONE) // not an ancestor
}

/// Unified git.encode — dispatches on obj.type to encode commits, trees, or tags.
fn op_git_encode(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let obj = args.first().copied().unwrap_or(Value::NONE);
    let obj_type = obj_field_str(obj, "type", heap)
        .ok_or_else(|| RexError::HostError("git.encode: object must have a 'type' field".into()))?;

    match obj_type.as_str() {
        "commit" => encode_commit_obj(obj, heap),
        "tree" => encode_tree_obj(obj, heap),
        "tag" => encode_tag_obj(obj, heap),
        _ => Err(RexError::HostError(format!("git.encode: unknown type: {obj_type}"))),
    }
}

fn encode_commit_obj(obj: Value, heap: &mut Heap) -> Result<Value, RexError> {
    let tree = obj_field_str(obj, "tree", heap).unwrap_or_default();
    let parents = obj_field_val(obj, "parents", heap).unwrap_or(Value::NONE);
    let message = obj_field_str(obj, "message", heap).unwrap_or_default();
    let author = obj_field_val(obj, "author", heap).unwrap_or(Value::NONE);
    let committer = obj_field_val(obj, "committer", heap).unwrap_or(Value::NONE);

    let mut body = format!("tree {tree}\n");

    if parents.is_array() {
        for &p in heap.array_items(parents) {
            if let Some(hash) = p.as_str(heap) {
                body.push_str(&format!("parent {hash}\n"));
            }
        }
    }

    body.push_str(&format!("author {}\n", format_ident(author, heap)));
    body.push_str(&format!("committer {}\n", format_ident(committer, heap)));
    body.push('\n');
    body.push_str(&message);

    let header = format!("commit {}\0", body.len());
    let mut data = header.into_bytes();
    data.extend_from_slice(body.as_bytes());
    Ok(heap.alloc_blob(data))
}

fn encode_tree_obj(obj: Value, heap: &mut Heap) -> Result<Value, RexError> {
    let entries = obj_field_val(obj, "entries", heap).unwrap_or(Value::NONE);
    let mut body = Vec::new();

    if entries.is_array() {
        for &entry in heap.array_items(entries) {
            let mode = obj_field_str(entry, "mode", heap).unwrap_or_else(|| "100644".to_string());
            let name = obj_field_str(entry, "name", heap).unwrap_or_default();
            let hash_hex = obj_field_str(entry, "hash", heap).unwrap_or_default();

            body.extend_from_slice(mode.as_bytes());
            body.push(b' ');
            body.extend_from_slice(name.as_bytes());
            body.push(0);
            let hash_bytes =
                hex::decode(&hash_hex).map_err(|e| RexError::HostError(format!("git.encode: {e}")))?;
            body.extend_from_slice(&hash_bytes);
        }
    }

    let header = format!("tree {}\0", body.len());
    let mut data = header.into_bytes();
    data.extend_from_slice(&body);
    Ok(heap.alloc_blob(data))
}

fn encode_tag_obj(obj: Value, heap: &mut Heap) -> Result<Value, RexError> {
    let object = obj_field_str(obj, "object", heap).unwrap_or_default();
    let tag_type = obj_field_str(obj, "tag-type", heap).unwrap_or_default();
    let tag_name = obj_field_str(obj, "tag", heap).unwrap_or_default();
    let tagger = obj_field_val(obj, "tagger", heap).unwrap_or(Value::NONE);
    let message = obj_field_str(obj, "message", heap).unwrap_or_default();

    let body = format!(
        "object {object}\ntype {tag_type}\ntag {tag_name}\ntagger {}\n\n{message}",
        format_ident(tagger, heap)
    );

    let header = format!("tag {}\0", body.len());
    let mut data = header.into_bytes();
    data.extend_from_slice(body.as_bytes());
    Ok(heap.alloc_blob(data))
}

fn format_ident(obj: Value, heap: &Heap) -> String {
    if obj.is_object() {
        let name = obj_field_str(obj, "name", heap).unwrap_or_default();
        let email = obj_field_str(obj, "email", heap).unwrap_or_default();
        let time = obj_field_i64(obj, "time", heap).unwrap_or(0);
        let tz = obj_field_str(obj, "tz", heap).unwrap_or_else(|| "+0000".to_string());
        format!("{name} <{email}> {time} {tz}")
    } else if let Some(s) = obj.as_str(heap) {
        s.to_string()
    } else {
        "Unknown <unknown> 0 +0000".to_string()
    }
}

fn obj_field_str(obj: Value, key: &str, heap: &Heap) -> Option<String> {
    for &(k, v) in heap.object_pairs(obj) {
        if heap.resolve_str(k) == key {
            return v.as_str(heap).map(|s| s.to_string());
        }
    }
    None
}

fn obj_field_val(obj: Value, key: &str, heap: &Heap) -> Option<Value> {
    for &(k, v) in heap.object_pairs(obj) {
        if heap.resolve_str(k) == key {
            return Some(v);
        }
    }
    None
}

fn obj_field_i64(obj: Value, key: &str, heap: &Heap) -> Option<i64> {
    for &(k, v) in heap.object_pairs(obj) {
        if heap.resolve_str(k) == key {
            return v.as_i64();
        }
    }
    None
}

fn op_git_encode_blob(args: &[Value], heap: &mut Heap) -> Result<Value, RexError> {
    let input = heap
        .blob_data(args[0])
        .ok_or_else(|| RexError::HostError("git.encode-blob: expected binary".into()))?
        .to_vec();

    let header = format!("blob {}\0", input.len());
    let mut data = header.into_bytes();
    data.extend_from_slice(&input);
    Ok(heap.alloc_blob(data))
}

// ── Database Init ─────────────────────────────────────────────────────

pub fn init_db(path: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path)
        .expect("failed to open database");
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS cas (hash TEXT PRIMARY KEY, data BLOB NOT NULL);"
    ).expect("failed to create tables");
    conn
}
