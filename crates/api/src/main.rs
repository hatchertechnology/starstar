use std::error::Error;
use std::sync::Arc;

use starstar_api::config::Config;
use starstar_api::prusalink::http_client;
use starstar_api::store::PrinterStore;
use starstar_api::{AppState, router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env();

    let state = AppState {
        store: Arc::new(PrinterStore::open(&config.database)?),
        http: http_client()?,
        config: Arc::new(config.clone()),
    };

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    println!(
        "starstar api listening on {} (database: {})",
        config.bind, config.database
    );
    axum::serve(listener, router(state)).await?;
    Ok(())
}
