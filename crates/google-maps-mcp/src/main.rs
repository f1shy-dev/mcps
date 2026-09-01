mod config;
mod google;
mod mcp;
mod store;

use std::{env, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Router,
    routing::{get, post},
};
use config::Config;
use google::GoogleClient;
use mcp::{AppState, handle_mcp};
use mcp_shared::{health, method_not_allowed};
use store::Store;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG")
                .unwrap_or_else(|_| "google_maps_mcp=info,tower_http=warn".to_string()),
        )
        .init();

    let config_path = env::var_os("GOOGLE_MAPS_MCP_CONFIG").map(PathBuf::from);
    let mut config = Config::load(config_path)?;
    if let Some(port) = env::var_os("PORT") {
        let port = port
            .to_str()
            .context("PORT must be valid Unicode")?
            .parse::<u16>()
            .context("PORT must be a valid TCP port")?;
        config.server.bind = ([0, 0, 0, 0, 0, 0, 0, 0], port).into();
    }
    let config = Arc::new(config);
    let store = Arc::new(Store::open(&config)?);
    let google = Arc::new(GoogleClient::new(config.clone(), store.clone())?);
    let state = AppState::new(config.clone(), google);

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
