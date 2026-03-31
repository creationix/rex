use axum::Router;
use axum::routing::get;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

use crate::config::Config;
use crate::router::RouteTable;

pub struct AppState {
    pub config: Config,
    pub route_table: RwLock<RouteTable>,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    pub project_root: PathBuf,
    /// Broadcast channel for file change notifications (for WebSocket clients)
    pub reload_tx: broadcast::Sender<String>,
    /// In-memory KV store with pub/sub
    pub kv: Arc<std::sync::Mutex<crate::kv::KvStore>>,
}

pub async fn run(config: Config, project_root: PathBuf) {
    let routes_dir = project_root.join(&config.routes.dir);
    let db_path = project_root.join(&config.db.path);

    // Init database
    let conn = crate::opcodes::init_db(&db_path);
    let db = Arc::new(std::sync::Mutex::new(conn));

    // Build route table
    tracing::info!("scanning routes in {}", routes_dir.display());
    let table = RouteTable::build(&routes_dir);
    tracing::info!(
        "loaded {} routes, {} middlewares, {} static files",
        table.routes.len(),
        table.middlewares.len(),
        table.static_files.len(),
    );

    for route in &table.routes {
        let pattern: Vec<String> = route.segments.iter().map(|s| match s {
            crate::router::Segment::Static(s) => s.clone(),
            crate::router::Segment::Param(n) => format!(":{n}"),
            crate::router::Segment::CatchAll(n) => format!("*{n}"),
        }).collect();
        let path = if pattern.is_empty() { "/".into() } else { format!("/{}", pattern.join("/")) };
        tracing::info!("  route: {path} ← {}", route.source_path.display());
    }

    for mw in &table.middlewares {
        tracing::info!("  middleware: {}** ← {}", mw.prefix, mw.source_path.display());
    }

    for sf in &table.static_files {
        tracing::info!("  static: {} ← {}", sf.url_path, sf.fs_path.display());
    }

    // Broadcast channel for live reload notifications
    let (reload_tx, _) = broadcast::channel::<String>(16);

    // In-memory KV store
    let kv = Arc::new(std::sync::Mutex::new(crate::kv::KvStore::new()));

    // Background TTL eviction
    let kv_evict = kv.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            if let Ok(mut store) = kv_evict.lock() {
                store.evict_expired();
            }
        }
    });

    let state = Arc::new(AppState {
        config,
        route_table: RwLock::new(table),
        db,
        project_root,
        reload_tx,
        kv,
    });

    // Start file watcher for hot reload
    let watcher_state = state.clone();
    let watcher_dir = routes_dir.clone();
    tokio::spawn(async move {
        watch_routes(watcher_dir, watcher_state).await;
    });

    let app = Router::new()
        .route("/__reload", get(ws_reload_handler))
        .route("/__ws/{channel}", get(ws_pubsub_handler))
        .fallback(crate::handler::handle_request)
        .with_state(state.clone());

    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);
    tracing::info!("listening on http://{addr}");
    tracing::info!("live reload WebSocket at ws://{addr}/__reload");

    let listener = tokio::net::TcpListener::bind(&addr).await
        .expect("failed to bind");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

/// WebSocket handler for live reload notifications.
/// Clients connect and receive file paths as they change.
async fn ws_reload_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| ws_reload_connection(socket, state))
}

