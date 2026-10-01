mod common;
use common::*;
use finguard_domain::*;
use finguard_groq::{ChatBackend, ChatRequest, GroqConfig, GroqHttpBackend};
use serde_json::{Value, json};
use std::time::Duration;

fn req() -> ChatRequest {
    ChatRequest {
        system: "sys".into(),
        user: "hello".into(),
        schema_name: "finguard_test",
        schema: json!({"type":"object","properties":{"a":{"type":"string"}},"required":["a"],"additionalProperties":false}),
        repair_hint: None,
    }
}
fn ok_body() -> String {
    json!({"model":"openai/gpt-oss-20b","choices":[{"message":{"content":"{\"a\":\"x\"}"}}],
           "usage":{"prompt_tokens":312,"completion_tokens":61,"total_tokens":373}})
    .to_string()
}
fn backend(url: &str, retries: u32, timeout_ms: u64) -> GroqHttpBackend {
    let mut c = GroqConfig::new(
        "gsk_TESTSECRETKEY123".into(),
        url.into(),
        "openai/gpt-oss-20b".into(),
    );
    c.max_retries = retries;
    c.timeout = Duration::from_millis(timeout_ms);
    c.backoff_base = Duration::from_millis(10);
    GroqHttpBackend::new(c).unwrap()
}
fn body_of(raw: &str) -> Value {
    serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap()
}

#[tokio::test]
async fn strict_request_shape_and_usage_parsing() {
    let s = serve(vec![Scripted::json(200, &ok_body())]).await;
    let r = backend(&s.url, 0, 2000).complete(&req()).await.unwrap();
    assert_eq!(r.content, "{\"a\":\"x\"}");
    assert_eq!((r.input_tokens, r.output_tokens), (Some(312), Some(61)));
    let raw = s.requests.lock().unwrap()[0].clone();
    assert!(raw.starts_with("POST /chat/completions"));
    assert!(
        raw.to_lowercase()
            .contains("authorization: bearer gsk_testsecretkey123")
    );
    let b = body_of(&raw);
    assert_eq!(b["model"], "openai/gpt-oss-20b");
    assert_eq!(b["temperature"], 0);
    assert_eq!(b["response_format"]["type"], "json_schema");
    assert_eq!(b["response_format"]["json_schema"]["strict"], true);
    assert_eq!(b["response_format"]["json_schema"]["name"], "finguard_test");
    assert_eq!(b["reasoning_effort"], "low");
}

#[tokio::test]
async fn degrades_to_json_mode_when_strict_schema_is_rejected() {
    let reject = r#"{"error":{"message":"response_format json_schema is not supported for this model","type":"invalid_request_error"}}"#;
    let s = serve(vec![
        Scripted::json(400, reject),
        Scripted::json(200, &ok_body()),
    ])
    .await;
    let r = backend(&s.url, 0, 2000).complete(&req()).await.unwrap();
    assert_eq!(r.content, "{\"a\":\"x\"}");
    let reqs = s.requests.lock().unwrap().clone();
    assert_eq!(reqs.len(), 2);
    let second = body_of(&reqs[1]);
    assert_eq!(second["response_format"]["type"], "json_object");
    assert!(
        second["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("JSON Schema"),
        "schema moves into the prompt"
    );
}

#[tokio::test]
async fn retries_429_then_succeeds_and_5xx_is_bounded() {
    let s = serve(vec![
        Scripted::json(429, "{}").with_header("retry-after", "0"),
        Scripted::json(200, &ok_body()),
    ])
    .await;
    assert!(backend(&s.url, 1, 2000).complete(&req()).await.is_ok());
    assert_eq!(s.request_count(), 2);

    let s = serve(vec![Scripted::json(503, "{}")]).await;
    let e = backend(&s.url, 1, 2000).complete(&req()).await.unwrap_err();
    assert!(
        matches!(e, LlmProviderError::ProviderUnavailable(_)),
        "{e:?}"
    );
    assert_eq!(s.request_count(), 2);
}

#[tokio::test]
async fn auth_failures_are_not_retried_and_long_retry_after_is_surfaced() {
    let s = serve(vec![Scripted::json(401, "{}")]).await;
    assert_eq!(
        backend(&s.url, 3, 2000).complete(&req()).await.unwrap_err(),
        LlmProviderError::Unauthorized
    );
    assert_eq!(s.request_count(), 1);

    let s = serve(vec![
        Scripted::json(429, "{}").with_header("retry-after", "30"),
    ])
    .await;
    let e = backend(&s.url, 3, 2000).complete(&req()).await.unwrap_err();
    assert_eq!(
        e,
        LlmProviderError::RateLimited {
            retry_after_ms: Some(30_000)
        }
    );
    assert_eq!(s.request_count(), 1);
}

#[tokio::test]
async fn timeout_and_malformed_responses_are_typed() {
    let s = serve(vec![Scripted::json(200, &ok_body()).delayed(1500)]).await;
    assert_eq!(
        backend(&s.url, 0, 100).complete(&req()).await.unwrap_err(),
        LlmProviderError::Timeout
    );

    let s = serve(vec![Scripted::json(200, "not json")]).await;
    assert!(matches!(
        backend(&s.url, 0, 2000).complete(&req()).await.unwrap_err(),
        LlmProviderError::InvalidStructuredOutput(_)
    ));
    let s = serve(vec![Scripted::json(200, r#"{"choices":[]}"#)]).await;
    assert!(matches!(
        backend(&s.url, 0, 2000).complete(&req()).await.unwrap_err(),
        LlmProviderError::InvalidStructuredOutput(_)
    ));
}

#[tokio::test]
async fn secrets_never_leak_into_errors_or_debug() {
    let s = serve(vec![Scripted::json(500, "{}")]).await;
    let e = backend(&s.url, 0, 2000).complete(&req()).await.unwrap_err();
    assert!(!format!("{e} {e:?}").contains("TESTSECRETKEY"));
    let c = GroqConfig::new("gsk_TOPSECRET".into(), "http://x".into(), "m".into());
    assert!(!format!("{c:?}").contains("TOPSECRET"));
}
