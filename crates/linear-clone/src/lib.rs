pub mod auth;
pub mod db;
pub mod schema;
pub mod rest;

use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use auth::TokenStore;
use axum::{
    extract::State,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use schema::{build_schema, AppSchema};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub schema: AppSchema,
    pub token_store: TokenStore,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/graphql", post(graphql_handler))
        .route("/graphql", get(graphql_playground))
        .route("/api/health", get(|| async { "ok" }))
        .route("/admin/tokens", post(auth::admin_mint_token))
        .nest("/api", rest::router())
        .with_state(state)
}

async fn graphql_handler(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let bound = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .and_then(|t| state.token_store.lookup(t.trim()));
    let ctx = auth::AuthCtx { bound_issue: bound };
    state.schema.execute(req.into_inner().data(ctx)).await.into()
}

async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new("/graphql")))
}

/// Test helpers
pub async fn init_pool_for_test() -> SqlitePool {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .expect("pool");
    sqlx::query(include_str!("../migrations/0001_init.sql"))
        .execute(&pool)
        .await
        .expect("migrate");
    sqlx::query(include_str!("../migrations/0002_seed.sql"))
        .execute(&pool)
        .await
        .expect("seed");
    sqlx::query(include_str!("../migrations/0003_attachments.sql"))
        .execute(&pool)
        .await
        .expect("migrate attachments");
    pool
}

pub fn build_router_for_test(pool: SqlitePool) -> Router {
    build_router_for_test_with_auth(pool, TokenStore::default())
}

pub fn build_router_for_test_with_auth(pool: SqlitePool, token_store: TokenStore) -> Router {
    let schema = build_schema(pool.clone());
    build_router(AppState { pool, schema, token_store })
}
