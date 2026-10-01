use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use finguard_agent::AgentError;
use finguard_domain::{ErrorCode, HardPolicyReason, TraceId};
use serde_json::json;

/// One consistent error schema; no stack traces, no internals.
#[derive(Debug)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    pub trace_id: TraceId,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>, trace_id: TraceId) -> Self {
        Self {
            code,
            message: message.into(),
            trace_id,
        }
    }
    pub fn invalid(message: impl Into<String>, trace_id: TraceId) -> Self {
        Self::new(ErrorCode::InvalidRequest, message, trace_id)
    }
    pub fn from_agent(e: AgentError, trace_id: TraceId) -> Self {
        Self::new(e.code, e.message, trace_id)
    }
}

pub fn status_for(code: ErrorCode) -> StatusCode {
    use ErrorCode::*;
    match code {
        InvalidRequest => StatusCode::BAD_REQUEST,
        Unauthorized => StatusCode::UNAUTHORIZED,
        Forbidden => StatusCode::FORBIDDEN,
        NotFound => StatusCode::NOT_FOUND,
        UnknownAction => StatusCode::UNPROCESSABLE_ENTITY,
        InsufficientFunds | TransferLimitExceeded | RecipientBlocked | PolicyBlocked => {
            StatusCode::UNPROCESSABLE_ENTITY
        }
        IdempotencyConflict
        | ConfirmationInvalid
        | ConfirmationRequired
        | ClarificationRequired
        | ManualReviewRequired => StatusCode::CONFLICT,
        ConfirmationExpired => StatusCode::GONE,
        JevUnavailable | GroqUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        InternalError => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub fn code_for_reason(r: HardPolicyReason) -> ErrorCode {
    match r {
        HardPolicyReason::InsufficientFunds => ErrorCode::InsufficientFunds,
        HardPolicyReason::TransferLimitExceeded => ErrorCode::TransferLimitExceeded,
        HardPolicyReason::RecipientBlocked => ErrorCode::RecipientBlocked,
        HardPolicyReason::NotAuthenticated => ErrorCode::Unauthorized,
        HardPolicyReason::MissingPermission
        | HardPolicyReason::AccountNotOwned
        | HardPolicyReason::CardNotOwned => ErrorCode::Forbidden,
        _ => ErrorCode::PolicyBlocked,
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({"error": {
            "code": self.code.as_str(),
            "message": self.message,
            "trace_id": self.trace_id,
        }});
        (status_for(self.code), Json(body)).into_response()
    }
}
