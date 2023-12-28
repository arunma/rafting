use thiserror::Error;

pub type RaftResult<T> = Result<T, RaftError>;

#[derive(Error, Debug)]
pub enum RaftError {
    #[error("{0}")]
    ApplicationStartup(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("unexpected error has occurred")]
    InternalServerError,
    #[error("{0}")]
    InternalServerErrorWithContext(String),
    #[error(transparent)]
    AnyhowError(#[from] anyhow::Error),
}