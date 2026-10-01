//! Reconstructs an action's full lifecycle from append-only audit events and
//! model evaluation records. Nothing here is computed from live state that
//! could have changed since: it is a read of history.

use crate::{Result, actions};
use finguard_domain::*;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use time::OffsetDateTime;

#[derive(Debug, Serialize)]
pub struct EventView {
    pub seq: i64,
    pub event_type: String,
    pub label: String,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
    /// Milliseconds since the first event of the trace.
    pub offset_ms: i64,
    pub duration_ms: Option<i64>,
    pub category: &'static str,
    pub metadata: Value,
}

#[derive(Debug, Serialize)]
pub struct StageView {
    pub key: &'static str,
    pub label: &'static str,
    /// passed | warning | blocked | skipped | executed | pending
    pub status: &'static str,
    pub detail: String,
}

#[derive(Debug, Serialize, Default)]
pub struct LatencyBreakdown {
    pub intent_parsing_ms: i64,
    pub hard_policy_ms: i64,
    pub jev_assessment_ms: i64,
    pub groq_escalation_ms: i64,
    pub db_execution_ms: i64,
}

#[derive(Debug, Serialize, Default)]
pub struct UsageView {
    pub groq_input_tokens: i64,
    pub groq_output_tokens: i64,
    pub jev_credits_charged: Option<i64>,
    pub jev_input_tokens: i64,
    pub jev_output_tokens: i64,
}

#[derive(Debug, Serialize)]
pub struct TraceView {
    pub trace_id: TraceId,
    pub action_id: ActionId,
    pub parent_action_id: Option<ActionId>,
    pub status: String,
    pub action_type: String,
    pub risk_class: String,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub completed_at: Option<OffsetDateTime>,
    pub total_duration_ms: i64,
    pub policy_version: String,
    pub versions: Value,
    pub user_request: String,
    pub hard_policy: Option<Value>,
    pub policy_decision: Option<String>,
    pub decision_reasons: Value,
    pub stages: Vec<StageView>,
    pub events: Vec<EventView>,
    pub jev: Vec<Value>,
    pub groq: Vec<Value>,
    pub latency: LatencyBreakdown,
    pub usage: UsageView,
    pub confirmation: Option<Value>,
    pub ledger: Option<Value>,
    pub result: Option<Value>,
    /// The questions from the Trace Viewer spec, answered from the record.
    pub audit_answers: Vec<(String, String)>,
}

fn label(ev: &str) -> (&'static str, &'static str) {
    // (human label, category)
    match ev {
        "request_received" => ("Request received", "request"),
        "intent_parsing_started" => ("Intent parsing started", "intent"),
        "intent_parsed" => ("Intent parsing completed", "intent"),
        "intent_parse_failed" => ("Intent parsing failed", "intent"),
        "action_created" => ("Typed action created", "action"),
        "authorization_passed" => ("Authorization passed", "policy"),
        "authorization_failed" => ("Authorization failed", "policy"),
        "hard_policy_passed" => ("Hard policy passed", "policy"),
        "hard_policy_blocked" => ("Hard policy BLOCKED", "policy"),
        "jev_skipped" => ("Jev assessment skipped", "jev"),
        "jev_started" => ("Jev evaluation started", "jev"),
        "jev_completed" => ("Jev evaluation completed", "jev"),
        "jev_failed" => ("Jev evaluation failed", "jev"),
        "confidence_escalation_triggered" => ("Confidence escalation triggered", "escalation"),
        "groq_escalation_started" => ("Groq escalation started", "groq"),
        "groq_escalation_completed" => ("Groq escalation completed", "groq"),
        "groq_escalation_failed" => ("Groq escalation failed", "groq"),
        "policy_decision" => ("Policy decision", "policy"),
        "clarification_requested" => ("Clarification requested", "policy"),
        "clarification_answered" => ("Clarification answered; new proposal", "policy"),
        "confirmation_requested" => ("Confirmation requested", "confirmation"),
        "confirmation_received" => ("User confirmation received", "confirmation"),
        "confirmation_rejected" => ("Confirmation rejected", "confirmation"),
        "confirmation_expired" => ("Confirmation expired", "confirmation"),
        "execution_started" => ("Execution started", "execution"),
        "account_row_locked" => ("Account row locked", "database"),
        "balance_revalidated" => ("Invariants revalidated", "database"),
        "ledger_entries_written" => ("Ledger entries written", "database"),
        "transaction_committed" => ("Database transaction committed", "database"),
        "execution_completed" => ("Action executed", "execution"),
        "execution_blocked" => ("Execution blocked at revalidation", "execution"),
        "execution_failed" => ("Execution failed", "execution"),
        "manual_review_required" => ("Held for manual review", "policy"),
        "action_cancelled" => ("Action cancelled", "execution"),
        "idempotent_replay" => ("Duplicate retry; stored result returned", "idempotency"),
        _ => ("Event", "other"),
    }
}

