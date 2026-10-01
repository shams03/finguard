//! Scripted, deterministic stand-in for Jev. Unit tests and the offline demo
//! use this; real calls belong to development and integration tests.
//!
//! Without an override it derives a plausible assessment from the context
//! (so the demo scenarios behave sensibly): incomplete -> low confidence,
//! new recipient + far above typical or unfamiliar device -> high risk,
//! otherwise low risk.

use async_trait::async_trait;
use finguard_domain::*;
use serde_json::json;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockFailure {
    Timeout,
    RateLimited,
    ServerError,
    InvalidResponse,
    Unauthorized,
}

impl MockFailure {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "timeout" => Self::Timeout,
            "429" | "rate_limited" => Self::RateLimited,
            "5xx" | "server_error" => Self::ServerError,
            "invalid_response" => Self::InvalidResponse,
            "unauthorized" => Self::Unauthorized,
            _ => return None,
        })
    }
    fn error(self) -> DecisionProviderError {
        match self {
            Self::Timeout => DecisionProviderError::Timeout,
            Self::RateLimited => DecisionProviderError::RateLimited {
                retry_after_ms: Some(1000),
            },
            Self::ServerError => DecisionProviderError::ProviderUnavailable("HTTP 503".into()),
            Self::InvalidResponse => {
                DecisionProviderError::InvalidResponse("mock: malformed response".into())
            }
            Self::Unauthorized => DecisionProviderError::Unauthorized,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockProfile {
    LowRisk,
    HighRisk,
    LowConfidence,
}

#[derive(Clone, Default)]
pub struct MockDecisionProvider {
    failure: Arc<Mutex<Option<MockFailure>>>,
    profile: Arc<Mutex<Option<MockProfile>>>,
    calls: Arc<AtomicUsize>,
    latency: Duration,
}

impl MockDecisionProvider {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_latency(mut self, d: Duration) -> Self {
        self.latency = d;
        self
    }
    pub fn fail_with(&self, f: Option<MockFailure>) {
        *self.failure.lock().expect("mock lock") = f;
    }
    pub fn force_profile(&self, p: Option<MockProfile>) {
        *self.profile.lock().expect("mock lock") = p;
    }
    /// How many times `assess` was called (tests assert 0 for hard blocks).
    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    pub fn current_failure(&self) -> Option<MockFailure> {
        *self.failure.lock().expect("mock lock")
    }
}

fn score(level: f64, probs: [f64; 5], conf: f64, labels: &[&str; 5]) -> ScoredAssessment {
    ScoredAssessment {
        score: level,
        labels: labels.iter().map(|s| (*s).to_owned()).collect(),
        probabilities: probs.to_vec(),
        confidence: conf,
    }
}

#[async_trait]
impl DecisionProvider for MockDecisionProvider {
    fn name(&self) -> &'static str {
        "mock-jev"
    }

    async fn assess(&self, ctx: &RiskContext) -> Result<RiskAssessment, DecisionProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.latency.is_zero() {
            tokio::time::sleep(self.latency).await;
        }
        if let Some(f) = self.current_failure() {
            return Err(f.error());
        }
        let profile = self.profile.lock().expect("mock lock").unwrap_or_else(|| {
            let unfamiliar = !ctx.device_trusted || !ctx.location_familiar;
            let far = ctx.amount_to_typical_ratio.is_some_and(|r| r >= 5.0);
            let new_recipient = !ctx.recipient_known;
            if !ctx.missing_fields.is_empty() {
                MockProfile::LowConfidence
            } else if (new_recipient && far) || (unfamiliar && far) {
                MockProfile::HighRisk
            } else {
                MockProfile::LowRisk
            }
        });
        let a_lbl = [
            "Normal for this customer",
            "Mildly unusual",
            "Elevated",
            "Highly unusual",
            "Extreme",
        ];
        let r_lbl = [
            "Established recipient, low-risk context",
            "Slightly unusual recipient context",
            "Elevated recipient risk",
            "High recipient risk",
            "Extreme recipient risk",
        ];
        let incomplete = !ctx.missing_fields.is_empty();
        let (intent, sufficient, anomaly, recipient) = match profile {
            MockProfile::LowRisk => (
                ProbabilityAssessment::from_yes_probability(0.98),
                ProbabilityAssessment::from_yes_probability(0.98),
                Some(score(0.2, [0.81, 0.14, 0.04, 0.01, 0.0], 0.93, &a_lbl)),
                Some(score(0.15, [0.88, 0.09, 0.03, 0.0, 0.0], 0.95, &r_lbl)),
            ),
            MockProfile::HighRisk => (
                ProbabilityAssessment::from_yes_probability(0.97),
                ProbabilityAssessment::from_yes_probability(0.97),
                Some(score(3.2, [0.0, 0.02, 0.1, 0.68, 0.2], 0.93, &a_lbl)),
                Some(score(2.8, [0.02, 0.08, 0.2, 0.7, 0.0], 0.9, &r_lbl)),
            ),
            MockProfile::LowConfidence => (
                ProbabilityAssessment::from_yes_probability(0.96),
                // yes ~ 0.185 => derived confidence 0.63, matching the spec's example
                ProbabilityAssessment::from_yes_probability(0.185),
                Some(score(0.6, [0.5, 0.3, 0.15, 0.05, 0.0], 0.63, &a_lbl)),
                Some(score(0.5, [0.55, 0.3, 0.1, 0.05, 0.0], 0.7, &r_lbl)),
            ),
        };
        let (anomaly, recipient) = if incomplete {
            (None, None)
        } else {
            (anomaly, recipient)
        };
        let mut a = RiskAssessment {
            intent_alignment: Some(intent),
            behavioral_anomaly: anomaly,
            recipient_risk: recipient,
            context_sufficient: Some(sufficient),
            overall_confidence: 0.0,
            provider_metadata: ProviderMetadata {
                provider: "mock-jev".into(),
                model: Some("mock-jev-1".into()),
                latency_ms: self.latency.as_millis() as u64,
                question_set_version: RISK_QUESTION_SET_VERSION.into(),
                input_tokens: Some(300),
                output_tokens: Some(40),
                credits_charged: Some(1),
                credits_remaining: None,
            },
            raw: json!({"mock": true, "profile": format!("{profile:?}")}),
        };
        a.overall_confidence = a.weakest_link_confidence();
        Ok(a)
    }
}
