//! Confidence-aware escalation. Low confidence means "extra handling is
//! required" — never "the second model wins".

use crate::{
    action::MissingField,
    intent::{LlmProviderError, ModelCallMeta},
    money::Money,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EscalationReason {
    LowDecisionConfidence,
    AmbiguousIntent,
    MissingInformation,
    ConflictingSignals,
    HighRiskAction,
    DecisionProviderUnavailable,
}

impl EscalationReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::LowDecisionConfidence => "LOW_DECISION_CONFIDENCE",
            Self::AmbiguousIntent => "AMBIGUOUS_INTENT",
            Self::MissingInformation => "MISSING_INFORMATION",
            Self::ConflictingSignals => "CONFLICTING_SIGNALS",
            Self::HighRiskAction => "HIGH_RISK_ACTION",
            Self::DecisionProviderUnavailable => "DECISION_PROVIDER_UNAVAILABLE",
        }
    }
}

/// The *only* things the escalation model may recommend. There is deliberately
/// no variant meaning "execute" or "override policy": such a value cannot be
/// deserialised, so a compromised/injected model response is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecommendedHandling {
    ContinuePolicy,
    AskUser,
    Review,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EscalationInput {
    pub reason: EscalationReason,
    /// Scrubbed original request.
    pub user_request: String,
    pub proposed_action: String,
    pub missing_fields: Vec<MissingField>,
    /// Plain-text digest of the Jev assessment (no raw payloads).
    pub jev_summary: Option<String>,
    /// First name only, used to phrase the question.
    pub recipient_first_name: Option<String>,
    /// A candidate amount the **backend** derived from the user's own history.
    /// The model may mention an amount in its question only if this is set.
    pub candidate_amount: Option<Money>,
    pub candidate_evidence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EscalationAnalysis {
    pub intent_supported: bool,
    pub ambiguity_detected: bool,
    pub missing_information: Vec<String>,
    pub clarification_needed: bool,
    pub clarification_question: Option<String>,
    pub recommended_handling: RecommendedHandling,
    #[serde(skip)]
    pub meta: Option<ModelCallMeta>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum EscalationError {
    #[error(transparent)]
    Llm(#[from] LlmProviderError),
}

#[async_trait]
pub trait EscalationAnalyzer: Send + Sync {
    fn name(&self) -> &'static str;
    async fn analyze(&self, input: EscalationInput) -> Result<EscalationAnalysis, EscalationError>;
}

/// Deterministic clarification used when the model fails or writes a question
/// we cannot verify. Never invents a number: it only names the backend's own
/// candidate amount.
pub fn fallback_question(input: &EscalationInput) -> String {
    let who = input
        .recipient_first_name
        .as_deref()
        .unwrap_or("the recipient");
    if input.missing_fields.contains(&MissingField::Recipient) {
        return "Who would you like to send this to?".to_owned();
    }
    if input.missing_fields.contains(&MissingField::Amount) {
        return match input.candidate_amount {
            Some(m) => format!(
                "You usually send {who} {}. Would you like to send {} this time?",
                m.pretty(),
                m.pretty()
            ),
            None => format!("How much would you like to send {who}?"),
        };
    }
    "Please double-check the details of this request before continuing.".to_owned()
}

/// Grounding check for model-written clarification questions: every number in
/// the question must come from the user's own request or from the backend's
/// `candidate_amount`. Prevents a model from inventing an irreversible
/// parameter ("You usually send ₱50,000!").
pub fn question_is_grounded(question: &str, input: &EscalationInput) -> bool {
    let mut allowed: Vec<String> = digit_runs(&input.user_request);
    if let Some(m) = input.candidate_amount {
        allowed.extend(digit_runs(&m.pretty()));
        allowed.extend(digit_runs(&m.plain_major()));
        allowed.push((m.amount_minor / 100).to_string());
    }
    digit_runs(question).iter().all(|n| allowed.contains(n))
}

/// Digit sequences with thousands separators and decimals normalised away:
/// "₱12,000.50" -> ["12000", "50"].
fn digit_runs(s: &str) -> Vec<String> {
    let cleaned: String = {
        let chars: Vec<char> = s.chars().collect();
        let mut out = String::new();
        for (i, &c) in chars.iter().enumerate() {
            let between_digits = i > 0
                && i + 1 < chars.len()
                && chars[i - 1].is_ascii_digit()
                && chars[i + 1].is_ascii_digit();
            if c == ',' && between_digits {
                continue;
            }
            out.push(if c.is_ascii_digit() || c == '.' && between_digits {
                c
            } else {
                ' '
            });
        }
        out
    };
    cleaned
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.trim_start_matches('0').to_owned())
        .map(|p| if p.is_empty() { "0".into() } else { p })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(candidate: Option<Money>) -> EscalationInput {
        EscalationInput {
            reason: EscalationReason::MissingInformation,
            user_request: "Send Alex the usual amount".into(),
            proposed_action: "Transfer of an unspecified amount".into(),
            missing_fields: vec![MissingField::Amount],
            jev_summary: None,
            recipient_first_name: Some("Alex".into()),
            candidate_amount: candidate,
            candidate_evidence: None,
        }
    }

    #[test]
    fn grounded_question_only_mentions_backend_amount() {
        let i = input(Some(Money::php_major(5_000)));
        assert!(question_is_grounded(
            "You usually send Alex ₱5,000. Send ₱5,000 this time?",
            &i
        ));
        assert!(!question_is_grounded(
            "You usually send Alex ₱50,000. Send it?",
            &i
        ));
    }

    #[test]
    fn no_candidate_means_no_numbers() {
        let i = input(None);
        assert!(question_is_grounded(
            "How much would you like to send Alex?",
            &i
        ));
        assert!(!question_is_grounded("Do you want to send ₱12,000?", &i));
    }

    #[test]
    fn handling_cannot_express_execution() {
        for bad in [
            "EXECUTE_TRANSFER",
            "OVERRIDE_AUTHORIZATION",
            "IGNORE_POLICY",
        ] {
            let r: Result<RecommendedHandling, _> = serde_json::from_str(&format!("\"{bad}\""));
            assert!(r.is_err(), "{bad} must not deserialise");
        }
        assert!(serde_json::from_str::<RecommendedHandling>("\"ASK_USER\"").is_ok());
    }
}
