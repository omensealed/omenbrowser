pub type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("{0}")]
    Message(String),
    #[error("I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}
