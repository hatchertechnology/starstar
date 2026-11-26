use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use starstar_api::auth::AuthStore;
use starstar_api::auth::clock::Clock;
use starstar_api::auth::crypto::new_setup_code;
use starstar_api::auth::password::Hasher;
use starstar_api::auth::startup::check_config;
use starstar_api::config::{Config, redact_database_url};
use starstar_api::prusalink::http_client;
use starstar_api::store::{Db, DbOptions, PrinterStore};
use starstar_api::{AppState, router};

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    println!("shutdown signal received, draining connections");
}

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

    let database_url = config
        .database_url
        .as_deref()
        .expect("check_config verified STARSTAR_DATABASE_URL");
    let db = Db::connect_with_retry(
        database_url,
        DbOptions {
            max_connections: config.db_max_connections,
            connect_retry_timeout: config.db_connect_timeout,
            ..Default::default()
        },
    )
    .await?;

    let state = AppState {
        store: Arc::new(PrinterStore::from_db(db.clone())),
        auth: Arc::new(AuthStore::new(db.clone(), Clock::system(), hasher).await?),
        http: http_client()?,
        config: Arc::new(config.clone()),
    };

    if state.auth.user_count().await? == 0 {
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

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let auth = state.auth.clone();
    let prune_task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(err) = auth.prune().await {
                        eprintln!("auth prune failed: {err}");
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });

    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    println!(
        "starstar api listening on {} (public URL: {}, database: {})",
        config.bind,
        config.public_url,
        redact_database_url(database_url)
    );
    axum::serve(
        listener,
        router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    let _ = shutdown_tx.send(true);
    let _ = prune_task.await;
    db.close().await;
    println!("starstar api stopped");
    Ok(())
}