async fn ws_reload_connection(
    mut socket: axum::extract::ws::WebSocket,
    state: Arc<AppState>,
) {
    use axum::extract::ws::Message;

    let mut rx = state.reload_tx.subscribe();
    tracing::debug!("live reload client connected");

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(path) => {
                        if socket.send(Message::Text(path.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                // Client closed or sent a message (ignore messages)
                match msg {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
    tracing::debug!("live reload client disconnected");
}

/// WebSocket handler for pub/sub channels.
/// Clients connect to /__ws/{channel} and send/receive JSON messages.
/// If `routes/_ws/{channel}.rex` exists, each message is transformed through it.
async fn ws_pubsub_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::Path(channel): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
    // Check for a channel-specific Rex transform script
    let transform = {
        let table = state.route_table.read().await;
        table.ws_transforms.get(&channel).cloned()
    };
    ws.on_upgrade(move |socket| ws_pubsub_connection(socket, channel, state, transform))
}

async fn ws_pubsub_connection(
    mut socket: axum::extract::ws::WebSocket,
    channel: String,
    state: Arc<AppState>,
    transform: Option<String>,
) {
    use axum::extract::ws::Message;

    // Subscribe to the channel
    let mut rx = {
        let mut store = state.kv.lock().unwrap();
        store.subscribe(&channel)
    };

    tracing::debug!("pub/sub client connected to channel: {channel}");

    loop {
        tokio::select! {
            // Broadcast message from channel → send to client
            msg = rx.recv() => {
                match msg {
                    Ok(data) => {
                        if socket.send(Message::Text(data.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("pub/sub client lagged, skipped {n} messages");
                    }
                    Err(_) => break,
                }
            }
            // Client message → transform via Rex (if script exists) → publish
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let output = match &transform {
                            Some(bytecode) => run_ws_transform(bytecode, &text, &state),
                            None => Some(text.to_string()),
                        };
                        if let Some(data) = output {
                            let store = state.kv.lock().unwrap();
                            store.publish(&channel, &data);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
    tracing::debug!("pub/sub client disconnected from channel: {channel}");
}

/// Run a Rex transform script on a WebSocket message.
/// The script receives `event.data` (the raw message string).
/// Returns the transformed message string, or None to suppress.
fn run_ws_transform(bytecode: &str, data: &str, state: &AppState) -> Option<String> {
    use rex_core::interpret::{Context, RexValue};
    use crate::refs::OpcodeNamespace;
    use std::collections::HashMap;

    let mut vars = HashMap::new();
    vars.insert("event".into(), RexValue::Object(vec![
        ("data".into(), RexValue::Str(data.to_string())),
    ]));

    // Set up opcode namespaces so json.parse, kv.*, log.*, etc. work
    let mut ns_json = OpcodeNamespace { methods: vec![("parse", "jp"), ("stringify", "js")], tag_opcode: None };
    let mut ns_log = OpcodeNamespace { methods: vec![("info", "li"), ("warning", "lw"), ("error", "le")], tag_opcode: None };
    let mut ns_kv = OpcodeNamespace { methods: vec![("get", "kg"), ("set", "ks"), ("delete", "kd"), ("keys", "kk"), ("incr", "ki"), ("publish", "kp")], tag_opcode: None };
    let mut ns_db = OpcodeNamespace { methods: vec![("get", "dg"), ("set", "ds"), ("delete", "dd"), ("list", "dl")], tag_opcode: None };
    let mut ns_time = OpcodeNamespace { methods: vec![("now", "tn"), ("uuid", "tu")], tag_opcode: None };

    vars.insert("json".into(), RexValue::Host(0));
    vars.insert("log".into(), RexValue::Host(1));
    vars.insert("kv".into(), RexValue::Host(2));
    vars.insert("db".into(), RexValue::Host(3));
    vars.insert("time".into(), RexValue::Host(4));

    let opcodes = crate::opcodes::build_opcodes(
        state.db.clone(),
        state.project_root.clone(),
        state.kv.clone(),
    );

    let ctx = Context {
        refs: HashMap::new(),
        vars,
        host_objects: vec![
            &mut ns_json,   // 0
            &mut ns_log,    // 1
            &mut ns_kv,     // 2
            &mut ns_db,     // 3
            &mut ns_time,   // 4
        ],
        opcodes,
        gas_limit: state.config.server.gas_limit,
    };

    match rex_core::interpret::run(bytecode, ctx) {
        Ok(result) => {
            match &result.value {
                RexValue::RexNone => None,
                RexValue::Str(s) => Some(s.clone()),
                RexValue::Object(_) | RexValue::Array(_) => {
                    let json = crate::refs::rex_value_to_json(&result.value);
                    Some(json.to_string())
                }
                other => Some(crate::refs::rex_value_to_string(other)),
            }
        }
        Err(e) => {
            tracing::error!("ws transform error: {e}");
            Some(data.to_string())
        }
    }
}

async fn watch_routes(routes_dir: PathBuf, state: Arc<AppState>) {
    use notify::{Watcher, RecursiveMode, Event, EventKind};

    // Load the domain schema for type checking (look for *.rexd in project root)
    let schema = load_domain_schema(&state.project_root);

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<PathBuf>>(8);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    let _ = tx.try_send(event.paths);
                }
                _ => {}
            }
        }
    }).expect("failed to create file watcher");

    watcher.watch(&routes_dir, RecursiveMode::Recursive)
        .expect("failed to watch routes directory");

    tracing::info!("watching {} for changes", routes_dir.display());

    // Run type check on startup
    if let Some(ref s) = schema {
        run_type_check(&routes_dir, s);
    }

    while rx.recv().await.is_some() {
        // Debounce: wait briefly then drain queued events
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let mut changed_paths: Vec<PathBuf> = Vec::new();
        while let Ok(paths) = rx.try_recv() {
            changed_paths.extend(paths);
        }

        tracing::info!("change detected, reloading routes...");
        let new_table = RouteTable::build(&routes_dir);
        tracing::info!(
            "reloaded: {} routes, {} middlewares, {} static files",
            new_table.routes.len(),
            new_table.middlewares.len(),
            new_table.static_files.len(),
        );
        *state.route_table.write().await = new_table;

        // Type-check changed .rex files
        if let Some(ref s) = schema {
            let rex_files: Vec<&PathBuf> = changed_paths.iter()
                .filter(|p| p.extension().is_some_and(|e| e == "rex"))
                .collect();
            if !rex_files.is_empty() {
                type_check_files(&rex_files, &routes_dir, s);
            }
        }

        // Notify WebSocket clients of changed files
        for path in &changed_paths {
            let rel = path.strip_prefix(&routes_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            let _ = state.reload_tx.send(rel);
        }
    }
}

/// Load the domain schema from *.rexd files in the project root.
fn load_domain_schema(project_root: &std::path::Path) -> Option<rex_core::typecheck::DomainSchema> {
    // Find .rexd files in project root
    let mut rexd_content = String::new();
    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "rexd") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    tracing::info!("type checking with {}", path.display());
                    rexd_content.push_str(&content);
                    rexd_content.push('\n');
                }
            }
        }
    }
    if rexd_content.is_empty() {
        None
    } else {
        Some(rex_core::typecheck::parse_rexd(&rexd_content))
    }
}

