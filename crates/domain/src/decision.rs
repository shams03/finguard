//! Outputs of the deterministic policy layers. These are plain data so they
//! can be persisted, traced and rendered by the Decision Explorer.

use serde::{Deserialize, Serialize};

/// Result of one named hard-policy check (rendered as PASS / FAIL / SKIPPED).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckStatus {
    Pass,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HardPolicyReason {
    NotAuthenticated,
    MissingPermission,
    AccountNotOwned,
    CardNotOwned,
    AccountInactive,
    ActionNotAllowed,
    InvalidAmount,
    UnsupportedCurrency,
    RecipientNotFound,
    RecipientBlocked,
    InsufficientFunds,
    TransferLimitExceeded,
    CardStateInvalid,
    InvalidIdempotency,
}

impl HardPolicyReason {
    pub fn code(self) -> &'static str {
        match self {
            HardPolicyReason::NotAuthenticated => "NOT_AUTHENTICATED",
            HardPolicyReason::MissingPermission => "MISSING_PERMISSION",
            HardPolicyReason::AccountNotOwned => "ACCOUNT_NOT_OWNED",
            HardPolicyReason::CardNotOwned => "CARD_NOT_OWNED",
            HardPolicyReason::AccountInactive => "ACCOUNT_INACTIVE",
            HardPolicyReason::ActionNotAllowed => "ACTION_NOT_ALLOWED",
            HardPolicyReason::InvalidAmount => "INVALID_AMOUNT",
            HardPolicyReason::UnsupportedCurrency => "UNSUPPORTED_CURRENCY",
            HardPolicyReason::RecipientNotFound => "RECIPIENT_NOT_FOUND",
            HardPolicyReason::RecipientBlocked => "RECIPIENT_BLOCKED",
            HardPolicyReason::InsufficientFunds => "INSUFFICIENT_FUNDS",
            HardPolicyReason::TransferLimitExceeded => "TRANSFER_LIMIT_EXCEEDED",
            HardPolicyReason::CardStateInvalid => "CARD_STATE_INVALID",
            HardPolicyReason::InvalidIdempotency => "INVALID_IDEMPOTENCY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "result",
    content = "reason",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
pub enum HardPolicyResult {
    Pass,
    Block(HardPolicyReason),
}

impl HardPolicyResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, HardPolicyResult::Pass)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardCheck {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
    pub reason: Option<HardPolicyReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardPolicyReport {
    pub checks: Vec<HardCheck>,
    pub result: HardPolicyResult,
}

/// Why final policy chose Block. Deterministic and enumerable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlockReason {
    HardPolicy(HardPolicyReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyDecision {
    Execute,
    RequireConfirmation,
    AskForClarification { question: String },
    ManualReview,
    Block { reason: BlockReason },
}

impl PolicyDecision {
    pub fn code(&self) -> &'static str {
        match self {
            PolicyDecision::Execute => "EXECUTE",
            PolicyDecision::RequireConfirmation => "REQUIRE_CONFIRMATION",
            PolicyDecision::AskForClarification { .. } => "ASK_FOR_CLARIFICATION",
            PolicyDecision::ManualReview => "MANUAL_REVIEW",
            PolicyDecision::Block { .. } => "BLOCK",
        }
    }
}
