//! `risk_questions_v1`: four atomic questions, never one giant prompt. Each
//! asks about one thing and tells the model to use only the supplied state, so
//! it is not invited to hallucinate data we do not have.

use finguard_domain::RiskContext;
use serde_json::{Value, json};

pub const INTENT_ALIGNMENT: &str = "intent_alignment";
pub const BEHAVIORAL_ANOMALY: &str = "behavioral_anomaly";
pub const RECIPIENT_RISK: &str = "recipient_risk";
pub const CONTEXT_SUFFICIENT: &str = "context_sufficient";

pub const ANOMALY_LEVELS: [&str; 5] = [
    "Normal for this customer",
    "Mildly unusual",
    "Elevated",
    "Highly unusual",
    "Extreme",
];
pub const RECIPIENT_LEVELS: [&str; 5] = [
    "Established recipient, low-risk context",
    "Slightly unusual recipient context",
    "Elevated recipient risk",
    "High recipient risk",
    "Extreme recipient risk",
];

/// Which questions are supported by the state we actually have.
pub fn question_ids(ctx: &RiskContext) -> Vec<&'static str> {
    // Without an amount/recipient there is nothing to score for anomaly or
    // recipient risk; asking would only invite guessing.
    if ctx.missing_fields.is_empty() {
        vec![
            INTENT_ALIGNMENT,
            BEHAVIORAL_ANOMALY,
            RECIPIENT_RISK,
            CONTEXT_SUFFICIENT,
        ]
    } else {
        vec![INTENT_ALIGNMENT, CONTEXT_SUFFICIENT]
    }
}

pub fn questions(ctx: &RiskContext) -> Value {
    let mut q = serde_json::Map::new();
    for id in question_ids(ctx) {
        let def = match id {
            INTENT_ALIGNMENT => json!({
                "type": "noul",
                "instructions": "Does the proposed financial action clearly match the user's stated request? Compare `user_request` with `proposed_action` only.",
                "criteria": {
                    "true": "The proposed action is exactly what the user asked for",
                    "false": "The proposed action differs from, or goes beyond, what the user asked for"
                }
            }),
            BEHAVIORAL_ANOMALY => json!({
                "type": "score",
                "instructions": "How unusual is this action for this customer, judged only from the supplied fields (amount_to_typical_ratio, usual_transfer_range_php, transfers_in_last_hour, transactions_in_last_hour, device_trusted, location_familiar)? Do not assume anything that is not in the state.",
                "criteria": ANOMALY_LEVELS
            }),
            RECIPIENT_RISK => json!({
                "type": "score",
                "instructions": "How unusual or risky is this recipient context relative to the supplied user history (recipient_known, recipient_age_days, amount_to_typical_ratio)? Use only the supplied fields.",
                "criteria": RECIPIENT_LEVELS
            }),
            CONTEXT_SUFFICIENT => json!({
                "type": "noul",
                "instructions": "Does the available context contain enough information to safely interpret the proposed action? If `missing_fields` is not empty, the answer is false.",
                "criteria": {
                    "true": "All details needed to interpret the action are present",
                    "false": "Important details (e.g. amount or recipient) are missing or ambiguous"
                }
            }),
            _ => continue,
        };
        q.insert(id.to_owned(), def);
    }
    Value::Object(q)
}

/// Request body. `model` is sent for the official TypeSafe API; JevStation
/// ignores it (it pins the model server-side), so sending it is harmless.
pub fn build_request(ctx: &RiskContext, model: &str) -> Value {
    json!({
        "state": ctx,
        "model": model,
        "questions": questions(ctx),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use finguard_domain::*;

    fn ctx(missing: Vec<MissingField>) -> RiskContext {
        RiskContext {
            action_type: ActionKind::TransferFunds,
            risk_class: RiskClass::MoneyMovement,
            user_request: "Send 2000 to Alex".into(),
            proposed_action: "Transfer PHP 2000.00 to a known recipient".into(),
            amount_php: Some("2000.00".into()),
            currency: Some("PHP".into()),
            amount_to_typical_ratio: Some(0.25),
            recipient_known: true,
            recipient_age_days: Some(240),
            device_trusted: true,
            location_familiar: true,
            transactions_in_last_hour: 0,
            transfers_in_last_hour: 0,
            usual_transfer_range_php: Some("2000.00 - 8000.00".into()),
            missing_fields: missing,
        }
    }

    #[test]
    fn full_context_asks_four_questions() {
        let q = questions(&ctx(vec![]));
        assert_eq!(q.as_object().unwrap().len(), 4);
        assert_eq!(
            q["behavioral_anomaly"]["criteria"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert_eq!(q["intent_alignment"]["type"], "noul");
    }

    #[test]
    fn incomplete_context_does_not_ask_unanswerable_questions() {
        let q = questions(&ctx(vec![MissingField::Amount]));
        let keys: Vec<_> = q.as_object().unwrap().keys().cloned().collect();
        assert!(keys.contains(&"intent_alignment".to_string()));
        assert!(keys.contains(&"context_sufficient".to_string()));
        assert!(!keys.contains(&"behavioral_anomaly".to_string()));
    }

    #[test]
    fn state_contains_only_minimised_fields() {
        let body = build_request(&ctx(vec![]), "jev-latest");
        let s = body["state"].to_string();
        assert!(!s.contains("Maya") && !s.contains("balance") && !s.contains("account"));
        assert_eq!(body["model"], "jev-latest");
    }
}
