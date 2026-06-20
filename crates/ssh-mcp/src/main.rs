mod config;
mod mcp;
mod ssh;

use std::{env, path::PathBuf, sync::Arc};

use anyhow::Result;
use axum::{
    Router,
    routing::{get, post},
};
use config::Config;
use mcp::{AppState, handle_mcp};
use mcp_shared::{health, method_not_allowed};
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG").unwrap_or_else(|_| "ssh_mcp=info,tower_http=warn".to_string()),
        )
        .init();

    let config_path = env::var_os("SSH_MCP_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/ssh-mcp/config.toml"));
    let config = Arc::new(Config::load(config_path)?);
    let state = AppState::new(config.clone());

    let app = Router::new()
        .route("/health", get(health))
        .route(
            "/mcp",
            post(handle_mcp)
                .get(method_not_allowed)
                .delete(method_not_allowed),
        )
        .with_state(state);

    let listener = TcpListener::bind(config.server.bind).await?;
    info!("listening on {}", config.server.bind);
    axum::serve(listener, app).await?;
    Ok(())
}
