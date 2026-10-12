//! Durable store access for `jarvis-core`.
//!
//! PostgreSQL is the durable state (ADR-0003). All schema changes go through
//! `migrations/` and are applied by [`migrate`]. The initial deployment is
//! loopback/local with trust authentication; transport TLS is a deployment
//! concern added with remote clients (Phase 6), so SSL is disabled explicitly
//! here rather than guessed.

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;
use std::time::Duration;

/// Build a pool from a `postgres://…` URL with SSL mode disabled.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let opts =
        PgConnectOptions::from_str(database_url)?.ssl_mode(sqlx::postgres::PgSslMode::Disable);
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(opts)
        .await
}

/// Apply embedded migrations from `migrations/`.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("../../migrations").run(pool).await
}
