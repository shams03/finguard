mod common;
use common::*;
use finguard_domain::*;
use finguard_jev::{JevConfig, JevDecisionProvider};
use std::time::Duration;

fn ctx() -> RiskContext {
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
        missing_fields: vec![],
    }
}

const OK_BODY: &str = r#"{"code":0,"message":"ok","data":{"model":"jev-1.13.0","answers":{
 "intent_alignment":{"type":"noul","noul":0.97},
 "behavioral_anomaly":{"type":"score","score":0.2,"confidence":0.93,"probabilities":{"0":0.81,"1":0.14,"2":0.04,"3":0.01,"4":0.0}},
 "recipient_risk":{"type":"score","score":0.1,"confidence":0.95,"probabilities":{"0":0.9,"1":0.08,"2":0.02,"3":0.0,"4":0.0}},
 "context_sufficient":{"type":"noul","noul":0.98}},
 "usage":{"input_tokens":392,"output_tokens":65},"credits":{"charged":1,"remaining":199},"latency_ms":400}}"#;

fn provider(url: &str, retries: u32, timeout_ms: u64) -> JevDecisionProvider {
    let mut c = JevConfig::new("sk_test_SECRETSECRET".into(), url.into());
    c.max_retries = retries;
    c.timeout = Duration::from_millis(timeout_ms);
    c.total_deadline = Duration::from_secs(5);
    c.backoff_base = Duration::from_millis(10);
    JevDecisionProvider::new(c).unwrap()
}

#[tokio::test]
async fn success_sends_bearer_and_minimised_state() {
    let s = serve(vec![Scripted::json(200, OK_BODY)]).await;
    let a = provider(&s.url, 1, 2000).assess(&ctx()).await.unwrap();
    assert_eq!(a.provider_metadata.credits_charged, Some(1));
    assert!(a.overall_confidence > 0.9);
    let req = s.requests.lock().unwrap()[0].to_lowercase();
    assert!(req.contains("authorization: bearer sk_test_secretsecret"));
    assert!(req.contains("\"questions\"") && req.contains("intent_alignment"));
    assert!(!req.contains("maya"));
}

#[tokio::test]
async fn retries_transient_5xx_then_succeeds() {
    let s = serve(vec![
        Scripted::json(503, "{}"),
        Scripted::json(200, OK_BODY),
    ])
    .await;
    assert!(provider(&s.url, 1, 2000).assess(&ctx()).await.is_ok());
    assert_eq!(s.request_count(), 2);
}

#[tokio::test]
async fn persistent_5xx_is_provider_unavailable_after_bounded_retries() {
    let s = serve(vec![Scripted::json(500, "{}")]).await;
    let e = provider(&s.url, 1, 2000).assess(&ctx()).await.unwrap_err();
    assert!(
        matches!(e, DecisionProviderError::ProviderUnavailable(_)),
        "{e:?}"
    );
    assert_eq!(s.request_count(), 2, "1 try + 1 retry, no more");
}

#[tokio::test]
async fn rate_limit_honours_retry_after_then_succeeds() {
    let s = serve(vec![
        Scripted::json(429, "{}").with_header("retry-after", "0"),
        Scripted::json(200, OK_BODY),
    ])
    .await;
    assert!(provider(&s.url, 1, 2000).assess(&ctx()).await.is_ok());
    assert_eq!(s.request_count(), 2);
}

#[tokio::test]
async fn long_retry_after_is_surfaced_not_slept_through() {
    let s = serve(vec![
        Scripted::json(429, "{}").with_header("retry-after", "30"),
    ])
    .await;
    let e = provider(&s.url, 3, 2000).assess(&ctx()).await.unwrap_err();
    assert!(
        matches!(
            e,
            DecisionProviderError::RateLimited {
                retry_after_ms: Some(30_000)
            }
        ),
        "{e:?}"
    );
    assert_eq!(s.request_count(), 1);
}

#[tokio::test]
async fn unauthorized_is_not_retried() {
    let s = serve(vec![Scripted::json(401, "{}")]).await;
    let e = provider(&s.url, 3, 2000).assess(&ctx()).await.unwrap_err();
    assert_eq!(e, DecisionProviderError::Unauthorized);
    assert_eq!(s.request_count(), 1);
}

#[tokio::test]
async fn bad_request_is_not_retried() {
    let s = serve(vec![Scripted::json(422, "{}")]).await;
    let e = provider(&s.url, 3, 2000).assess(&ctx()).await.unwrap_err();
    assert!(matches!(e, DecisionProviderError::InvalidResponse(_)));
    assert_eq!(s.request_count(), 1);
}

#[tokio::test]
async fn slow_provider_times_out() {
    let s = serve(vec![Scripted::json(200, OK_BODY).delayed(1500)]).await;
    let e = provider(&s.url, 0, 100).assess(&ctx()).await.unwrap_err();
    assert_eq!(e, DecisionProviderError::Timeout);
}

#[tokio::test]
async fn garbage_and_invalid_shapes_are_invalid_response() {
    for body in [
        "not json",
        r#"{"answers":{}}"#,
        r#"{"code":-1,"message":"boom","data":null}"#,
    ] {
        let s = serve(vec![Scripted::json(200, body)]).await;
        let e = provider(&s.url, 0, 2000).assess(&ctx()).await.unwrap_err();
        assert!(
            matches!(e, DecisionProviderError::InvalidResponse(_)),
            "{body}: {e:?}"
        );
    }
}

#[tokio::test]
async fn api_key_never_appears_in_errors_or_debug() {
    let s = serve(vec![Scripted::json(500, "{}")]).await;
    let p = provider(&s.url, 0, 2000);
    let e = p.assess(&ctx()).await.unwrap_err();
    assert!(!format!("{e} {e:?}").contains("SECRETSECRET"));
    let cfg = JevConfig::new("sk_live_TOPSECRET".into(), "http://x".into());
    assert!(!format!("{cfg:?}").contains("TOPSECRET"));
}
