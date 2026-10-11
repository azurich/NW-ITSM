//! NW-ITSM backend: the `/api/v2` HTTP API and the host of the React
//! application, running next to the legacy PHP application on the same
//! database (see `docs/adr/0001-migration-progressive.md`).

pub mod bridge;
pub mod client;
pub mod config;
pub mod error;
pub mod openapi;
pub mod routes;
pub mod session;
pub mod state;
pub mod throttle;

use anyhow::Context;
use axum::{
    Router,
    http::{HeaderValue, header},
    response::Redirect,
    routing::get,
};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use tracing_subscriber::EnvFilter;

use crate::{config::Config, state::AppState};

/// Builds the application router.
pub fn router(state: AppState, config: &Config) -> Router {
    let api = routes::api().layer(SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    ));

    let mut app = Router::new().nest("/api/v2", api);
    if let Some(dir) = &config.static_dir {
        // Client-side routing: unknown paths get the application shell.
        let spa = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));
        app = app
            .route("/app", get(|| async { Redirect::permanent("/app/") }))
            .nest_service("/app/", spa);
    }

    app.layer(SetResponseHeaderLayer::overriding(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    ))
    .layer(TraceLayer::new_for_http())
    .with_state(state)
}

/// Entry point of the `nw-server` binary.
pub async fn run() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        print!("{}", openapi::json());
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();

    let config = Config::from_env()?;
    let pool = nw_db::connect(&config.database_url, config.database_max_connections)
        .await
        .context("cannot connect to the database")?;
    let state = AppState::new(pool, &config);

    let listener = tokio::net::TcpListener::bind(config.bind_address)
        .await
        .with_context(|| format!("cannot listen on {}", config.bind_address))?;
    tracing::info!(address = %config.bind_address, "NW-ITSM API listening");
    let app = router(state, &config).into_make_service_with_connect_info::<std::net::SocketAddr>();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutting down");
}
