use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

use crate::config::Config;
use crate::router::RouteTable;

pub struct AppState {
    pub config: Config,
    pub route_table: RwLock<RouteTable>,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    /// Optional durable string KV backend. SQLite remains available for CAS/git data.
    pub upstash: Option<Arc<crate::storage::UpstashClient>>,
    pub project_root: PathBuf,
    /// Broadcast channel for file change notifications (for WebSocket clients)
    pub reload_tx: broadcast::Sender<String>,
    /// In-memory KV store with pub/sub
    pub kv: Arc<std::sync::Mutex<crate::kv::KvStore>>,
    /// Domain schema source (.rexd) for domain-aware compilation
    pub domain_source: Option<String>,
    /// Read-only secrets loaded from environment variables with the configured prefix.
    pub secrets: serde_json::Value,
}

impl AppState {
    /// Build shared state from a config and project root.
    /// Initializes the database, type-checks all routes, and builds the route table.
    /// Returns an error string if type checking fails.
    pub fn build(config: Config, project_root: PathBuf) -> Result<Arc<Self>, String> {
        let routes_dir = project_root.join(&config.routes.dir);
        let db_path = project_root.join(&config.db.path);

        // Init database — fall back to /tmp if configured path is read-only (e.g. Vercel)
        let db_path = if std::fs::OpenOptions::new().write(true).create(true).open(&db_path).is_ok() {
            db_path
        } else {
            let tmp_path = std::path::PathBuf::from("/tmp/rex-serve-data.db");
            tracing::info!("db path {} not writable, using {}", db_path.display(), tmp_path.display());
            tmp_path
        };
        let conn = crate::opcodes::init_db(&db_path);
        let db = Arc::new(std::sync::Mutex::new(conn));

        let upstash = match config.db.backend.as_str() {
            "sqlite" => None,
            "auto" => crate::storage::UpstashClient::from_env()?.map(Arc::new),
            "upstash" => Some(Arc::new(
                crate::storage::UpstashClient::from_env()?
                    .ok_or_else(|| "db.backend is upstash but Upstash environment variables are missing".to_string())?,
            )),
            backend => return Err(format!("unsupported database backend: {backend}")),
        };
        if upstash.is_some() {
            tracing::info!("using Upstash Redis for db.* string KV operations");
        } else {
            tracing::info!("using SQLite for db.* string KV operations");
        }

        let secrets = load_secrets(&config.secrets.env_prefix);
        if let Some(values) = secrets.as_object() {
            tracing::info!("loaded {} secret(s) from the environment", values.len());
        }

        // Load domain schema (.rexd) for domain-aware compilation and type checking
        let domain_source = find_rexd(&project_root);
        let schema = domain_source.as_ref().map(|src| {
            // Type-check the .rexd file itself
            let diags = rex_core::typecheck::check_source(src, &rex_core::typecheck::DomainSchema::default());
            let rexd_errors: Vec<_> = diags.iter()
                .filter(|d| d.kind == rex_core::typecheck::DiagnosticKind::Error)
                .collect();
            if !rexd_errors.is_empty() {
                for d in &rexd_errors {
                    tracing::error!(".rexd: {}", d.message);
                }
            }
            (rex_core::typecheck::parse_rexd(src), rexd_errors.len())
        });

        if let Some((_, n)) = &schema {
            if *n > 0 {
                return Err(format!(".rexd has {} type error(s)", n));
            }
            tracing::info!("domain-aware compilation enabled (found .rexd)");
        }

        let parsed_schema = schema.map(|(s, _)| s);

        // Build route table with type checking and domain-aware compilation
        tracing::info!("scanning routes in {}", routes_dir.display());
        let (table, type_errors) = RouteTable::build_with_domain(
            &routes_dir, domain_source.as_deref(), parsed_schema.as_ref(),
        );
        if type_errors > 0 {
            return Err(format!("{} type error(s) in routes", type_errors));
        }

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

        // Broadcast channel (used by standalone server for hot reload; unused in serverless)
        let (reload_tx, _) = broadcast::channel::<String>(16);

        // In-memory KV store
        let kv = Arc::new(std::sync::Mutex::new(crate::kv::KvStore::new()));

        Ok(Arc::new(Self {
            config,
            route_table: RwLock::new(table),
            db,
            upstash,
            project_root,
            reload_tx,
            kv,
            domain_source,
            secrets,
        }))
    }
}

fn load_secrets(prefix: &str) -> serde_json::Value {
    let values = std::env::vars()
        .filter_map(|(name, value)| secret_name(&name, prefix).map(|key| (key, value.into())))
        .collect();
    serde_json::Value::Object(values)
}

fn secret_name(name: &str, prefix: &str) -> Option<String> {
    let suffix = name.strip_prefix(prefix)?;
    if suffix.is_empty() {
        return None;
    }
    Some(suffix.to_ascii_lowercase().replace('_', "-"))
}

#[cfg(test)]
mod tests {
    use super::secret_name;

    #[test]
    fn secret_names_map_to_rex_properties() {
        assert_eq!(
            secret_name("REX_SECRET_API_KEY", "REX_SECRET_"),
            Some("api-key".into())
        );
        assert_eq!(secret_name("OTHER_API_KEY", "REX_SECRET_"), None);
    }
}

/// Find and read the first .rexd file in the project root.
pub fn find_rexd(project_root: &std::path::Path) -> Option<String> {
    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "rexd") && path.is_file() {
                return std::fs::read_to_string(&path).ok();
            }
        }
    }
    None
}
