use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("settings error: {0}")]
    Settings(String),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("unsupported feature: {0}")]
    Unsupported(String),
    #[error("browser error: {0}")]
    Browser(String),
    #[error("micron parse error: {0}")]
    Micron(String),
}

pub type AppResult<T> = Result<T, AppError>;
