use serde::{Deserialize, Serialize};

/// Stable, client-visible error codes (one consistent error schema).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidRequest,
    Unauthorized,
    Forbidden,
    UnknownAction,
    InsufficientFunds,
    TransferLimitExceeded,
    RecipientBlocked,
    IdempotencyConflict,
    ConfirmationRequired,
    ConfirmationExpired,
    ConfirmationInvalid,
    ClarificationRequired,
    ManualReviewRequired,
    PolicyBlocked,
    JevUnavailable,
    GroqUnavailable,
    NotFound,
    InternalError,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        use ErrorCode::*;
        match self {
            InvalidRequest => "INVALID_REQUEST",
            Unauthorized => "UNAUTHORIZED",
            Forbidden => "FORBIDDEN",
            UnknownAction => "UNKNOWN_ACTION",
            InsufficientFunds => "INSUFFICIENT_FUNDS",
            TransferLimitExceeded => "TRANSFER_LIMIT_EXCEEDED",
            RecipientBlocked => "RECIPIENT_BLOCKED",
            IdempotencyConflict => "IDEMPOTENCY_CONFLICT",
            ConfirmationRequired => "CONFIRMATION_REQUIRED",
            ConfirmationExpired => "CONFIRMATION_EXPIRED",
            ConfirmationInvalid => "CONFIRMATION_INVALID",
            ClarificationRequired => "CLARIFICATION_REQUIRED",
            ManualReviewRequired => "MANUAL_REVIEW_REQUIRED",
            PolicyBlocked => "POLICY_BLOCKED",
            JevUnavailable => "JEV_UNAVAILABLE",
            GroqUnavailable => "GROQ_UNAVAILABLE",
            NotFound => "NOT_FOUND",
            InternalError => "INTERNAL_ERROR",
        }
    }

    pub fn default_message(self) -> &'static str {
        use ErrorCode::*;
        match self {
            InvalidRequest => "The request was malformed or missing required fields.",
            Unauthorized => "Authentication is required.",
            Forbidden => "The actor is not permitted to perform this action.",
            UnknownAction => "The requested action is not an allow-listed tool.",
            InsufficientFunds => "The account does not have sufficient available funds.",
            TransferLimitExceeded => "The transfer exceeds the absolute per-transfer limit.",
            RecipientBlocked => "The recipient is blocked.",
            IdempotencyConflict => {
                "This idempotency key was already used with a different request."
            }
            ConfirmationRequired => "Explicit user confirmation is required.",
            ConfirmationExpired => "The confirmation has expired.",
            ConfirmationInvalid => {
                "The confirmation is invalid, already used, or does not match the action."
            }
            ClarificationRequired => "More information is required before this action can proceed.",
            ManualReviewRequired => "The action was held for manual review. No funds moved.",
            PolicyBlocked => "Policy blocked this action.",
            JevUnavailable => "The risk assessment provider is unavailable.",
            GroqUnavailable => "The language model provider is unavailable.",
            NotFound => "The requested resource was not found.",
            InternalError => "An internal error occurred.",
        }
    }
}
