//! JSON views of an action for the API/UI: the typed proposal, the policy
//! decision and a "decision explorer" section assembled from the records.

use crate::error::AgentError;
use finguard_domain::*;
use finguard_persistence::{actions::ActionRow, confirmations::ConfirmationRow};
use finguard_policy::ConfidenceThresholds;
use serde_json::{Value, json};
use sqlx::PgConnection;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

fn ts(t: time::OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

async fn name_of(
    c: &mut PgConnection,
    table: &'static str,
    col: &'static str,
    id: Option<Uuid>,
) -> Result<Option<String>, AgentError> {
    let Some(id) = id else { return Ok(None) };
    // table/col are compile-time constants chosen below, never user input.
    let sql = match (table, col) {
        ("accounts", "name") => "SELECT name FROM accounts WHERE id = $1",
        ("recipients", "display_name") => "SELECT display_name FROM recipients WHERE id = $1",
        ("cards", "last4") => "SELECT last4 FROM cards WHERE id = $1",
        _ => return Ok(None),
    };
    Ok(sqlx::query_as::<_, (String,)>(sql)
        .bind(id)
        .fetch_optional(c)
        .await?
        .map(|r| r.0))
}

pub async fn action_view(
    c: &mut PgConnection,
    a: &ActionRow,
    confirmation: Option<&ConfirmationRow>,
    thresholds: &ConfidenceThresholds,
) -> Result<Value, AgentError> {
    let kind = a.kind()?;
    let class = kind.risk_class();
    let account = name_of(c, "accounts", "name", a.account_id).await?;
    let recipient = name_of(c, "recipients", "display_name", a.recipient_id).await?;
    let card = name_of(c, "cards", "last4", a.card_id).await?;
    let amount = a.amount();

    let proposed = json!({
        "type": kind.as_str(),
        "label": kind.label(),
        "risk_class": class.as_str(),
        "from_account": account,
        "recipient": recipient,
        "card": card.map(|l| format!("•••• {l}")),
        "amount": amount.map(|m| m.to_api_string()),
        "amount_pretty": amount.map(|m| m.pretty()),
        "missing_fields": a.missing_fields,
    });

    // ---- explorer: Jev ---------------------------------------------------
    let jev_row: Option<(String, String, Option<String>, Option<i64>, Option<Value>, Option<f64>, String, Option<i64>, Option<i64>, Option<i64>, Option<i64>)> =
        sqlx::query_as(
            "SELECT provider, provider_status, model, latency_ms, assessment, overall_confidence, question_set_version, \
             credits_charged, credits_remaining, input_tokens, output_tokens \
             FROM jev_evaluations WHERE action_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(a.id)
        .fetch_optional(&mut *c)
        .await?;
    let threshold = thresholds.for_class(class);
    let jev = jev_row.map(|(provider, status, model, latency, assessment, conf, qsv, cc, cr, it, ot)| {
        json!({
            "provider": provider, "status": status, "model": model, "latency_ms": latency,
            "question_set_version": qsv, "overall_confidence": conf, "threshold": threshold,
            "confidence_sufficient": conf.map(|x| x >= threshold),
            "assessment": assessment, "credits_charged": cc, "credits_remaining": cr,
            "input_tokens": it, "output_tokens": ot,
            "note": "Model assessment: probability distribution and confidence. Not a fraud probability.",
        })
    });

    // ---- explorer: escalation ------------------------------------------------
    let esc_event: Option<(Value,)> = sqlx::query_as(
        "SELECT metadata FROM audit_events WHERE action_id = $1 AND event_type = 'confidence_escalation_triggered' ORDER BY seq LIMIT 1",
    )
    .bind(a.id)
    .fetch_optional(&mut *c)
    .await?;
    let groq_row: Option<(String, String, Option<String>, Option<i64>, Option<Value>, Option<i64>, Option<i64>)> = sqlx::query_as(
        "SELECT purpose, provider_status, model, latency_ms, result, input_tokens, output_tokens FROM groq_evaluations \
         WHERE action_id = $1 AND purpose = 'ESCALATION_ANALYSIS' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(a.id)
    .fetch_optional(&mut *c)
    .await?;
    let escalation = esc_event.map(|(meta,)| {
        json!({
            "triggered": true,
            "reason": meta.get("reason"), "confidence": meta.get("confidence"), "required_threshold": meta.get("threshold"),
            "groq": groq_row.map(|(purpose, status, model, latency, result, it, ot)| json!({
                "purpose": "Clarification analysis", "raw_purpose": purpose, "status": status, "model": model,
                "latency_ms": latency, "result": result, "input_tokens": it, "output_tokens": ot,
            })),
        })
    });

    let clarification = a.clarification_question.as_ref().map(|q| {
        json!({
            "question": q,
            "suggested_amount_minor": a.clarification_candidate_minor,
            "suggested_amount": a.clarification_candidate_minor.map(|m| Money::php(m).pretty()),
        })
    });

    Ok(json!({
        "action_id": a.action_id(),
        "trace_id": a.trace(),
        "parent_action_id": a.parent_action_id.map(ActionId),
        "user_request": a.user_text,
        "proposed_action": proposed,
        "status": a.status,
        "policy_decision": a.policy_decision,
        "decision_reasons": a.decision_reasons,
        "clarification": clarification,
        "confirmation": confirmation.map(|c| json!({
            "confirmation_id": ConfirmationId(c.id),
            "expires_at": ts(c.expires_at),
            "used": c.used_at.is_some(),
        })),
        "result": a.result,
        "explorer": {
            "hard_policy": a.hard_policy,
            "jev": jev,
            "escalation": escalation.unwrap_or_else(|| json!({"triggered": false})),
            "policy_version": a.policy_version,
            "versions": {
                "policy": POLICY_VERSION, "risk_question_set": RISK_QUESTION_SET_VERSION,
                "intent_schema": INTENT_SCHEMA_VERSION, "escalation_schema": ESCALATION_SCHEMA_VERSION,
            },
        },
        "created_at": ts(a.created_at),
        "updated_at": ts(a.updated_at),
    }))
}
