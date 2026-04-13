use axum::Router;
use axum::routing::get;
use std::path::PathBuf;
use std::sync::Arc;
use rex_serve::config::Config;
use rex_serve::router::RouteTable;
use rex_serve::state::AppState;

pub async fn run(config: Config, project_root: PathBuf) {
    // Bind the port early so we fail fast if it's already in use
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot bind to {addr}: {e}");
            std::process::exit(1);
        }
    };

    let state = match AppState::build(config, project_root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let routes_dir = state.project_root.join(&state.config.routes.dir);

    // Background TTL eviction for in-memory KV store
    let kv_evict = state.kv.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            if let Ok(mut store) = kv_evict.lock() {
                store.evict_expired();
            }
        }
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
        .fallback(rex_serve::handler::handle_request)
        .with_state(state.clone());

    tracing::info!("listening on http://{addr}");
    tracing::info!("live reload WebSocket at ws://{addr}/__reload");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

// ── WebSocket handlers (standalone server only) ──────────────────────

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
                match msg {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
    tracing::debug!("live reload client disconnected");
}

async fn ws_pubsub_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::Path(channel): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
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

    let mut rx = {
        let mut store = state.kv.lock().unwrap();
        store.subscribe(&channel)
    };

    tracing::debug!("pub/sub client connected to channel: {channel}");

    loop {
        tokio::select! {
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

fn run_ws_transform(bytecode: &str, data: &str, state: &AppState) -> Option<String> {
    use rex_core::heap::{Value, Heap};
    use rex_core::interpret::Context;
    use rex_serve::refs::OpcodeNamespace;
    use std::collections::HashMap;

    let mut heap = Heap::new();

    let k_data = heap.intern("data");
    let v_data = heap.intern_value(data);
    let event_obj = heap.alloc_object(vec![(k_data, v_data)]);

    let mut vars = HashMap::new();
    vars.insert("event".into(), event_obj);

    let mut ns_json = OpcodeNamespace { methods: vec![("parse", "jp"), ("stringify", "js")], tag_opcode: None };
    let mut ns_log = OpcodeNamespace { methods: vec![("info", "li"), ("warning", "lw"), ("error", "le")], tag_opcode: None };
    let mut ns_kv = OpcodeNamespace { methods: vec![("get", "kg"), ("set", "ks"), ("del", "kd"), ("keys", "kk"), ("incr", "ki"), ("publish", "kp")], tag_opcode: None };
    let mut ns_db = OpcodeNamespace { methods: vec![("get", "dg"), ("set", "ds"), ("del", "dd"), ("list", "dl"), ("cas", "dc")], tag_opcode: None };
    let mut ns_time = OpcodeNamespace { methods: vec![("now", "tn"), ("uuid", "tu")], tag_opcode: None };
    let mut ns_cas = OpcodeNamespace { methods: vec![("put", "cp"), ("get", "cg"), ("has", "cx")], tag_opcode: None };
    let mut ns_git = OpcodeNamespace { methods: vec![("decode", "gd"), ("children", "gc"), ("verify", "gv"), ("is-ancestor", "ga"), ("encode", "ge"), ("encode-blob", "gB")], tag_opcode: None };
    let mut ns_crypto = OpcodeNamespace { methods: vec![("hash", "ch"), ("hmac", "cm"), ("random", "cr")], tag_opcode: None };

    vars.insert("json".into(), Value::host(0));
    vars.insert("log".into(), Value::host(1));
    vars.insert("kv".into(), Value::host(2));
    vars.insert("db".into(), Value::host(3));
    vars.insert("time".into(), Value::host(4));
    vars.insert("cas".into(), Value::host(5));
    vars.insert("git".into(), Value::host(6));
    vars.insert("crypto".into(), Value::host(7));

    let opcodes = rex_serve::opcodes::build_opcodes(
        state.db.clone(),
        state.project_root.clone(),
        state.kv.clone(),
    );

    let ctx = Context {
        refs: HashMap::new(),
        vars,
        host_objects: vec![
            &mut ns_json,
            &mut ns_log,
            &mut ns_kv,
            &mut ns_db,
            &mut ns_time,
            &mut ns_cas,
            &mut ns_git,
            &mut ns_crypto,
        ],
        opcodes,
        gas_limit: state.config.server.gas_limit,
        heap,
    };

    match rex_core::interpret::run(bytecode, ctx) {
        Ok(result) => {
            let v = result.value;
            let heap = &result.heap;
            if v.is_none() {
                None
            } else if let Some(s) = v.as_str(heap) {
                Some(s.to_string())
            } else if v.is_object() || v.is_array() {
                let json = rex_serve::refs::value_to_json(v, heap);
                Some(json.to_string())
            } else {
                Some(rex_serve::refs::value_to_string(v, heap))
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

    let schema = state.domain_source.as_ref()
        .map(|src| rex_core::typecheck::parse_rexd(src));

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

    while rx.recv().await.is_some() {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let mut changed_paths: Vec<PathBuf> = Vec::new();
        while let Ok(paths) = rx.try_recv() {
            changed_paths.extend(paths);
        }

        tracing::info!("change detected, reloading routes...");
        let (new_table, type_errors) = RouteTable::build_with_domain(
            &routes_dir, state.domain_source.as_deref(), schema.as_ref(),
        );
        if type_errors > 0 {
            tracing::error!("reload rejected: {} type error(s), keeping previous version", type_errors);
            continue;
        }
        tracing::info!(
            "reloaded: {} routes, {} middlewares, {} static files",
            new_table.routes.len(),
            new_table.middlewares.len(),
            new_table.static_files.len(),
        );
        *state.route_table.write().await = new_table;

        for path in &changed_paths {
            let rel = path.strip_prefix(&routes_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            let _ = state.reload_tx.send(rel);
        }
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install ctrl-c handler");
    tracing::info!("shutting down");
}
