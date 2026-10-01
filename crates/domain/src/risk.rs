//! Risk evidence: the minimised context we are willing to show a model vendor,
//! the vendor-neutral assessment we normalise answers into, and the
//! `DecisionProvider` trait implemented by Jev (real) and a scripted mock.

use crate::{
    action::{ActionKind, MissingField, RiskClass},
    money::Money,
    redact::scrub_text,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Everything we know, loaded from the database. **Never sent to a vendor.**
/// It exists so the minimisation step (`RiskContext::from_facts`) is explicit
/// and testable.
#[derive(Debug, Clone)]
pub struct RiskFacts {
    pub action_kind: ActionKind,
    pub user_request: String,
    pub amount: Option<Money>,
    pub recipient_known: bool,
    pub recipient_age_days: Option<i64>,
    pub recipient_display_name: Option<String>,
    pub device_trusted: bool,
    pub location_familiar: bool,
    pub transactions_last_hour: u32,
    pub transfers_last_hour: u32,
    pub usual_transfer_min: Option<Money>,
    pub usual_transfer_max: Option<Money>,
    pub missing_fields: Vec<MissingField>,
    // Fields below are deliberately dropped by `from_facts`:
    pub customer_name: String,
    pub account_number: String,
    pub balance: Option<Money>,
}

/// Minimised risk state. Serialised as-is into the Jev `state`, so *adding a
/// field here is the only way to expose new data to a vendor* — a reviewable
/// allow-list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskContext {
    pub action_type: ActionKind,
    pub risk_class: RiskClass,
    /// Original request with emails / long digit runs / secrets scrubbed.
    pub user_request: String,
    pub proposed_action: String,
    pub amount_php: Option<String>,
    pub currency: Option<String>,
    pub amount_to_typical_ratio: Option<f64>,
    pub recipient_known: bool,
    pub recipient_age_days: Option<i64>,
    pub device_trusted: bool,
    pub location_familiar: bool,
    pub transactions_in_last_hour: u32,
    pub transfers_in_last_hour: u32,
    pub usual_transfer_range_php: Option<String>,
    pub missing_fields: Vec<MissingField>,
}

impl RiskContext {
    pub fn from_facts(f: &RiskFacts) -> Self {
        let ratio = match (f.amount, f.usual_transfer_max) {
            (Some(a), Some(max)) if max.amount_minor > 0 && a.currency == max.currency => {
                // round to 2dp so the ratio is stable and not falsely precise
                Some((a.amount_minor as f64 / max.amount_minor as f64 * 100.0).round() / 100.0)
            }
            _ => None,
        };
        let proposed_action = match (f.action_kind, f.amount) {
            (ActionKind::TransferFunds, Some(a)) => format!(
                "Transfer {} to a {} recipient",
                a.to_api_string(),
                if f.recipient_known { "known" } else { "new" }
            ),
            (ActionKind::TransferFunds, None) => {
                "Transfer of an unspecified amount to a recipient".to_owned()
            }
            (k, _) => k.label().to_owned(),
        };
        RiskContext {
            action_type: f.action_kind,
            risk_class: f.action_kind.risk_class(),
            user_request: scrub_text(&f.user_request, 280),
            proposed_action,
            amount_php: f.amount.map(|a| a.plain_major()),
            currency: f.amount.map(|a| a.currency.code().to_owned()),
            amount_to_typical_ratio: ratio,
            recipient_known: f.recipient_known,
            recipient_age_days: f.recipient_age_days,
            device_trusted: f.device_trusted,
            location_familiar: f.location_familiar,
            transactions_in_last_hour: f.transactions_last_hour,
            transfers_in_last_hour: f.transfers_last_hour,
            usual_transfer_range_php: match (f.usual_transfer_min, f.usual_transfer_max) {
                (Some(a), Some(b)) => Some(format!("{} - {}", a.plain_major(), b.plain_major())),
                _ => None,
            },
            missing_fields: f.missing_fields.clone(),
        }
    }
}

/// A yes/no style assessment (from a Jev `Noul` answer).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ProbabilityAssessment {
    /// P(yes) in 0..=1
    pub yes: f64,
    /// Certainty of the yes/no call: `|2p - 1|`. 0 when the model is on the
    /// fence (p = 0.5), 1 when it is certain either way. Jev's Noul answers
    /// carry no confidence field, so this is *our* derived measure.
    pub confidence: f64,
}

impl ProbabilityAssessment {
    pub fn from_yes_probability(p: f64) -> Self {
        let p = p.clamp(0.0, 1.0);
        Self {
            yes: p,
            confidence: (2.0 * p - 1.0).abs(),
        }
    }
    pub fn no(&self) -> f64 {
        1.0 - self.yes
    }
}

