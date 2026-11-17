use thiserror::Error;

#[derive(Debug, Error)]
pub enum IrisError {
    #[error("screen not found: {0}")]
    ScreenNotFound(String),

    #[error("note not found: {0}")]
    NoteNotFound(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("persistence error: {0}")]
    Persistence(String),

    #[error("render error: {0}")]
    Render(String),

    #[error("mqtt error: {0}")]
    Mqtt(String),
}

pub type Result<T> = std::result::Result<T, IrisError>;
