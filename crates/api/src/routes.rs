use crate::{
    error::{ApiError, code_for_reason},
    state::AppState,
};
use axum::{
    Extension, Json, Router,
    extract::{FromRequest, Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use finguard_agent::{ClarifyAnswer, view};
use finguard_domain::*;
use finguard_groq::MockLlmFailure;
use finguard_jev::MockFailure;
use finguard_persistence::{
    actions, executor::ConfirmOutcome, facts, reads, seed, trace as trace_mod,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
use tracing::Instrument;

/// JSON body extractor whose failures use the standard error schema (and carry
/// the request's trace id) instead of axum's plain-text rejection.
pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        let trace = req
            .extensions()
            .get::<RequestCtx>()
            .map(|c| c.trace_id)
            .unwrap_or_default();
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(ApiJson(v)),
            Err(_) => Err(ApiError::invalid(
                "the request body is missing, too large, or not valid JSON for this endpoint",
                trace,
            )),
        }
    }
}

async fn not_found(Extension(ctx): Extension<RequestCtx>) -> ApiError {
    ApiError::new(
        ErrorCode::NotFound,
        ErrorCode::NotFound.default_message(),
        ctx.trace_id,
    )
}

/// Per-request identifiers, also attached to the tracing span.
#[derive(Clone, Copy)]
pub struct RequestCtx {
    pub request_id: uuid::Uuid,
    pub trace_id: TraceId,
}

async fn request_context(mut req: Request, next: Next) -> Response {
    let ctx = RequestCtx {
        request_id: uuid::Uuid::now_v7(),
        trace_id: TraceId::new(),
    };
    req.extensions_mut().insert(ctx);
    let span = tracing::info_span!(
        "http",
        request_id = %ctx.request_id,
        trace_id = %ctx.trace_id,
        method = %req.method(),
        path = %req.uri().path(),
    );
    let mut resp = next.run(req).instrument(span).await;
    if let Ok(v) = HeaderValue::from_str(&ctx.request_id.to_string()) {
        resp.headers_mut().insert("x-request-id", v);
    }
    if let Ok(v) = HeaderValue::from_str(&ctx.trace_id.to_string()) {
        resp.headers_mut().insert("x-trace-id", v);
    }
    // Conservative defaults for an API that is only ever called by our own UI.
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    resp.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    resp
}

pub fn router(state: AppState, cors_origin: &str) -> Router {
    let cors = match HeaderValue::from_str(cors_origin) {
        Ok(origin) => CorsLayer::new()
            .allow_origin(origin)
            .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
            .allow_headers([
                header::CONTENT_TYPE,
                axum::http::HeaderName::from_static("idempotency-key"),
            ])
            .expose_headers([
                axum::http::HeaderName::from_static("x-trace-id"),
                axum::http::HeaderName::from_static("x-request-id"),
            ]),
        Err(_) => CorsLayer::new(),
    };
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/account", get(get_account))
        .route("/api/v1/cards", get(get_cards))
        .route("/api/v1/recipients", get(get_recipients))
        .route("/api/v1/transactions", get(get_transactions))
        .route("/api/v1/agent/commands", post(post_command))
        .route("/api/v1/actions", get(list_actions))
        .route("/api/v1/actions/{id}", get(get_action))
        .route("/api/v1/actions/{id}/confirm", post(post_confirm))
        .route("/api/v1/actions/{id}/cancel", post(post_cancel))
        .route("/api/v1/actions/{id}/clarify", post(post_clarify))
        .route("/api/v1/actions/{id}/trace", get(get_trace))
        .route("/api/v1/demo/state", get(demo_state))
        .route("/api/v1/demo/reset", post(demo_reset))
        .route("/api/v1/demo/scenarios/{scenario}", post(demo_scenario))
        .route(
            "/api/v1/demo/failures/{provider}/{mode}",
            post(demo_failure),
        )
        .fallback(not_found)
        .layer(RequestBodyLimitLayer::new(16 * 1024))
        .layer(middleware::from_fn(request_context))
        .layer(cors)
        .with_state(state)
}

