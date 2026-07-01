#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("{0} not found: {1}")]
    NotFound(String, String),
    #[error("Migration error: {0}")]
    Migration(String),
}

impl DbError {
    pub fn ipo_not_found(id: i64) -> Self {
        DbError::NotFound("IPO".into(), id.to_string())
    }

    pub fn market_not_found(id: &str) -> Self {
        DbError::NotFound("Market".into(), id.to_string())
    }
}
