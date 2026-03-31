use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::router::RouteTable;

pub struct AppState {
    pub config: Config,
    pub route_table: RwLock<RouteTable>,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    pub project_root: PathBuf,
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

    let state = Arc::new(AppState {
        config,
        route_table: RwLock::new(table),
        db,
        project_root,
    });

    // Start file watcher for hot reload
    let watcher_state = state.clone();
    let watcher_dir = routes_dir.clone();
    tokio::spawn(async move {
        watch_routes(watcher_dir, watcher_state).await;
    });

    let app = Router::new()
        .fallback(crate::handler::handle_request)
        .with_state(state.clone());

    let addr = format!("{}:{}", state.config.server.host, state.config.server.port);
    tracing::info!("listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await
        .expect("failed to bind");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn watch_routes(routes_dir: PathBuf, state: Arc<AppState>) {
    use notify::{Watcher, RecursiveMode, Event, EventKind};

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(1);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        if let Ok(event) = res {
            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                    let _ = tx.try_send(());
                }
                _ => {}
            }
        }
    }).expect("failed to create file watcher");

    watcher.watch(&routes_dir, RecursiveMode::Recursive)
        .expect("failed to watch routes directory");

    tracing::info!("watching {} for changes", routes_dir.display());

    // Keep the watcher alive and debounce rebuild events
    while rx.recv().await.is_some() {
        // Drain any queued events (debounce rapid saves)
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        while rx.try_recv().is_ok() {}

        tracing::info!("change detected, reloading routes...");
        let new_table = RouteTable::build(&routes_dir);
        tracing::info!(
            "reloaded: {} routes, {} middlewares, {} static files",
            new_table.routes.len(),
            new_table.middlewares.len(),
            new_table.static_files.len(),
        );
        *state.route_table.write().await = new_table;
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install ctrl-c handler");
    tracing::info!("shutting down");
}
