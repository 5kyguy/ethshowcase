use thiserror::Error;

/// Errors produced by the forklore pipeline.
#[derive(Error, Debug)]
pub enum Error {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("flight parse error: {0}")]
    Flight(String),

    #[error("unexpected http status {status} for {url}")]
    Status { status: u16, url: String },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
