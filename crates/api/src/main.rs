use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

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

    println!(
        "{}",
        starstar_api::auth::startup::trusted_proxies_line(&config)
    );

    let hasher = Hasher::secure();
    // Compute the unknown-user dummy hash now, not on the first bad login.
    hasher.prewarm();
    let db = Db::open(&config.database)?;
    let state = AppState {
        store: Arc::new(PrinterStore::from_db(db.clone())),
        auth: Arc::new(AuthStore::new(db, Clock::system(), hasher)?),
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

    let auth = state.auth.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tick.tick().await;
            if let Err(err) = auth.prune() {
                eprintln!("auth prune failed: {err}");
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    println!(
        "starstar api listening on {} (public URL: {}, database: {})",
        config.bind, config.public_url, config.database
    );
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
