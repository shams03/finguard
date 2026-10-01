use async_trait::async_trait;
use finguard_domain::*;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockLlmFailure {
    Timeout,
    RateLimited,
    Unavailable,
    InvalidOutput,
}

impl MockLlmFailure {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "timeout" => Self::Timeout,
            "429" | "rate_limited" => Self::RateLimited,
            "5xx" | "unavailable" => Self::Unavailable,
            "invalid_output" => Self::InvalidOutput,
            _ => return None,
        })
    }
    fn error(self) -> LlmProviderError {
        match self {
            Self::Timeout => LlmProviderError::Timeout,
            Self::RateLimited => LlmProviderError::RateLimited {
                retry_after_ms: Some(1000),
            },
            Self::Unavailable => LlmProviderError::ProviderUnavailable("HTTP 503".into()),
            Self::InvalidOutput => {
                LlmProviderError::InvalidStructuredOutput("mock: malformed output".into())
            }
        }
    }
}

/// Deterministic offline escalation analyzer (tests, demos without a Groq key).
#[derive(Clone, Default)]
pub struct MockEscalationAnalyzer {
    failure: Arc<Mutex<Option<MockLlmFailure>>>,
    calls: Arc<AtomicUsize>,
}

impl MockEscalationAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn fail_with(&self, f: Option<MockLlmFailure>) {
        *self.failure.lock().expect("mock lock") = f;
    }
    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl EscalationAnalyzer for MockEscalationAnalyzer {
    fn name(&self) -> &'static str {
        "mock-groq"
    }
    async fn analyze(&self, input: EscalationInput) -> Result<EscalationAnalysis, EscalationError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(f) = *self.failure.lock().expect("mock lock") {
            return Err(EscalationError::Llm(f.error()));
        }
        let missing = !input.missing_fields.is_empty();
        Ok(EscalationAnalysis {
            intent_supported: true,
            ambiguity_detected: missing,
            missing_information: input
                .missing_fields
                .iter()
                .map(|m| m.as_str().to_owned())
                .collect(),
            clarification_needed: missing,
            clarification_question: missing.then(|| fallback_question(&input)),
            recommended_handling: if missing {
                RecommendedHandling::AskUser
            } else {
                RecommendedHandling::Review
            },
            meta: Some(ModelCallMeta {
                provider: "mock-groq".into(),
                model: "mock-llm".into(),
                latency_ms: 1,
                input_tokens: Some(180),
                output_tokens: Some(45),
            }),
        })
    }
}
