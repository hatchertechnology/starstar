use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use starstar_api::auth::AuthStore;
use starstar_api::auth::clock::Clock;
use starstar_api::auth::crypto::new_setup_code;
use starstar_api::auth::password::Hasher;
use starstar_api::auth::startup::check_config;
use starstar_api::config::Config;
use starstar_api::prusalink::http_client;
use starstar_api::store::{Db, PrinterStore};
use starstar_api::{AppState, router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env();

    match check_config(&config) {
        Ok(Some(warning)) => eprintln!("{warning}"),
        Ok(None) => {}
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }

    let db = Db::open(&config.database)?;
    let state = AppState {
        store: Arc::new(PrinterStore::from_db(db.clone())),
        auth: Arc::new(AuthStore::new(db, Clock::system(), Hasher::secure())?),
        http: http_client()?,
        config: Arc::new(config.clone()),
    };

    if state.auth.user_count()? == 0 {
        match &config.setup_token {
            Some(token) => {
                state.auth.set_setup_code(Some(token.clone()));
                println!(
                    "no users yet: create the first admin with the setup code from STARSTAR_SETUP_TOKEN"
                );
            }
            None => {
                let code = new_setup_code();
                state.auth.set_setup_code(Some(code.clone()));
                println!(
                    "no users yet: create the first admin at {}/setup with setup code {code}",
                    config.public_url
                );
            }
        }
    }

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    println!(
        "starstar api listening on {} (database: {})",
        config.bind, config.database
    );
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