// ---------------------------------------------------------------- helpers --

type ApiResult<T> = Result<T, ApiError>;

async fn actor(state: &AppState, ctx: &RequestCtx) -> ApiResult<ActorContext> {
    let mut c = state
        .store
        .pool()
        .acquire()
        .await
        .map_err(|e| db_err(e.into(), ctx))?;
    facts::load_actor(&mut c, seed::SESSION)
        .await
        .map_err(|e| match e {
            finguard_persistence::StoreError::NotFound => ApiError::new(
                ErrorCode::Unauthorized,
                "no demo session; call POST /api/v1/demo/reset to seed demo data",
                ctx.trace_id,
            ),
            other => db_err(other, ctx),
        })
}

fn db_err(e: finguard_persistence::StoreError, ctx: &RequestCtx) -> ApiError {
    ApiError::from_agent(e.into(), ctx.trace_id)
}

fn parse_id<T: std::str::FromStr>(s: &str, what: &str, ctx: &RequestCtx) -> ApiResult<T> {
    s.parse()
        .map_err(|_| ApiError::invalid(format!("invalid {what} id"), ctx.trace_id))
}

fn idem_key(headers: &HeaderMap, ctx: &RequestCtx) -> ApiResult<Option<IdempotencyKey>> {
    match headers.get("idempotency-key") {
        None => Ok(None),
        Some(v) => {
            let s = v
                .to_str()
                .map_err(|_| ApiError::invalid("Idempotency-Key must be ASCII", ctx.trace_id))?;
            IdempotencyKey::new(s)
                .map(Some)
                .map_err(|e| ApiError::invalid(e.to_string(), ctx.trace_id))
        }
    }
}

async fn render(
    state: &AppState,
    action: &actions::ActionRow,
    conf: Option<&finguard_persistence::confirmations::ConfirmationRow>,
    ctx: &RequestCtx,
) -> ApiResult<Value> {
    let mut c = state
        .store
        .pool()
        .acquire()
        .await
        .map_err(|e| db_err(e.into(), ctx))?;
    let mut v = view::action_view(&mut c, action, conf, &state.agent.config().thresholds)
        .await
        .map_err(|e| ApiError::from_agent(e, ctx.trace_id))?;
    // A policy BLOCK is a deterministic *outcome*, delivered with the same
    // error schema the UI already understands.
    if action.status == "BLOCKED" {
        if let Some(Ok(reason)) = action
            .hard_policy
            .as_ref()
            .and_then(|h| h.get("result"))
            .and_then(|r| r.get("reason").cloned())
            .map(serde_json::from_value::<HardPolicyReason>)
        {
            let code = code_for_reason(reason);
            v["error"] = json!({"code": code.as_str(), "message": code.default_message(), "trace_id": action.trace()});
        } else if action.policy_decision.as_deref() == Some("BLOCK") {
            v["error"] = json!({"code": ErrorCode::PolicyBlocked.as_str(), "message": ErrorCode::PolicyBlocked.default_message(), "trace_id": action.trace()});
        }
    }
    if action.status == "MANUAL_REVIEW" {
        v["notice"] = json!({"title": "ACTION HELD FOR REVIEW", "message": "No funds moved."});
    }
    Ok(v)
}

// --------------------------------------------------------------- handlers --

