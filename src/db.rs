use std::str::FromStr;

use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS study_cards (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chunk TEXT NOT NULL,
    surface_chunk TEXT NOT NULL,
    context_sentence TEXT NOT NULL,
    korean_meaning TEXT NOT NULL,
    extra_examples_json TEXT NOT NULL DEFAULT '[]',
    nuance_note TEXT NOT NULL DEFAULT '',
    tags_json TEXT NOT NULL DEFAULT '[]',
    srs_stage INTEGER NOT NULL DEFAULT 0,
    due_date TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_reviewed_at TEXT,
    review_count INTEGER NOT NULL DEFAULT 0,
    lapse_count INTEGER NOT NULL DEFAULT 0,
    source_type TEXT NOT NULL DEFAULT 'manual',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS review_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id INTEGER NOT NULL REFERENCES study_cards(id) ON DELETE CASCADE,
    rating TEXT NOT NULL,
    mode TEXT NOT NULL DEFAULT 'standard',
    user_input TEXT NOT NULL DEFAULT '',
    ai_feedback TEXT NOT NULL DEFAULT '',
    reviewed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
"#;

pub async fn init_db(database_url: &str) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

pub async fn init_memory_db() -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::raw_sql(SCHEMA_SQL).execute(pool).await?;
    Ok(())
}
