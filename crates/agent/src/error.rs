use finguard_domain::ErrorCode;
use finguard_persistence::StoreError;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{code:?}: {message}")]
pub struct AgentError {
    pub code: ErrorCode,
    pub message: String,
}

impl AgentError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message)
    }
}

impl From<StoreError> for AgentError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound => {
                Self::new(ErrorCode::NotFound, ErrorCode::NotFound.default_message())
            }
            StoreError::Forbidden => {
                Self::new(ErrorCode::Forbidden, ErrorCode::Forbidden.default_message())
            }
            StoreError::Domain { code, message } => Self::new(code, message),
            StoreError::IllegalState(m) => Self::new(ErrorCode::InvalidRequest, m),
            other => {
                // Never leak SQL/driver details to clients.
                tracing::error!(error = %other, "internal store error");
                Self::new(
                    ErrorCode::InternalError,
                    ErrorCode::InternalError.default_message(),
                )
            }
        }
    }
}

impl From<sqlx::Error> for AgentError {
    fn from(e: sqlx::Error) -> Self {
        StoreError::from(e).into()
    }
}
