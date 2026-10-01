//! Append-only audit events. Metadata must be *safe*: no secrets, no raw
//! provider payloads, no unnecessary user text.

use crate::Result;
use finguard_domain::{ActionId, TraceId};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Event type names (kept as constants so typos are compile errors).
pub mod ev {
    pub const REQUEST_RECEIVED: &str = "request_received";
    pub const INTENT_PARSING_STARTED: &str = "intent_parsing_started";
    pub const INTENT_PARSED: &str = "intent_parsed";
    pub const INTENT_PARSE_FAILED: &str = "intent_parse_failed";
    pub const ACTION_CREATED: &str = "action_created";
    pub const AUTHORIZATION_PASSED: &str = "authorization_passed";
    pub const AUTHORIZATION_FAILED: &str = "authorization_failed";
    pub const HARD_POLICY_PASSED: &str = "hard_policy_passed";
    pub const HARD_POLICY_BLOCKED: &str = "hard_policy_blocked";
    pub const JEV_SKIPPED: &str = "jev_skipped";
    pub const JEV_STARTED: &str = "jev_started";
    pub const JEV_COMPLETED: &str = "jev_completed";
    pub const JEV_FAILED: &str = "jev_failed";
    pub const CONFIDENCE_ESCALATION_TRIGGERED: &str = "confidence_escalation_triggered";
    pub const GROQ_ESCALATION_STARTED: &str = "groq_escalation_started";
    pub const GROQ_ESCALATION_COMPLETED: &str = "groq_escalation_completed";
    pub const GROQ_ESCALATION_FAILED: &str = "groq_escalation_failed";
    pub const POLICY_DECISION: &str = "policy_decision";
    pub const CLARIFICATION_REQUESTED: &str = "clarification_requested";
    pub const CLARIFICATION_ANSWERED: &str = "clarification_answered";
    pub const CONFIRMATION_REQUESTED: &str = "confirmation_requested";
    pub const CONFIRMATION_RECEIVED: &str = "confirmation_received";
    pub const CONFIRMATION_REJECTED: &str = "confirmation_rejected";
    pub const CONFIRMATION_EXPIRED: &str = "confirmation_expired";
    pub const EXECUTION_STARTED: &str = "execution_started";
    pub const ACCOUNT_ROW_LOCKED: &str = "account_row_locked";
    pub const BALANCE_REVALIDATED: &str = "balance_revalidated";
    pub const LEDGER_ENTRIES_WRITTEN: &str = "ledger_entries_written";
    pub const TRANSACTION_COMMITTED: &str = "transaction_committed";
    pub const EXECUTION_COMPLETED: &str = "execution_completed";
    pub const EXECUTION_BLOCKED: &str = "execution_blocked";
    pub const EXECUTION_FAILED: &str = "execution_failed";
    pub const MANUAL_REVIEW_REQUIRED: &str = "manual_review_required";
    pub const ACTION_CANCELLED: &str = "action_cancelled";
    pub const IDEMPOTENT_REPLAY: &str = "idempotent_replay";
}

pub async fn record(
    conn: &mut PgConnection,
    trace_id: TraceId,
    action_id: Option<ActionId>,
    event_type: &str,
    metadata: Value,
) -> Result<()> {
    // clock_timestamp() (default on the column) is wall-clock *now*, not the
    // transaction start, so events inside one transaction keep real ordering.
    sqlx::query(
        "INSERT INTO audit_events (event_id, trace_id, action_id, event_type, metadata) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(trace_id.as_uuid())
    .bind(action_id.map(|a| a.as_uuid()))
    .bind(event_type)
    .bind(metadata)
    .execute(conn)
    .await?;
    Ok(())
}