/// Type-check all .rex files in the routes directory.
fn run_type_check(routes_dir: &std::path::Path, schema: &rex_core::typecheck::DomainSchema) {
    let mut files = Vec::new();
    collect_rex_files(routes_dir, &mut files);
    if files.is_empty() { return; }
    type_check_files(&files.iter().collect::<Vec<_>>(), routes_dir, schema);
}

/// Recursively collect all .rex files.
fn collect_rex_files(dir: &std::path::Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            collect_rex_files(&path, files);
        } else if path.extension().is_some_and(|e| e == "rex") {
            files.push(path);
        }
    }
}

/// Type-check specific files and log diagnostics.
fn type_check_files(
    files: &[&PathBuf],
    routes_dir: &std::path::Path,
    schema: &rex_core::typecheck::DomainSchema,
) {
    use rex_core::typecheck::DiagnosticKind;

    let mut total_errors = 0u32;
    let mut total_warnings = 0u32;

    for path in files {
        let Ok(source) = std::fs::read_to_string(path) else { continue };
        let diagnostics = rex_core::typecheck::check_source(&source, schema);

        for d in &diagnostics {
            let rel = path.strip_prefix(routes_dir).unwrap_or(path);
            let line = span_to_line(&source, d.span.start);
            match d.kind {
                DiagnosticKind::Warning => {
                    tracing::warn!("{}:{}: {}", rel.display(), line, d.message);
                    total_warnings += 1;
                }
                DiagnosticKind::Error => {
                    tracing::error!("{}:{}: {}", rel.display(), line, d.message);
                    total_errors += 1;
                }
            }
        }
    }

    if total_errors > 0 || total_warnings > 0 {
        tracing::info!("type check: {} error(s), {} warning(s)", total_errors, total_warnings);
    } else if !files.is_empty() {
        tracing::info!("type check: all clear");
    }
}

/// Convert a byte offset to a 1-based line number.
fn span_to_line(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].matches('\n').count() + 1
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install ctrl-c handler");
    tracing::info!("shutting down");
}
