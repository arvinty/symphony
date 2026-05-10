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
    let pool = SqlitePoolOptions::new().max_connections(8).connect_with(opts).await?;
    sqlx::query(include_str!("../migrations/0001_init.sql")).execute(&pool).await?;
    sqlx::query(include_str!("../migrations/0002_seed.sql")).execute(&pool).await?;
    sqlx::query(include_str!("../migrations/0003_attachments.sql")).execute(&pool).await?;
    Ok(pool)
}
