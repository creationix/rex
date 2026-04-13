mod server;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rex-serve", version, about = "Rex edge function server")]
struct Cli {
    /// Project root directory (contains rex-serve.toml and routes/)
    #[arg(short, long, default_value = ".")]
    dir: PathBuf,

    /// Override port
    #[arg(short, long)]
    port: Option<u16>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let project_root = cli.dir.canonicalize().unwrap_or_else(|_| {
        eprintln!("error: directory not found: {}", cli.dir.display());
        std::process::exit(1);
    });

    let mut config = rex_serve::config::Config::load(&project_root);

    if let Some(port) = cli.port {
        config.server.port = port;
    }

    server::run(config, project_root).await;
}
