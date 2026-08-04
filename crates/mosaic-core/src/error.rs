//! Error type shared by mosaic-core and mosaic-scrapers.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("config error: {0}")]
    Config(String),

    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("rate limited, retry after {0:?}")]
    RateLimited(std::time::Duration),

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn parse(msg: impl Into<String>) -> Self {
        Error::Parse(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
