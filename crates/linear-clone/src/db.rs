use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

pub async fn open_and_migrate(path: &Path) -> Result<SqlitePool> {
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;

    // `sqlx::migrate!` embeds ./migrations at compile time and applies only the
    // files not already recorded in the `_sqlx_migrations` tracking table — in
    // version order, each exactly once. This replaces the prior approach of
    // re-running every .sql file on every startup, which silently depended on
    // every migration staying hand-idempotent forever (a CREATE TABLE that
    // forgot `IF NOT EXISTS` would crash the second startup).
    //
    // For databases created by the old approach (no `_sqlx_migrations` table),
    // the migrator records all current migrations on first run; because the
    // existing files are idempotent, replaying them against the already-built
    // schema is harmless and the DB self-heals into tracked state.
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