fn has(events: &[EventView], t: &str) -> bool {
    events.iter().any(|e| e.event_type == t)
}
fn find<'a>(events: &'a [EventView], t: &str) -> Option<&'a EventView> {
    events.iter().find(|e| e.event_type == t)
}

pub async fn build(conn: &mut PgConnection, action_id: ActionId) -> Result<TraceView> {
    let action = actions::get(conn, action_id).await?;

    let rows: Vec<(i64, String, OffsetDateTime, Value)> = sqlx::query_as(
        "SELECT seq, event_type, occurred_at, metadata FROM audit_events \
         WHERE trace_id = $1 OR action_id = $2 ORDER BY seq",
    )
    .bind(action.trace_id)
    .bind(action.id)
    .fetch_all(&mut *conn)
    .await?;
    let t0 = rows.first().map(|r| r.2).unwrap_or(action.created_at);
    let events: Vec<EventView> = rows
        .into_iter()
        .map(|(seq, event_type, at, metadata)| {
            let (l, category) = label(&event_type);
            EventView {
                seq,
                label: l.to_owned(),
                category,
                offset_ms: ((at - t0).whole_microseconds() / 1000) as i64,
                duration_ms: metadata.get("duration_ms").and_then(Value::as_i64),
                occurred_at: at,
                event_type,
                metadata,
            }
        })
        .collect();

    // --- model records -------------------------------------------------------
    let jev_rows: Vec<(String, String, Option<String>, Option<i64>, Option<Value>, Option<Value>, Option<Value>, Option<f64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>, String)> =
        sqlx::query_as(
            "SELECT provider, provider_status, model, latency_ms, questions, answers, assessment, overall_confidence, \
             input_tokens, output_tokens, credits_charged, credits_remaining, question_set_version \
             FROM jev_evaluations WHERE action_id = $1 ORDER BY created_at",
        )
        .bind(action.id)
        .fetch_all(&mut *conn)
        .await?;
    let mut usage = UsageView::default();
    let jev: Vec<Value> = jev_rows
        .into_iter()
        .map(|(provider, status, model, latency, questions, answers, assessment, conf, it, ot, cc, cr, qsv)| {
            usage.jev_input_tokens += it.unwrap_or(0);
            usage.jev_output_tokens += ot.unwrap_or(0);
            if let Some(c) = cc {
                usage.jev_credits_charged = Some(usage.jev_credits_charged.unwrap_or(0) + c);
            }
            json!({
                "provider": provider, "status": status, "model": model, "latency_ms": latency,
                "question_set_version": qsv, "questions": questions, "answers": answers,
                "assessment": assessment, "overall_confidence": conf,
                "input_tokens": it, "output_tokens": ot, "credits_charged": cc, "credits_remaining": cr,
            })
        })
        .collect();

    let groq_rows: Vec<(String, String, String, Option<String>, Option<i64>, Option<Value>, Option<i64>, Option<i64>)> =
        sqlx::query_as(
            "SELECT purpose, schema_version, provider_status, model, latency_ms, result, input_tokens, output_tokens \
             FROM groq_evaluations WHERE trace_id = $1 AND (action_id = $2 OR action_id IS NULL) ORDER BY created_at",
        )
        .bind(action.trace_id)
        .bind(action.id)
        .fetch_all(&mut *conn)
        .await?;
    let groq: Vec<Value> = groq_rows
        .into_iter()
        .map(|(purpose, sv, status, model, latency, result, it, ot)| {
            usage.groq_input_tokens += it.unwrap_or(0);
            usage.groq_output_tokens += ot.unwrap_or(0);
            json!({"purpose": purpose, "schema_version": sv, "status": status, "model": model,
                   "latency_ms": latency, "result": result, "input_tokens": it, "output_tokens": ot})
        })
        .collect();

    // --- latency breakdown -----------------------------------------------------
    let dur = |t: &str| find(&events, t).and_then(|e| e.duration_ms).unwrap_or(0);
    let mut latency = LatencyBreakdown {
        intent_parsing_ms: dur("intent_parsed") + dur("intent_parse_failed"),
        hard_policy_ms: dur("hard_policy_passed") + dur("hard_policy_blocked"),
        jev_assessment_ms: dur("jev_completed") + dur("jev_failed"),
        groq_escalation_ms: dur("groq_escalation_completed") + dur("groq_escalation_failed"),
        db_execution_ms: 0,
    };
    if let Some(start) = find(&events, "execution_started") {
        let end = events.iter().rev().find(|e| {
            matches!(
                e.event_type.as_str(),
                "execution_completed" | "execution_blocked"
            )
        });
        if let Some(end) = end {
            latency.db_execution_ms =
                ((end.occurred_at - start.occurred_at).whole_microseconds() / 1000) as i64;
        }
    }

    // --- confirmation / ledger ---------------------------------------------------
    let confirmation = crate::confirmations::get_by_action(conn, action_id, false).await?.map(|c| {
        json!({
            "confirmation_id": ConfirmationId(c.id).to_string(),
            "created_at": c.created_at.format(&time::format_description::well_known::Rfc3339).ok(),
            "expires_at": c.expires_at.format(&time::format_description::well_known::Rfc3339).ok(),
            "used": c.used_at.is_some(),
        })
    });
    let ledger = match action.ledger_transaction_id {
        Some(lt) => {
            let entries: Vec<(String, i64)> = sqlx::query_as(
                "SELECT a.name, e.amount_minor FROM ledger_entries e JOIN accounts a ON a.id = e.account_id \
                 WHERE e.ledger_transaction_id = $1 ORDER BY e.id",
            )
            .bind(lt)
            .fetch_all(&mut *conn)
            .await?;
            Some(json!({
                "ledger_transaction_id": LedgerTransactionId(lt).to_string(),
                "entries": entries.iter().map(|(n, a)| json!({"account": n, "amount_minor": a, "amount": Money::php(*a).pretty()})).collect::<Vec<_>>(),
            }))
        }
        None => None,
    };

    // --- stages -------------------------------------------------------------------
    let status = action.status()?;
    let decision = action.policy_decision.clone().unwrap_or_default();
    let hard_blocked = has(&events, "hard_policy_blocked");
    let jev_ran = has(&events, "jev_completed");
    let jev_failed = has(&events, "jev_failed");
    let escalated = has(&events, "confidence_escalation_triggered");
    let stage = |key, label, status, detail: &str| StageView {
        key,
        label,
        status,
        detail: detail.to_owned(),
    };
    let stages = vec![
        stage(
            "intent",
            "User intent",
            if has(&events, "intent_parsed") {
                "passed"
            } else {
                "warning"
            },
            &action.user_text,
        ),
        stage(
            "action",
            "Proposed action",
            "passed",
            &format!("{} ({})", action.kind, action.risk_class),
        ),
        stage(
            "hard",
            "Hard policy",
            if hard_blocked {
                "blocked"
            } else if has(&events, "hard_policy_passed") {
                "passed"
            } else {
                "pending"
            },
            "",
        ),
        stage(
            "jev",
            "Jev assessment",
            if jev_ran {
                "passed"
            } else if jev_failed {
                "warning"
            } else if hard_blocked || has(&events, "jev_skipped") {
                "skipped"
            } else {
                "pending"
            },
            if hard_blocked {
                "AI assessment skipped: hard policy blocked first (0 Jev calls)"
            } else {
                ""
            },
        ),
        stage(
            "groq",
            "Groq escalation",
            if has(&events, "groq_escalation_completed")
                || has(&events, "groq_escalation_failed")
                || escalated
            {
                "warning"
            } else {
                "skipped"
            },
            if escalated {
                "confidence/ambiguity escalation triggered"
            } else {
                "not triggered"
            },
        ),
        stage(
            "policy",
            "Final policy",
            match decision.as_str() {
                "BLOCK" => "blocked",
                "MANUAL_REVIEW" | "ASK_FOR_CLARIFICATION" => "warning",
                "" => "pending",
                _ => "passed",
            },
            &decision,
        ),
        stage(
            "confirmation",
            "Confirmation",
            if has(&events, "confirmation_received") {
                "passed"
            } else if status == ActionStatus::AwaitingConfirmation {
                "pending"
            } else {
                "skipped"
            },
            "",
        ),
        stage(
            "execution",
            "Execution",
            match status {
                ActionStatus::Executed => "executed",
                ActionStatus::Blocked => "blocked",
                ActionStatus::Executing | ActionStatus::Approved => "pending",
                _ => "skipped",
            },
            &status.to_string(),
        ),
    ];

    // --- answers to the "security value" questions -----------------------------------
    let yn = |b: bool| if b { "Yes" } else { "No" }.to_owned();
    let executed_events = events
        .iter()
        .filter(|e| e.event_type == "execution_completed")
        .count();
    let audit_answers = vec![
        ("What did the user ask?".into(), action.user_text.clone()),
        (
            "What action did the model propose?".into(),
            format!("{} ({})", action.kind, action.risk_class),
        ),
        (
            "What did hard policy check?".into(),
            action
                .hard_policy
                .as_ref()
                .and_then(|h| h.get("checks"))
                .and_then(Value::as_array)
                .map(|c| format!("{} checks", c.len()))
                .unwrap_or_else(|| "not recorded".into()),
        ),
        ("Was Jev invoked?".into(), yn(jev_ran || jev_failed)),
        (
            "What did Jev return?".into(),
            jev.first()
                .and_then(|j| j.get("overall_confidence"))
                .and_then(Value::as_f64)
                .map(|c| format!("overall confidence {:.0}%", c * 100.0))
                .unwrap_or_else(|| "nothing".into()),
        ),
        ("Was Groq escalation triggered?".into(), yn(escalated)),
        (
            "Why did policy choose the final result?".into(),
            action
                .decision_reasons
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|r| r.get("detail").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default(),
        ),
        (
            "Did the user confirm?".into(),
            yn(has(&events, "confirmation_received")),
        ),
        (
            "Did execution happen?".into(),
            yn(status == ActionStatus::Executed),
        ),
        ("Did it happen only once?".into(), yn(executed_events <= 1)),
        (
            "What ledger transaction resulted?".into(),
            action
                .ledger_transaction_id
                .map(|l| LedgerTransactionId(l).to_string())
                .unwrap_or_else(|| "none".into()),
        ),
    ];

    let end = action
        .completed_at
        .or_else(|| events.last().map(|e| e.occurred_at));
    Ok(TraceView {
        trace_id: action.trace(),
        action_id,
        parent_action_id: action.parent_action_id.map(ActionId),
        status: status.to_string(),
        action_type: action.kind.clone(),
        risk_class: action.risk_class.clone(),
        started_at: t0,
        completed_at: action.completed_at,
        total_duration_ms: end
            .map(|e| ((e - t0).whole_microseconds() / 1000) as i64)
            .unwrap_or(0),
        policy_version: action.policy_version.clone(),
        versions: json!({
            "policy": POLICY_VERSION,
            "risk_question_set": RISK_QUESTION_SET_VERSION,
            "intent_schema": INTENT_SCHEMA_VERSION,
            "escalation_schema": ESCALATION_SCHEMA_VERSION,
        }),
        user_request: action.user_text.clone(),
        hard_policy: action.hard_policy.clone(),
        policy_decision: action.policy_decision.clone(),
        decision_reasons: action.decision_reasons.clone(),
        stages,
        events,
        jev,
        groq,
        latency,
        usage,
        confirmation,
        ledger,
        result: action.result.clone(),
        audit_answers,
    })
}
