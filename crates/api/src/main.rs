use finguard_api::{build_state, config::Config, logging::RedactingMakeWriter, router};
use finguard_persistence::Store;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // .env is a development convenience; production sets real environment variables.
    let _ = dotenvy::dotenv();

    let cfg = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("configuration error: {e}");
            std::process::exit(2);
        }
    };

    let writer = RedactingMakeWriter::new(cfg.secret_values());
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));
    let fmt = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_target(false);
    if cfg.is_production() {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt.json())
            .init();
    } else {
        tracing_subscriber::registry().with(filter).with(fmt).init();
    }
    tracing::info!(config = ?cfg, "starting finguard-api");

    let store = match Store::connect(&cfg.db).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "cannot connect to the database");
            std::process::exit(1);
        }
    };

    let arg = std::env::args().nth(1);
    if matches!(arg.as_deref(), Some("migrate")) {
        match store.migrate().await {
            Ok(()) => tracing::info!("migrations applied"),
            Err(e) => {
                tracing::error!(error = %e, "migration failed");
                std::process::exit(1);
            }
        }
        return;
    }
    // Serving also applies pending migrations (idempotent), so a fresh
    // `docker compose up` + `cargo run` works without a separate step.
    if let Err(e) = store.migrate().await {
        tracing::error!(error = %e, "migration failed");
        std::process::exit(1);
    }
    if !cfg.is_production() {
        // Seed on first run so the UI has something to show.
        let seeded: Result<(i64,), _> = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(store.pool())
            .await;
        if matches!(seeded, Ok((0,))) {
            match finguard_persistence::seed::reset_demo(store.pool(), &Default::default()).await {
                Ok(()) => tracing::info!("seeded demo data"),
                Err(e) => tracing::warn!(error = %e, "could not seed demo data"),
            }
        }
    }

    let state = match build_state(store, &cfg) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "cannot build application state");
            std::process::exit(1);
        }
    };
    let app = router(state, &cfg.cors_origin, cfg.web_dir.as_deref());
    let listener = match tokio::net::TcpListener::bind(&cfg.bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error = %e, addr = %cfg.bind_addr, "cannot bind");
            std::process::exit(1);
        }
    };
    tracing::info!(addr = %cfg.bind_addr, "listening");
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutting down");
    };
    if let Err(e) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
    {
        tracing::error!(error = %e, "server error");
    }
}