/// An ordered-rubric assessment (from a Jev `Score` answer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredAssessment {
    /// Probability-weighted level; can be fractional (e.g. 2.4).
    pub score: f64,
    pub labels: Vec<String>,
    /// Probability per level, same order as `labels`.
    pub probabilities: Vec<f64>,
    /// Jev's own confidence for this answer.
    pub confidence: f64,
}

impl ScoredAssessment {
    pub fn max_level(&self) -> usize {
        self.labels.len().saturating_sub(1)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub provider: String,
    pub model: Option<String>,
    /// Measured client-side around the HTTP call (we do not trust a vendor
    /// figure when we can measure).
    pub latency_ms: u64,
    pub question_set_version: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub credits_charged: Option<i64>,
    pub credits_remaining: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub intent_alignment: Option<ProbabilityAssessment>,
    pub behavioral_anomaly: Option<ScoredAssessment>,
    pub recipient_risk: Option<ScoredAssessment>,
    pub context_sufficient: Option<ProbabilityAssessment>,
    /// Weakest-link confidence over the answers present. This is *not* a
    /// fraud probability and *not* an authorisation signal.
    pub overall_confidence: f64,
    pub provider_metadata: ProviderMetadata,
    /// Questions and answers as exchanged (contains no secrets) for audit.
    pub raw: serde_json::Value,
}

impl RiskAssessment {
    pub fn weakest_link_confidence(&self) -> f64 {
        let mut c: Vec<f64> = vec![];
        if let Some(a) = &self.intent_alignment {
            c.push(a.confidence);
        }
        if let Some(a) = &self.context_sufficient {
            c.push(a.confidence);
        }
        if let Some(a) = &self.behavioral_anomaly {
            c.push(a.confidence);
        }
        if let Some(a) = &self.recipient_risk {
            c.push(a.confidence);
        }
        c.into_iter().fold(1.0_f64, f64::min)
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecisionProviderError {
    #[error("decision provider rejected our credentials")]
    Unauthorized,
    #[error("decision provider rate limited us")]
    RateLimited { retry_after_ms: Option<u64> },
    #[error("decision provider timed out")]
    Timeout,
    #[error("decision provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("decision provider returned an invalid response: {0}")]
    InvalidResponse(String),
}

impl DecisionProviderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "UNAUTHORIZED",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::Timeout => "TIMEOUT",
            Self::ProviderUnavailable(_) => "PROVIDER_UNAVAILABLE",
            Self::InvalidResponse(_) => "INVALID_RESPONSE",
        }
    }
}

#[async_trait]
pub trait DecisionProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn assess(&self, context: &RiskContext) -> Result<RiskAssessment, DecisionProviderError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> RiskFacts {
        RiskFacts {
            action_kind: ActionKind::TransferFunds,
            user_request: "send 2000 to alex, my email is maya@example.com card 4111111111111111"
                .into(),
            amount: Some(Money::php_major(2_000)),
            recipient_known: true,
            recipient_age_days: Some(240),
            recipient_display_name: Some("Alex Rivera".into()),
            device_trusted: true,
            location_familiar: true,
            transactions_last_hour: 0,
            transfers_last_hour: 0,
            usual_transfer_min: Some(Money::php_major(2_000)),
            usual_transfer_max: Some(Money::php_major(8_000)),
            missing_fields: vec![],
            customer_name: "Maya Santos".into(),
            account_number: "PH-0012-3456".into(),
            balance: Some(Money::php_major(120_000)),
        }
    }

    #[test]
    fn minimisation_drops_pii_and_balances() {
        let ctx = RiskContext::from_facts(&facts());
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(!json.contains("Maya"));
        assert!(!json.contains("Santos"));
        assert!(
            !json.contains("Rivera"),
            "recipient name must not leave the building"
        );
        assert!(!json.contains("PH-0012"));
        assert!(!json.contains("maya@example.com"));
        assert!(!json.contains("4111111111111111"));
        assert!(!json.contains("120000"), "balance is not sent to vendors");
        assert!(json.contains("[email]") && json.contains("[number]"));
    }

    #[test]
    fn ratio_and_summary() {
        let ctx = RiskContext::from_facts(&facts());
        assert_eq!(ctx.amount_to_typical_ratio, Some(0.25));
        assert_eq!(
            ctx.proposed_action,
            "Transfer PHP 2000.00 to a known recipient"
        );
        assert_eq!(ctx.risk_class, RiskClass::MoneyMovement);
    }

    #[test]
    fn noul_confidence_is_certainty_of_the_call() {
        assert_eq!(
            ProbabilityAssessment::from_yes_probability(0.5).confidence,
            0.0
        );
        assert!((ProbabilityAssessment::from_yes_probability(0.97).confidence - 0.94).abs() < 1e-9);
        assert!((ProbabilityAssessment::from_yes_probability(0.03).confidence - 0.94).abs() < 1e-9);
    }
}
