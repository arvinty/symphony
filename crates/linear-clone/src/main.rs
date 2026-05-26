use anyhow::Result;
use axum::{response::Html, Router};
use clap::Parser;
use linear_clone::{build_router, db, schema::build_schema, AppState};
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Debug, Parser)]
#[command(name = "linear-clone", about = "Linear-shaped issue tracker")]
struct Cli {
    #[arg(long, env = "LINEAR_CLONE_DB", default_value = "linear-clone.db")]
    db: PathBuf,
    #[arg(long, default_value_t = 4000)]
    port: u16,
    /// Bind address. Defaults to loopback; set to 0.0.0.0 to accept off-host
    /// traffic (e.g. when running in a container).
    #[arg(long, env = "LINEAR_CLONE_HOST", default_value = "127.0.0.1")]
    host: IpAddr,
    #[arg(
        long,
        env = "LINEAR_CLONE_WEB",
        default_value = "crates/linear-clone/static"
    )]
    web_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer().with_target(true))
        .init();

    let cli = Cli::parse();
    let pool = db::open_and_migrate(&cli.db).await?;
    let schema = build_schema(pool.clone());
    let state = AppState {
        pool,
        schema,
        token_store: linear_clone::auth::TokenStore::default(),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let api = build_router(state);
    let app: Router = if cli.web_dir.exists() {
        // The web UI uses BrowserRouter, so client-side routes (/board,
        // /issue/ENG-1, …) aren't real files. Built assets all live under
        // /assets, served by ServeDir (which 404s correctly for a genuinely
        // missing asset). Every other unmatched path falls back to index.html
        // with a 200, so a deep-link or refresh boots the app instead of
        // 404ing. API routes are matched by `api` first, so they're unaffected.
        let index_html =
            std::fs::read_to_string(cli.web_dir.join("index.html")).unwrap_or_default();
        api.nest_service("/assets", ServeDir::new(cli.web_dir.join("assets")))
            .fallback(move || {
                let html = index_html.clone();
                async move { Html(html) }
            })
            .layer(cors)
    } else {
        api.fallback(|| async { Html(LANDING) }).layer(cors)
    };

    let addr = SocketAddr::from((cli.host, cli.port));
    tracing::info!(%addr, "linear_clone_listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("linear_clone_shutdown_complete");
    Ok(())
}

/// Resolves when the process receives SIGINT or (on Unix) SIGTERM. SIGTERM is
/// what container runtimes / systemd send on stop; handling it lets axum drain
/// in-flight requests instead of the process being hard-killed.
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
            Err(e) => {
                tracing::warn!("sigterm_handler_install_failed: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("sigint_received"),
        _ = terminate => tracing::info!("sigterm_received"),
    }
}

const LANDING: &str = r#"<!doctype html><meta charset=utf-8><title>Linear Clone</title>
<style>body{font-family:ui-sans-serif,system-ui;background:#0c0d12;color:#e6e6f0;padding:32px}
a{color:#a8a8ff}</style>
<h1>Linear Clone</h1>
<p>The web UI hasn't been built yet. Run <code>cd web && npm install && npm run build</code>.</p>
<ul>
  <li><a href=/graphql>GraphQL Playground</a></li>
  <li><a href=/api/health>Health</a></li>
</ul>
"#;
