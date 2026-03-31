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
async fn ws_pubsub_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::Path(channel): axum::extract::Path<String>,
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| ws_pubsub_connection(socket, channel, state))
}

async fn ws_pubsub_connection(
    mut socket: axum::extract::ws::WebSocket,
    channel: String,
    state: Arc<AppState>,
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
                    Err(_) => break,
                }
            }
            // Client message → publish to channel
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let mut store = state.kv.lock().unwrap();
                        store.publish(&channel, &text);
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
    tracing::debug!("pub/sub client disconnected from channel: {channel}");
}

async fn watch_routes(routes_dir: PathBuf, state: Arc<AppState>) {
    use notify::{Watcher, RecursiveMode, Event, EventKind};

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

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install ctrl-c handler");
    tracing::info!("shutting down");
}
