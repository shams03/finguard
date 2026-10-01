use async_trait::async_trait;
use finguard_domain::{LlmProviderError, Secret};
use reqwest::{StatusCode, header};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub system: String,
    pub user: String,
    /// Name for the JSON schema (strict mode) — also used in logs.
    pub schema_name: &'static str,
    pub schema: Value,
    /// Extra user-turn text appended on a repair attempt.
    pub repair_hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    /// The model's JSON text (not yet validated).
    pub content: String,
    pub model: String,
    pub latency_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[async_trait]
pub trait ChatBackend: Send + Sync {
    fn model(&self) -> String;
    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, LlmProviderError>;
}

#[derive(Clone)]
pub struct GroqConfig {
    pub api_key: Secret<String>,
    pub base_url: String,
    pub model: String,
    pub timeout: Duration,
    pub max_retries: u32,
    pub backoff_base: Duration,
    /// `true`: `response_format: json_schema` with `strict: true` (guaranteed
    /// by Groq for supported models). `false`: JSON mode + our own validation.
    pub strict_schema: bool,
    /// `reasoning_effort` for reasoning models such as gpt-oss ("low" keeps
    /// latency and token usage down). Empty = not sent.
    pub reasoning_effort: Option<String>,
}

impl GroqConfig {
    pub fn new(api_key: String, base_url: String, model: String) -> Self {
        Self {
            api_key: Secret::new(api_key),
            base_url,
            model,
            timeout: Duration::from_secs(15),
            max_retries: 1,
            backoff_base: Duration::from_millis(500),
            strict_schema: true,
            reasoning_effort: Some("low".into()),
        }
    }
}

impl std::fmt::Debug for GroqConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroqConfig")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("strict_schema", &self.strict_schema)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

pub struct GroqHttpBackend {
    cfg: GroqConfig,
    http: reqwest::Client,
}

impl GroqHttpBackend {
    pub fn new(cfg: GroqConfig) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder()
            .timeout(cfg.timeout)
            .connect_timeout(Duration::from_secs(5))
            .user_agent("finguard/0.1")
            .build()?;
        Ok(Self { cfg, http })
    }

    fn body(&self, req: &ChatRequest, strict: bool) -> Value {
        let mut system = req.system.clone();
        let response_format = if strict {
            json!({
                "type": "json_schema",
                "json_schema": {"name": req.schema_name, "strict": true, "schema": req.schema}
            })
        } else {
            // JSON mode: the schema goes in the prompt and we validate server-side.
            system.push_str(&format!(
                "\n\nRespond with ONLY a JSON object matching this JSON Schema:\n{}",
                req.schema
            ));
            json!({"type": "json_object"})
        };
        let mut user = req.user.clone();
        if let Some(h) = &req.repair_hint {
            user.push_str("\n\n");
            user.push_str(h);
        }
        let mut body = json!({
            "model": self.cfg.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "temperature": 0,
            "max_completion_tokens": 1500,
            "response_format": response_format,
        });
        if let Some(e) = self
            .cfg
            .reasoning_effort
            .as_deref()
            .filter(|e| !e.is_empty())
        {
            body["reasoning_effort"] = json!(e);
        }
        body
    }

    async fn once(&self, body: &Value) -> Result<(Value, Option<u64>), LlmProviderError> {
        let url = format!(
            "{}/chat/completions",
            self.cfg.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(url)
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", self.cfg.api_key.expose()),
            )
            .json(body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LlmProviderError::Timeout
                } else {
                    LlmProviderError::ProviderUnavailable("connection failed".into())
                }
            })?;
        let status = resp.status();
        let retry_after_ms = resp
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.trim().parse::<f64>().ok())
            .map(|s| (s * 1000.0) as u64);
        if status.is_success() {
            let v = resp.json::<Value>().await.map_err(|_| {
                LlmProviderError::InvalidStructuredOutput("response body was not JSON".into())
            })?;
            return Ok((v, retry_after_ms));
        }
        let text = resp.text().await.unwrap_or_default().to_lowercase();
        Err(match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => LlmProviderError::Unauthorized,
            StatusCode::TOO_MANY_REQUESTS => LlmProviderError::RateLimited { retry_after_ms },
            StatusCode::BAD_REQUEST
                if text.contains("schema")
                    || text.contains("json_validate")
                    || text.contains("response_format")
                    || text.contains("does not match") =>
            {
                LlmProviderError::InvalidStructuredOutput(
                    "provider rejected the schema/format".into(),
                )
            }
            s if s.is_server_error() || s.as_u16() == 498 => {
                LlmProviderError::ProviderUnavailable(format!("HTTP {}", s.as_u16()))
            }
            s => LlmProviderError::ProviderUnavailable(format!(
                "request rejected: HTTP {}",
                s.as_u16()
            )),
        })
    }

    fn retryable(e: &LlmProviderError) -> bool {
        matches!(
            e,
            LlmProviderError::Timeout
                | LlmProviderError::RateLimited { .. }
                | LlmProviderError::ProviderUnavailable(_)
        )
    }
}

