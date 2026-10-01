//! HTTP-level checks of the real router against a real database (mock providers).
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use finguard_api::{AppState, build_state, config::*, router};
use finguard_persistence::{DbConfig, Store, seed};
use finguard_policy::ConfidenceThresholds;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

fn config(env: &str) -> Config {
    Config {
        app_env: env.into(),
        bind_addr: "127.0.0.1:0".into(),
        cors_origin: "http://localhost:5173".into(),
        db: DbConfig {
            url: "postgres://x:y@localhost/z".into(),
            migration_url: None,
            max_connections: 2,
            statement_cache: None,
        },
        jev: None,
        groq: None,
        parser: ParserKind::Deterministic,
        decision: DecisionKind::Mock,
        escalation: EscalationKind::Mock,
        thresholds: ConfidenceThresholds::default(),
    }
}

async fn app(pool: PgPool, env: &str) -> (Router, AppState) {
    seed::reset_demo(&pool, &Default::default()).await.unwrap();
    let state = build_state(Store::from_pool(pool), &config(env)).unwrap();
    (router(state.clone(), "http://localhost:5173"), state)
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    if body.is_some() {
        b = b.header("content-type", "application/json");
    }
    if let Some(k) = key {
        b = b.header("idempotency-key", k);
    }
    let req = b
        .body(
            body.map(|v| Body::from(v.to_string()))
                .unwrap_or_else(Body::empty),
        )
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[sqlx::test(migrations = "../../migrations")]
async fn full_transfer_over_http_with_idempotent_retry(pool: PgPool) {
    let (app, _) = app(pool, "development").await;
    let (st, v) = call(&app, "GET", "/health", None, None).await;
    assert_eq!((st, v["status"].as_str()), (StatusCode::OK, Some("ok")));

    let (st, prop) = call(
        &app,
        "POST",
        "/api/v1/agent/commands",
        Some("cmd-1"),
        Some(json!({"text": "Send ₱2,000 to Alex."})),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(prop["status"], "AWAITING_CONFIRMATION");
    assert_eq!(prop["proposed_action"]["amount"], "PHP 2000.00");
    assert_eq!(prop["proposed_action"]["recipient"], "Alex Rivera");
    let (id, cnf) = (
        prop["action_id"].as_str().unwrap(),
        prop["confirmation"]["confirmation_id"].as_str().unwrap(),
    );

    // same command key again -> the same action, 200 not 201
    let (st, again) = call(
        &app,
        "POST",
        "/api/v1/agent/commands",
        Some("cmd-1"),
        Some(json!({"text": "Send ₱2,000 to Alex."})),
    )
    .await;
    assert_eq!(
        (st, again["action_id"].as_str(), again["replayed"].as_bool()),
        (StatusCode::OK, Some(id), Some(true))
    );
    // same key, different request -> conflict in the standard error schema
    let (st, err) = call(
        &app,
        "POST",
        "/api/v1/agent/commands",
        Some("cmd-1"),
        Some(json!({"text": "Send ₱3,000 to Alex."})),
    )
    .await;
    assert_eq!(
        (st, err["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("IDEMPOTENCY_CONFLICT"))
    );
    assert!(
        err["error"]["trace_id"]
            .as_str()
            .unwrap()
            .starts_with("tr_")
    );

    let path = format!("/api/v1/actions/{id}/confirm");
    let (st, done) = call(
        &app,
        "POST",
        &path,
        Some("abc123"),
        Some(json!({"confirmation_id": cnf})),
    )
    .await;
    assert_eq!(
        (st, done["status"].as_str()),
        (StatusCode::OK, Some("EXECUTED"))
    );
    let (st, retry) = call(
        &app,
        "POST",
        &path,
        Some("abc123"),
        Some(json!({"confirmation_id": cnf})),
    )
    .await;
    assert_eq!(
        (st, retry["replayed"].as_bool()),
        (StatusCode::OK, Some(true))
    );
    // no key: single-use confirmation -> CONFIRMATION_INVALID
    let (st, bad) = call(
        &app,
        "POST",
        &path,
        None,
        Some(json!({"confirmation_id": cnf})),
    )
    .await;
    assert_eq!(
        (st, bad["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("CONFIRMATION_INVALID"))
    );

    let (_, acct) = call(&app, "GET", "/api/v1/account", None, None).await;
    assert_eq!(acct["account"]["balance_minor"], 11_800_000);
    let (_, tr) = call(
        &app,
        "GET",
        &format!("/api/v1/actions/{id}/trace"),
        None,
        None,
    )
    .await;
    assert_eq!(tr["status"], "EXECUTED");
    assert!(tr["ledger"]["entries"].as_array().unwrap().len() == 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn errors_use_one_schema_and_hide_internals(pool: PgPool) {
    let (app, _) = app(pool, "development").await;
    for (method, uri, body, code) in [
        (
            "POST",
            "/api/v1/agent/commands",
            Some(json!({"nope": 1})),
            "INVALID_REQUEST",
        ),
        (
            "POST",
            "/api/v1/agent/commands",
            Some(json!({"text": "tell me a joke"})),
            "UNKNOWN_ACTION",
        ),
        ("GET", "/api/v1/actions/not-an-id", None, "INVALID_REQUEST"),
        (
            "GET",
            "/api/v1/actions/act_00000000000000000000000000000001",
            None,
            "NOT_FOUND",
        ),
        ("GET", "/api/v1/does-not-exist", None, "NOT_FOUND"),
    ] {
        let (st, v) = call(&app, method, uri, None, body).await;
        assert!(st.is_client_error(), "{uri}: {st}");
        assert_eq!(v["error"]["code"], code, "{uri}");
        assert!(v["error"]["trace_id"].is_string());
        let text = v.to_string().to_lowercase();
        assert!(
            !text.contains("sqlx") && !text.contains("postgres") && !text.contains("panicked"),
            "{text}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn demo_endpoints_are_disabled_in_production(pool: PgPool) {
    let (app, _) = app(pool, "production").await;
    for uri in [
        "/api/v1/demo/reset",
        "/api/v1/demo/scenarios/normal",
        "/api/v1/demo/failures/jev/timeout",
    ] {
        let (st, v) = call(&app, "POST", uri, None, None).await;
        assert_eq!(
            (st, v["error"]["code"].as_str()),
            (StatusCode::FORBIDDEN, Some("FORBIDDEN")),
            "{uri}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn responses_never_contain_provider_secrets_or_db_urls(pool: PgPool) {
    let (app, _) = app(pool, "development").await;
    let (_, health) = call(&app, "GET", "/health", None, None).await;
    let s = health.to_string();
    assert!(
        !s.contains("postgres://")
            && !s.to_lowercase().contains("api_key")
            && !s.to_lowercase().contains("password")
    );
}
