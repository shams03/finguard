//! Natural language -> structured intent. Parsers only *interpret*; they never
//! authorise, query the database, or execute anything.

use crate::{
    action::MissingField,
    money::{Currency, Money},
};
use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntentKind {
    ReadBalance,
    FreezeCard,
    TransferFunds,
    /// Anything we do not support. Never mapped to a tool.
    Unsupported,
}

/// Token/latency info for a model call (shown in the trace; no secrets).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelCallMeta {
    pub provider: String,
    pub model: String,
    pub latency_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedIntent {
    pub intent_type: IntentKind,
    pub recipient_reference: Option<String>,
    pub amount_minor: Option<i64>,
    pub currency: Option<Currency>,
    pub missing_fields: Vec<MissingField>,
    pub ambiguity: bool,
    /// The user referred to "the usual amount" without stating a number. The
    /// backend (not a model) decides whether history supports a suggestion.
    pub refers_to_usual_amount: bool,
    #[serde(skip)]
    pub meta: Option<ModelCallMeta>,
}

impl ParsedIntent {
    pub fn unsupported() -> Self {
        Self {
            intent_type: IntentKind::Unsupported,
            recipient_reference: None,
            amount_minor: None,
            currency: None,
            missing_fields: vec![],
            ambiguity: false,
            refers_to_usual_amount: false,
            meta: None,
        }
    }
}

/// Minimal context given to a parser. No balances, no ids, no secrets.
#[derive(Debug, Clone)]
pub struct IntentContext {
    pub default_currency: Currency,
    /// Display names of known recipients (so "Alex" resolves sensibly).
    pub known_recipient_names: Vec<String>,
}

#[derive(Debug, Clone, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum LlmProviderError {
    #[error("LLM provider rejected our credentials")]
    Unauthorized,
    #[error("LLM provider rate limited us")]
    RateLimited { retry_after_ms: Option<u64> },
    #[error("LLM provider timed out")]
    Timeout,
    #[error("LLM provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("LLM returned invalid structured output: {0}")]
    InvalidStructuredOutput(String),
}

impl LlmProviderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "UNAUTHORIZED",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::Timeout => "TIMEOUT",
            Self::ProviderUnavailable(_) => "PROVIDER_UNAVAILABLE",
            Self::InvalidStructuredOutput(_) => "INVALID_STRUCTURED_OUTPUT",
        }
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum IntentError {
    #[error("empty request")]
    Empty,
    #[error(transparent)]
    Llm(#[from] LlmProviderError),
}

#[async_trait]
pub trait IntentParser: Send + Sync {
    fn name(&self) -> &'static str;
    async fn parse(
        &self,
        input: &str,
        context: &IntentContext,
    ) -> Result<ParsedIntent, IntentError>;
}

static AMOUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:₱|php\s*|\bp\s*)?(\d{1,3}(?:,\d{3})+|\d+)(\.\d{1,2})?\s*(k\b|thousand\b)?")
        .unwrap()
});

/// Every money amount literally written in `text` ("₱2,000", "75000", "2k").
/// Used two ways: the deterministic parser reads the amount from here, and the
/// Groq parser *verifies* the model's amount appears here — a model is never
/// allowed to invent an irreversible parameter.
pub fn amounts_mentioned(text: &str, currency: Currency) -> Vec<Money> {
    AMOUNT
        .captures_iter(text)
        .filter_map(|c| {
            let whole = c.get(1)?.as_str().replace(',', "");
            let frac = c.get(2).map(|m| m.as_str()).unwrap_or("");
            let mut m = Money::parse_major(&format!("{whole}{frac}"), currency).ok()?;
            if c.get(3).is_some() {
                m = Money::new(m.amount_minor.checked_mul(1000)?, currency);
            }
            Some(m)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_written_amounts() {
        let a = |t: &str| {
            amounts_mentioned(t, Currency::Php)
                .into_iter()
                .map(|m| m.amount_minor)
                .collect::<Vec<_>>()
        };
        assert_eq!(a("Send ₱2,000 to Alex"), vec![200_000]);
        assert_eq!(a("send 75000 pesos to this new account"), vec![7_500_000]);
        assert_eq!(a("pay php 1,250.50 now"), vec![125_050]);
        assert_eq!(a("send alex 2k"), vec![200_000]);
        assert_eq!(a("Send Alex the usual amount"), Vec::<i64>::new());
    }
}
