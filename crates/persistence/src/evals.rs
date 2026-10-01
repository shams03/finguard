//! Model audit records. Structured results only — never prompts or secrets.

use crate::Result;
use finguard_domain::*;
use serde_json::{Value, json};
use sqlx::PgConnection;
use uuid::Uuid;

pub struct JevRecord<'a> {
    pub action_id: ActionId,
    pub trace_id: TraceId,
    pub provider: &'a str,
    pub status: &'a str,
    pub assessment: Option<&'a RiskAssessment>,
    pub error: Option<&'a DecisionProviderError>,
    pub latency_ms: u64,
}

pub async fn insert_jev(conn: &mut PgConnection, r: &JevRecord<'_>) -> Result<()> {
    let (questions, answers, meta) = match r.assessment {
        Some(a) => (
            a.raw.get("questions").cloned(),
            a.raw.get("answers").cloned(),
            Some(&a.provider_metadata),
        ),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO jev_evaluations (id, action_id, trace_id, question_set_version, provider, provider_status, model, \
         latency_ms, questions, answers, assessment, overall_confidence, input_tokens, output_tokens, credits_charged, credits_remaining) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
    )
    .bind(Uuid::now_v7())
    .bind(r.action_id.as_uuid())
    .bind(r.trace_id.as_uuid())
    .bind(RISK_QUESTION_SET_VERSION)
    .bind(r.provider)
    .bind(r.status)
    .bind(meta.and_then(|m| m.model.clone()))
    .bind(r.latency_ms as i64)
    .bind(questions)
    .bind(answers)
    .bind(r.assessment.map(|a| {
        // Store the normalised assessment without the bulky raw copy.
        let mut v = serde_json::to_value(a).unwrap_or(Value::Null);
        if let Some(o) = v.as_object_mut() {
            o.remove("raw");
        }
        v
    }).or_else(|| r.error.map(|e| json!({"error": e.code(), "detail": e.to_string()}))))
    .bind(r.assessment.map(|a| a.overall_confidence))
    .bind(meta.and_then(|m| m.input_tokens).map(|v| v as i64))
    .bind(meta.and_then(|m| m.output_tokens).map(|v| v as i64))
    .bind(meta.and_then(|m| m.credits_charged))
    .bind(meta.and_then(|m| m.credits_remaining))
    .execute(conn)
    .await?;
    Ok(())
}

pub struct GroqRecord<'a> {
    pub action_id: Option<ActionId>,
    pub trace_id: TraceId,
    pub purpose: &'a str,
    pub schema_version: &'a str,
    pub status: &'a str,
    pub meta: Option<&'a ModelCallMeta>,
    pub result: Option<Value>,
    pub latency_ms: u64,
}

pub async fn insert_groq(conn: &mut PgConnection, r: &GroqRecord<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO groq_evaluations (id, action_id, trace_id, purpose, schema_version, provider_status, model, latency_ms, \
         result, input_tokens, output_tokens) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
    )
    .bind(Uuid::now_v7())
    .bind(r.action_id.map(|a| a.as_uuid()))
    .bind(r.trace_id.as_uuid())
    .bind(r.purpose)
    .bind(r.schema_version)
    .bind(r.status)
    .bind(r.meta.map(|m| m.model.clone()))
    .bind(r.latency_ms as i64)
    .bind(r.result.clone())
    .bind(r.meta.and_then(|m| m.input_tokens).map(|v| v as i64))
    .bind(r.meta.and_then(|m| m.output_tokens).map(|v| v as i64))
    .execute(conn)
    .await?;
    Ok(())
}
