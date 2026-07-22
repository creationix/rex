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
        .merge(rex_serve::ws::router())
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
