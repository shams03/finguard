use crate::{parse, questions};
use async_trait::async_trait;
use finguard_domain::*;
use reqwest::{StatusCode, header};
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct JevConfig {
    pub api_key: Secret<String>,
    /// Full endpoint URL, e.g. `https://jevstation.com/api/v1/systemone` or
    /// `https://api.typesafe.ai/v1/systemone`.
    pub endpoint: String,
    pub model: String,
    /// Per-attempt timeout (official SDK default: 10 s).
    pub timeout: Duration,
    /// Extra attempts for transient failures (timeouts, 429, 5xx). Bounded.
    pub max_retries: u32,
    /// Hard cap for the whole call including retries and backoff.
    pub total_deadline: Duration,
    pub backoff_base: Duration,
}

impl JevConfig {
    pub fn new(api_key: String, endpoint: String) -> Self {
        Self {
            api_key: Secret::new(api_key),
            endpoint,
            model: "jev-latest".into(),
            timeout: Duration::from_secs(10),
            max_retries: 1,
            total_deadline: Duration::from_secs(20),
            backoff_base: Duration::from_millis(400),
        }
    }
}

impl std::fmt::Debug for JevConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JevConfig")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("timeout", &self.timeout)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

pub struct JevDecisionProvider {
    cfg: JevConfig,
    http: reqwest::Client,
    provider: &'static str,
}

impl JevDecisionProvider {
    pub fn new(cfg: JevConfig) -> Result<Self, reqwest::Error> {
        let provider = if cfg.endpoint.contains("typesafe.ai") {
            "typesafe"
        } else {
            "jevstation"
        };
        let http = reqwest::Client::builder()
            .timeout(cfg.timeout) // never wait forever
            .connect_timeout(Duration::from_secs(5))
            .user_agent("finguard/0.1")
            .build()?;
        Ok(Self {
            cfg,
            http,
            provider,
        })
    }

    async fn attempt(&self, body: &Value) -> Result<Value, DecisionProviderError> {
        let resp = self
            .http
            .post(&self.cfg.endpoint)
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", self.cfg.api_key.expose()),
            )
            .json(body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    DecisionProviderError::Timeout
                } else {
                    // Do not include the URL/error chain: it can be noisy but, more
                    // importantly, we keep one stable, secret-free message.
                    DecisionProviderError::ProviderUnavailable("connection failed".into())
                }
            })?;
        let status = resp.status();
        if status.is_success() {
            return resp.json::<Value>().await.map_err(|_| {
                DecisionProviderError::InvalidResponse("body was not valid JSON".into())
            });
        }
        let retry_after_ms = resp
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|secs| (secs * 1000.0) as u64);
        Err(match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => DecisionProviderError::Unauthorized,
            StatusCode::TOO_MANY_REQUESTS => DecisionProviderError::RateLimited { retry_after_ms },
            StatusCode::PAYMENT_REQUIRED => {
                DecisionProviderError::ProviderUnavailable("provider credits exhausted".into())
            }
            s if s.is_server_error() || s.as_u16() == 529 => {
                DecisionProviderError::ProviderUnavailable(format!("HTTP {}", s.as_u16()))
            }
            s => DecisionProviderError::InvalidResponse(format!(
                "request rejected: HTTP {}",
                s.as_u16()
            )),
        })
    }

    fn retryable(e: &DecisionProviderError) -> bool {
        matches!(
            e,
            DecisionProviderError::Timeout
                | DecisionProviderError::RateLimited { .. }
                | DecisionProviderError::ProviderUnavailable(_)
        )
        // Unauthorized and InvalidResponse are deterministic: retrying cannot help.
    }
}

#[async_trait]
impl DecisionProvider for JevDecisionProvider {
    fn name(&self) -> &'static str {
        self.provider
    }

    async fn assess(&self, ctx: &RiskContext) -> Result<RiskAssessment, DecisionProviderError> {
        let request = questions::build_request(ctx, &self.cfg.model);
        let asked = request["questions"].clone();
        let started = Instant::now();

        let run = async {
            let mut attempt = 0;
            loop {
                match self.attempt(&request).await {
                    Ok(body) => {
                        let latency_ms = started.elapsed().as_millis() as u64;
                        return parse::parse_response(parse::ParseInput {
                            body: &body,
                            requested_questions: &asked,
                            provider: self.provider,
                            latency_ms,
                        });
                    }
                    Err(e) if attempt < self.cfg.max_retries && Self::retryable(&e) => {
                        let wait = match &e {
                            // Honour Retry-After, but never park a user request for long.
                            DecisionProviderError::RateLimited {
                                retry_after_ms: Some(ms),
                            } if *ms > 3_000 => {
                                return Err(e);
                            }
                            DecisionProviderError::RateLimited {
                                retry_after_ms: Some(ms),
                            } => Duration::from_millis(*ms),
                            _ => self.cfg.backoff_base * 2u32.pow(attempt),
                        };
                        tracing::warn!(code = e.code(), attempt, "jev call failed; retrying");
                        tokio::time::sleep(wait).await;
                        attempt += 1;
                    }
                    Err(e) => return Err(e),
                }
            }
        };
        match tokio::time::timeout(self.cfg.total_deadline, run).await {
            Ok(r) => r,
            Err(_) => Err(DecisionProviderError::Timeout),
        }
    }
}