#[async_trait]
impl ChatBackend for GroqHttpBackend {
    fn model(&self) -> String {
        self.cfg.model.clone()
    }

    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, LlmProviderError> {
        let started = Instant::now();
        let mut strict = self.cfg.strict_schema;
        let mut attempt = 0;
        loop {
            let body = self.body(req, strict);
            match self.once(&body).await {
                Ok((v, _)) => {
                    let content = v["choices"][0]["message"]["content"]
                        .as_str()
                        .ok_or_else(|| {
                            LlmProviderError::InvalidStructuredOutput("no message content".into())
                        })?
                        .to_owned();
                    return Ok(ChatResponse {
                        content,
                        model: v["model"]
                            .as_str()
                            .unwrap_or(&self.cfg.model)
                            .chars()
                            .take(64)
                            .collect(),
                        latency_ms: started.elapsed().as_millis() as u64,
                        input_tokens: v["usage"]["prompt_tokens"].as_u64(),
                        output_tokens: v["usage"]["completion_tokens"].as_u64(),
                    });
                }
                // The model does not accept strict schemas (or this schema):
                // degrade once to JSON mode; server-side validation still applies.
                Err(LlmProviderError::InvalidStructuredOutput(_)) if strict => {
                    tracing::warn!("groq rejected strict schema; falling back to JSON mode");
                    strict = false;
                }
                Err(e) if attempt < self.cfg.max_retries && Self::retryable(&e) => {
                    let wait = match &e {
                        LlmProviderError::RateLimited {
                            retry_after_ms: Some(ms),
                        } if *ms > 3_000 => return Err(e),
                        LlmProviderError::RateLimited {
                            retry_after_ms: Some(ms),
                        } => Duration::from_millis(*ms),
                        _ => self.cfg.backoff_base * 2u32.pow(attempt),
                    };
                    tracing::warn!(code = e.code(), attempt, "groq call failed; retrying");
                    tokio::time::sleep(wait).await;
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// Test double: returns queued responses (JSON text or errors) in order.
#[derive(Clone, Default)]
pub struct ScriptedBackend {
    queue: Arc<Mutex<VecDeque<Result<String, LlmProviderError>>>>,
    calls: Arc<AtomicUsize>,
    pub requests: Arc<Mutex<Vec<ChatRequest>>>,
}

impl ScriptedBackend {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push_ok(&self, json_text: &str) -> &Self {
        self.queue
            .lock()
            .unwrap()
            .push_back(Ok(json_text.to_owned()));
        self
    }
    pub fn push_err(&self, e: LlmProviderError) -> &Self {
        self.queue.lock().unwrap().push_back(Err(e));
        self
    }
    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ChatBackend for ScriptedBackend {
    fn model(&self) -> String {
        "scripted-model".into()
    }
    async fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, LlmProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.requests.lock().unwrap().push(req.clone());
        let next = self.queue.lock().unwrap().pop_front();
        match next {
            Some(Ok(content)) => Ok(ChatResponse {
                content,
                model: "scripted-model".into(),
                latency_ms: 1,
                input_tokens: Some(100),
                output_tokens: Some(20),
            }),
            Some(Err(e)) => Err(e),
            None => Err(LlmProviderError::ProviderUnavailable(
                "script exhausted".into(),
            )),
        }
    }
}