async fn health(State(s): State<AppState>) -> impl IntoResponse {
    let db = sqlx::query("SELECT 1")
        .execute(s.store.pool())
        .await
        .is_ok();
    let body = json!({
        "status": if db { "ok" } else { "degraded" },
        "database": if db { "ok" } else { "unreachable" },
        "app_env": s.app_env,
        "providers": *s.provider_summary,
    });
    (
        if db {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        Json(body),
    )
}

async fn get_account(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> ApiResult<Json<Value>> {
    let a = actor(&s, &ctx).await?;
    let mut c = s
        .store
        .pool()
        .acquire()
        .await
        .map_err(|e| db_err(e.into(), &ctx))?;
    let acct = reads::account(&mut c, a.user_id)
        .await
        .map_err(|e| db_err(e, &ctx))?;
    let profile = reads::profile(&mut c, &a)
        .await
        .map_err(|e| db_err(e, &ctx))?;
    Ok(Json(json!({"account": acct, "profile": profile})))
}

async fn get_cards(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> ApiResult<Json<Value>> {
    let a = actor(&s, &ctx).await?;
    let mut c = s
        .store
        .pool()
        .acquire()
        .await
        .map_err(|e| db_err(e.into(), &ctx))?;
    Ok(Json(
        json!({"cards": reads::cards(&mut c, a.user_id).await.map_err(|e| db_err(e, &ctx))?}),
    ))
}

async fn get_recipients(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> ApiResult<Json<Value>> {
    let a = actor(&s, &ctx).await?;
    let mut c = s
        .store
        .pool()
        .acquire()
        .await
        .map_err(|e| db_err(e.into(), &ctx))?;
    Ok(Json(
        json!({"recipients": reads::recipients(&mut c, a.user_id).await.map_err(|e| db_err(e, &ctx))?}),
    ))
}

async fn get_transactions(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> ApiResult<Json<Value>> {
    let a = actor(&s, &ctx).await?;
    let mut c = s
        .store
        .pool()
        .acquire()
        .await
        .map_err(|e| db_err(e.into(), &ctx))?;
    Ok(Json(
        json!({"transactions": reads::transactions(&mut c, a.user_id, 50).await.map_err(|e| db_err(e, &ctx))?}),
    ))
}

#[derive(Deserialize)]
struct CommandBody {
    text: String,
}

async fn post_command(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<CommandBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let a = actor(&s, &ctx).await?;
    let key = idem_key(&headers, &ctx)?;
    let out = s
        .agent
        .handle_command(&a, &body.text, key, ctx.trace_id)
        .await
        .map_err(|e| ApiError::from_agent(e, ctx.trace_id))?;
    let v = render(&s, &out.action, out.confirmation.as_ref(), &ctx).await?;
    let mut v = v;
    v["replayed"] = json!(out.replayed);
    Ok((
        if out.replayed {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(v),
    ))
}

async fn list_actions(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> ApiResult<Json<Value>> {
    let a = actor(&s, &ctx).await?;
    let rows: Vec<(uuid::Uuid, uuid::Uuid, String, String, Option<String>, String, time::OffsetDateTime)> = sqlx::query_as(
        "SELECT id, trace_id, kind, status, policy_decision, user_text, created_at FROM actions WHERE user_id = $1 ORDER BY created_at DESC LIMIT 30",
    )
    .bind(a.user_id.as_uuid())
    .fetch_all(s.store.pool())
    .await
    .map_err(|e| db_err(e.into(), &ctx))?;
    let items: Vec<Value> = rows
        .into_iter()
        .map(|(id, tr, kind, status, dec, text, at)| {
            json!({"action_id": ActionId(id), "trace_id": TraceId(tr), "kind": kind, "status": status,
                   "policy_decision": dec, "user_request": text,
                   "created_at": at.format(&time::format_description::well_known::Rfc3339).unwrap_or_default()})
        })
        .collect();
    Ok(Json(json!({"actions": items})))
}

async fn get_action(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let a = actor(&s, &ctx).await?;
    let aid: ActionId = parse_id(&id, "action", &ctx)?;
    let mut c = s
        .store
        .pool()
        .acquire()
        .await
        .map_err(|e| db_err(e.into(), &ctx))?;
    let row = actions::get(&mut c, aid)
        .await
        .map_err(|e| db_err(e, &ctx))?;
    if row.user_id != a.user_id.as_uuid() {
        return Err(ApiError::new(
            ErrorCode::Forbidden,
            ErrorCode::Forbidden.default_message(),
            ctx.trace_id,
        ));
    }
    let conf = finguard_persistence::confirmations::get_by_action(&mut c, aid, false)
        .await
        .map_err(|e| db_err(e, &ctx))?;
    drop(c);
    Ok(Json(render(&s, &row, conf.as_ref(), &ctx).await?))
}

#[derive(Deserialize)]
struct ConfirmBody {
    confirmation_id: String,
}

async fn post_confirm(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<ConfirmBody>,
) -> ApiResult<Json<Value>> {
    let a = actor(&s, &ctx).await?;
    let aid: ActionId = parse_id(&id, "action", &ctx)?;
    let cid: ConfirmationId = parse_id(&body.confirmation_id, "confirmation", &ctx)?;
    let key = idem_key(&headers, &ctx)?;
    match s
        .agent
        .confirm(&a, aid, cid, key)
        .await
        .map_err(|e| ApiError::from_agent(e, ctx.trace_id))?
    {
        ConfirmOutcome::Executed { action, replayed } => {
            let mut v = render(&s, &action, None, &ctx).await?;
            v["replayed"] = json!(replayed);
            Ok(Json(v))
        }
        ConfirmOutcome::Blocked { reason, .. } => {
            // Revalidation inside the DB transaction failed: nothing moved.
            let code = code_for_reason(reason);
            Err(ApiError::new(
                code,
                format!(
                    "{} (re-checked at execution time; no funds moved)",
                    code.default_message()
                ),
                ctx.trace_id,
            ))
        }
    }
}

async fn post_cancel(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let a = actor(&s, &ctx).await?;
    let aid: ActionId = parse_id(&id, "action", &ctx)?;
    let row = s
        .agent
        .cancel(&a, aid)
        .await
        .map_err(|e| ApiError::from_agent(e, ctx.trace_id))?;
    Ok(Json(render(&s, &row, None, &ctx).await?))
}

#[derive(Deserialize, Default)]
struct ClarifyBody {
    #[serde(default)]
    accept_suggestion: bool,
    /// Whole pesos as text, e.g. "5000" or "5,000.50".
    amount: Option<String>,
    text: Option<String>,
}

async fn post_clarify(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    headers: HeaderMap,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<ClarifyBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let a = actor(&s, &ctx).await?;
    let aid: ActionId = parse_id(&id, "action", &ctx)?;
    let key = idem_key(&headers, &ctx)?;
    let answer = if body.accept_suggestion {
        ClarifyAnswer::AcceptSuggestion
    } else if let Some(amt) = body.amount {
        let m = Money::parse_major(&amt, Currency::Php)
            .map_err(|e| ApiError::invalid(e.to_string(), ctx.trace_id))?;
        ClarifyAnswer::AmountMinor(m.amount_minor)
    } else if let Some(t) = body.text {
        ClarifyAnswer::Text(t)
    } else {
        return Err(ApiError::invalid(
            "provide accept_suggestion, amount, or text",
            ctx.trace_id,
        ));
    };
    let out = s
        .agent
        .clarify(&a, aid, answer, key)
        .await
        .map_err(|e| ApiError::from_agent(e, ctx.trace_id))?;
    let mut v = render(&s, &out.action, out.confirmation.as_ref(), &ctx).await?;
    v["replayed"] = json!(out.replayed);
    Ok((StatusCode::CREATED, Json(v)))
}

async fn get_trace(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(id): Path<String>,
) -> ApiResult<Json<trace_mod::TraceView>> {
    let a = actor(&s, &ctx).await?;
    let aid: ActionId = parse_id(&id, "action", &ctx)?;
    let mut c = s
        .store
        .pool()
        .acquire()
        .await
        .map_err(|e| db_err(e.into(), &ctx))?;
    let row = actions::get(&mut c, aid)
        .await
        .map_err(|e| db_err(e, &ctx))?;
    if row.user_id != a.user_id.as_uuid() {
        return Err(ApiError::new(
            ErrorCode::Forbidden,
            ErrorCode::Forbidden.default_message(),
            ctx.trace_id,
        ));
    }
    Ok(Json(
        trace_mod::build(&mut c, aid)
            .await
            .map_err(|e| db_err(e, &ctx))?,
    ))
}

// ------------------------------------------------------------------ demo --

fn demo_guard(s: &AppState, ctx: &RequestCtx) -> ApiResult<()> {
    if s.is_production() {
        return Err(ApiError::new(
            ErrorCode::Forbidden,
            "demo endpoints are disabled when APP_ENV=production",
            ctx.trace_id,
        ));
    }
    Ok(())
}

async fn demo_state(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> ApiResult<Json<Value>> {
    demo_guard(&s, &ctx)?;
    Ok(Json(json!({
        "providers": *s.provider_summary,
        "failure_injection": {
            "jev": s.mock_jev.as_ref().map(|m| m.current_failure().map(|f| format!("{f:?}"))),
            "available": s.mock_jev.is_some(),
        },
    })))
}

async fn demo_reset(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
) -> ApiResult<Json<Value>> {
    demo_guard(&s, &ctx)?;
    if let Some(m) = &s.mock_jev {
        m.fail_with(None);
    }
    if let Some(m) = &s.mock_groq {
        m.fail_with(None);
    }
    seed::reset_demo(s.store.pool(), &seed::DemoSetup::default())
        .await
        .map_err(|e| db_err(e, &ctx))?;
    Ok(Json(json!({"status": "reset", "scenario": "normal"})))
}

async fn demo_scenario(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path(name): Path<String>,
) -> ApiResult<Json<Value>> {
    demo_guard(&s, &ctx)?;
    let sc = seed::DemoScenario::parse(&name).ok_or_else(|| {
        ApiError::invalid(
            "unknown scenario (normal | suspicious | low_balance)",
            ctx.trace_id,
        )
    })?;
    if let Some(m) = &s.mock_jev {
        m.fail_with(None);
    }
    if let Some(m) = &s.mock_groq {
        m.fail_with(None);
    }
    seed::reset_demo(s.store.pool(), &sc.setup())
        .await
        .map_err(|e| db_err(e, &ctx))?;
    Ok(Json(json!({"status": "applied", "scenario": name})))
}

/// `POST /api/v1/demo/failures/{jev|groq}/{timeout|429|5xx|invalid_response|none}`
async fn demo_failure(
    State(s): State<AppState>,
    Extension(ctx): Extension<RequestCtx>,
    Path((provider, mode)): Path<(String, String)>,
) -> ApiResult<Json<Value>> {
    demo_guard(&s, &ctx)?;
    let none = mode == "none";
    match provider.as_str() {
        "jev" => {
            let m = s.mock_jev.as_ref().ok_or_else(|| ApiError::invalid("Jev is a real provider; failure injection only works with DECISION_PROVIDER=mock", ctx.trace_id))?;
            m.fail_with(if none {
                None
            } else {
                Some(
                    MockFailure::parse(&mode)
                        .ok_or_else(|| ApiError::invalid("unknown failure mode", ctx.trace_id))?,
                )
            });
        }
        "groq" => {
            let m = s.mock_groq.as_ref().ok_or_else(|| ApiError::invalid("Groq is a real provider; failure injection only works with ESCALATION_ANALYZER=mock", ctx.trace_id))?;
            m.fail_with(if none {
                None
            } else {
                Some(
                    MockLlmFailure::parse(&mode)
                        .ok_or_else(|| ApiError::invalid("unknown failure mode", ctx.trace_id))?,
                )
            });
        }
        _ => {
            return Err(ApiError::invalid(
                "provider must be jev or groq",
                ctx.trace_id,
            ));
        }
    }
    Ok(Json(json!({"provider": provider, "failure": mode})))
}
