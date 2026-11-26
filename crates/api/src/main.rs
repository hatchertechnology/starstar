use std::env;
use std::error::Error;
use std::sync::Arc;

use starstar_api::prusalink::http_client;
use starstar_api::store::PrinterStore;
use starstar_api::{AppState, router};

const DEFAULT_DATABASE: &str = "starstar.db";
const DEFAULT_BIND: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let database = env::var("STARSTAR_DATABASE").unwrap_or_else(|_| DEFAULT_DATABASE.to_owned());
    let bind = env::var("STARSTAR_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_owned());

    let state = AppState {
        store: Arc::new(PrinterStore::open(&database)?),
        http: http_client()?,
    };

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("starstar api listening on {bind} (database: {database})");
    axum::serve(listener, router(state)).await?;
    Ok(())
}
