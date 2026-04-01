use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub routes: RoutesConfig,
    #[serde(default)]
    pub db: DbConfig,
    #[serde(default)]
    pub secrets: SecretsConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_max_body")]
    pub max_body_bytes: usize,
    #[serde(default = "default_gas")]
    pub gas_limit: u64,
}

#[derive(Debug, Deserialize)]
pub struct RoutesConfig {
    #[serde(default = "default_routes_dir")]
    pub dir: PathBuf,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DbConfig {
    #[serde(default = "default_db_backend")]
    pub backend: String,
    #[serde(default = "default_db_path")]
    pub path: PathBuf,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct SecretsConfig {
    #[serde(default = "default_secrets_prefix")]
    pub env_prefix: String,
}

fn default_host() -> String { "0.0.0.0".into() }
fn default_port() -> u16 { 3000 }
fn default_max_body() -> usize { 1_048_576 }
fn default_gas() -> u64 { 1_000_000 }
fn default_routes_dir() -> PathBuf { PathBuf::from("routes") }
fn default_db_backend() -> String { "sqlite".into() }
fn default_db_path() -> PathBuf { PathBuf::from("data.db") }
fn default_secrets_prefix() -> String { "REX_SECRET_".into() }

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            max_body_bytes: default_max_body(),
            gas_limit: default_gas(),
        }
    }
}

impl Default for RoutesConfig {
    fn default() -> Self {
        Self { dir: default_routes_dir() }
    }
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            backend: default_db_backend(),
            path: default_db_path(),
        }
    }
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self { env_prefix: default_secrets_prefix() }
    }
}

impl Config {
    pub fn load(project_root: &Path) -> Self {
        let config_path = project_root.join("rex-serve.toml");
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .unwrap_or_default();
            toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("warning: failed to parse rex-serve.toml: {e}");
                Config::default()
            })
        } else {
            Config::default()
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            routes: RoutesConfig::default(),
            db: DbConfig::default(),
            secrets: SecretsConfig::default(),
        }
    }
}
