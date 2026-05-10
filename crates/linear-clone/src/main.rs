use anyhow::Result;
use axum::{response::Html, Router};
use clap::Parser;
use linear_clone::{build_router, db, schema::build_schema, AppState};
use std::net::SocketAddr;
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
    #[arg(long, env = "LINEAR_CLONE_WEB", default_value = "web/dist")]
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
    let state = AppState { pool, schema };

    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);
    let api = build_router(state);
    let app: Router = if cli.web_dir.exists() {
        let serve = ServeDir::new(&cli.web_dir).append_index_html_on_directories(true);
        api.fallback_service(serve).layer(cors)
    } else {
        api.fallback(|| async { Html(LANDING) }).layer(cors)
    };

    let addr = SocketAddr::from(([127, 0, 0, 1], cli.port));
    tracing::info!(%addr, "linear_clone_listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
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
