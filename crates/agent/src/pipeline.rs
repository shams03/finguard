use crate::{deterministic::DeterministicIntentParser, error::AgentError};
use finguard_domain::*;
use finguard_persistence::{
    Store,
    actions::{self, ActionRow, NewAction},
    audit::{self, ev},
    confirmations::{self, ConfirmationRow},
    evals::{self, GroqRecord, JevRecord},
    executor::{self, ConfirmOutcome, ConfirmParams},
    facts::{self, RecipientInfo, RecipientResolution},
    idempotency::{self, Claim},
};
use finguard_policy::*;
use serde_json::{Value, json};
use std::{sync::Arc, time::Instant};
use tracing::Instrument;

#[derive(Clone)]
pub struct AgentConfig {
    pub thresholds: ConfidenceThresholds,
    pub limits: HardLimits,
    pub confirmation_ttl_secs: i64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            thresholds: ConfidenceThresholds::default(),
            limits: HardLimits::default(),
            confirmation_ttl_secs: 300,
        }
    }
}

pub struct AgentDeps {
    pub store: Store,
    pub parser: Arc<dyn IntentParser>,
    pub decision: Arc<dyn DecisionProvider>,
    pub escalation: Arc<dyn EscalationAnalyzer>,
}

pub struct Agent {
    deps: AgentDeps,
    cfg: AgentConfig,
}

#[derive(Debug)]
pub struct CommandOutcome {
    pub action: ActionRow,
    pub confirmation: Option<ConfirmationRow>,
    /// True when an idempotent retry returned a previously stored result.
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub enum ClarifyAnswer {
    /// "Yes, send the amount you suggested."
    AcceptSuggestion,
    AmountMinor(i64),
    Text(String),
}

enum RecipientState {
    NoneGiven,
    Resolved(RecipientInfo),
    Unresolved,
    Ambiguous,
}

fn ms(t: Instant) -> u64 {
    t.elapsed().as_millis() as u64
}

fn first_name(full: &str) -> String {
    full.split_whitespace().next().unwrap_or(full).to_owned()
}

impl Agent {
    pub fn new(deps: AgentDeps, cfg: AgentConfig) -> Self {
        Self { deps, cfg }
    }
    pub fn store(&self) -> &Store {
        &self.deps.store
    }
    pub fn config(&self) -> &AgentConfig {
        &self.cfg
    }

    // ------------------------------------------------------------ command --

    pub async fn handle_command(
        &self,
        actor: &ActorContext,
        text: &str,
        key: Option<IdempotencyKey>,
        trace: TraceId,
    ) -> Result<CommandOutcome, AgentError> {
        let span = tracing::info_span!("command", trace_id = %trace, user = %actor.user_id);
        self.command_inner(actor, text, key, trace)
            .instrument(span)
            .await
    }

    async fn command_inner(
        &self,
        actor: &ActorContext,
        text: &str,
        key: Option<IdempotencyKey>,
        trace: TraceId,
    ) -> Result<CommandOutcome, AgentError> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > 500 {
            return Err(AgentError::invalid("command must be 1-500 characters"));
        }
        let pool = self.deps.store.pool();

        if let Some(k) = &key {
            let fp = idempotency::command_fingerprint(actor.user_id, text);
            let mut c = pool.acquire().await?;
            match idempotency::claim(&mut c, actor.user_id, "command", k, &fp).await? {
                Claim::Fresh => {}
                Claim::Existing(rec) => {
                    let Some(id) = rec.action_id.filter(|_| rec.status == "COMPLETED") else {
                        return Err(AgentError::new(
                            ErrorCode::IdempotencyConflict,
                            "a request with this key is still being processed",
                        ));
                    };
                    let action = actions::get(&mut c, ActionId(id)).await?;
                    audit::record(
                        &mut c,
                        action.trace(),
                        Some(action.action_id()),
                        ev::IDEMPOTENT_REPLAY,
                        json!({"scope": "command"}),
                    )
                    .await?;
                    let confirmation =
                        confirmations::get_by_action(&mut c, action.action_id(), false).await?;
                    return Ok(CommandOutcome {
                        action,
                        confirmation,
                        replayed: true,
                    });
                }
            }
        }

        let result = self.run_new_command(actor, text, trace).await;

        if let Some(k) = &key {
            let mut c = pool.acquire().await?;
            match &result {
                Ok(o) => {
                    idempotency::complete(
                        &mut c,
                        actor.user_id,
                        "command",
                        k,
                        o.action.action_id(),
                        &json!({"status": o.action.status}),
                    )
                    .await?;
                }
                Err(_) => idempotency::release(&mut c, actor.user_id, "command", k).await?,
            }
        }
        result
    }

    async fn run_new_command(
        &self,
        actor: &ActorContext,
        text: &str,
        trace: TraceId,
    ) -> Result<CommandOutcome, AgentError> {
        let pool = self.deps.store.pool();
        let safe_text = scrub_text(text, 280);
        {
            let mut c = pool.acquire().await?;
            audit::record(
                &mut c,
                trace,
                None,
                ev::REQUEST_RECEIVED,
                json!({"chars": text.chars().count()}),
            )
            .await?;
        }

        // ---- 1. language -> structured intent ------------------------------
        let recipients = {
            let mut c = pool.acquire().await?;
            facts::list_recipients(&mut c, actor.user_id).await?
        };
        let ictx = IntentContext {
            default_currency: Currency::Php,
            known_recipient_names: recipients.iter().map(|r| r.display_name.clone()).collect(),
        };
        let parser = self.deps.parser.name();
        {
            let mut c = pool.acquire().await?;
            audit::record(
                &mut c,
                trace,
                None,
                ev::INTENT_PARSING_STARTED,
                json!({"parser": parser}),
            )
            .await?;
        }
        let t = Instant::now();
        let mut used_parser = parser;
        let parsed = match self.deps.parser.parse(text, &ictx).await {
            Ok(p) => p,
            Err(IntentError::Empty) => return Err(AgentError::invalid("empty command")),
            Err(IntentError::Llm(e)) => {
                // The LLM parser is a convenience. Record the failure, then fall
                // back to the deterministic parser so simple commands still work.
                tracing::warn!(
                    code = e.code(),
                    "LLM intent parsing failed; using deterministic parser"
                );
                let mut c = pool.acquire().await?;
                evals::insert_groq(
                    &mut c,
                    &GroqRecord {
                        action_id: None,
                        trace_id: trace,
                        purpose: "INTENT_PARSING",
                        schema_version: INTENT_SCHEMA_VERSION,
                        status: e.code(),
                        meta: None,
                        result: None,
                        latency_ms: ms(t),
                    },
                )
                .await?;
                audit::record(&mut c, trace, None, ev::INTENT_PARSE_FAILED, json!({"parser": parser, "code": e.code(), "duration_ms": ms(t), "fallback": "deterministic"})).await?;
                used_parser = "deterministic (fallback)";
                DeterministicIntentParser
                    .parse(text, &ictx)
                    .await
                    .map_err(|_| AgentError::invalid("empty command"))?
            }
        };
        let parse_ms = ms(t);
        {
            let mut c = pool.acquire().await?;
            if let Some(meta) = &parsed.meta {
                evals::insert_groq(
                    &mut c,
                    &GroqRecord {
                        action_id: None, trace_id: trace, purpose: "INTENT_PARSING", schema_version: INTENT_SCHEMA_VERSION,
                        status: "OK", meta: Some(meta),
                        result: Some(json!({
                            "intent_type": parsed.intent_type, "recipient_reference": parsed.recipient_reference,
                            "amount_minor": parsed.amount_minor, "missing_fields": parsed.missing_fields,
                            "ambiguity": parsed.ambiguity,
                        })),
                        latency_ms: meta.latency_ms,
                    },
                )
                .await?;
            }
            audit::record(
                &mut c, trace, None, ev::INTENT_PARSED,
                json!({
                    "parser": used_parser, "schema_version": INTENT_SCHEMA_VERSION, "intent_type": parsed.intent_type,
                    "ambiguity": parsed.ambiguity, "missing_fields": parsed.missing_fields,
                    "model": parsed.meta.as_ref().map(|m| m.model.clone()), "duration_ms": parse_ms,
                }),
            )
            .await?;
        }

        // ---- 2. structured intent -> typed draft (allow-listed tools only) ---
        let (draft, rstate) = {
            let mut c = pool.acquire().await?;
            self.build_draft(&mut c, actor, &parsed).await?
        };
        let Some(draft) = draft else {
            let mut c = pool.acquire().await?;
            audit::record(
                &mut c,
                trace,
                None,
                ev::AUTHORIZATION_FAILED,
                json!({"reason": "unsupported or unknown action"}),
            )
            .await?;
            return Err(AgentError::new(
                ErrorCode::UnknownAction,
                "I can only check your balance, freeze your card, or send a transfer.",
            ));
        };
        self.process_draft(
            actor,
            trace,
            draft,
            &safe_text,
            None,
            rstate,
            parsed.refers_to_usual_amount,
        )
        .await
    }

    async fn build_draft(
        &self,
        c: &mut sqlx::PgConnection,
        actor: &ActorContext,
        parsed: &ParsedIntent,
    ) -> Result<(Option<ActionDraft>, RecipientState), AgentError> {
        let (account_id, card_id) = facts::primary_ids(c, actor.user_id).await?;
        let mut rstate = RecipientState::NoneGiven;
        let draft = match parsed.intent_type {
            IntentKind::ReadBalance => Some(ActionDraft::ReadBalance { account_id }),
            IntentKind::FreezeCard => Some(ActionDraft::FreezeCard { card_id }),
            IntentKind::TransferFunds => {
                let mut rid = None;
                if let Some(r) = &parsed.recipient_reference {
                    match facts::resolve_recipient(c, actor.user_id, r).await? {
                        RecipientResolution::Found(info) => {
                            rid = Some(info.id);
                            rstate = RecipientState::Resolved(info);
                        }
                        RecipientResolution::NotFound => rstate = RecipientState::Unresolved,
                        RecipientResolution::Ambiguous(_) => rstate = RecipientState::Ambiguous,
                    }
                }
                let cur = parsed.currency.unwrap_or(Currency::Php);
                Some(ActionDraft::TransferFunds {
                    from_account: account_id,
                    recipient_id: rid,
                    amount: parsed.amount_minor.map(|a| Money::new(a, cur)),
                })
            }
            IntentKind::Unsupported => None,
        };
        Ok((draft, rstate))
    }

    /// Everything after a draft exists. Used for fresh commands and for
    /// clarified re-proposals, so a clarification *re-runs the whole policy*.
    #[allow(clippy::too_many_arguments)]
    async fn process_draft(
        &self,
        actor: &ActorContext,
        trace: TraceId,
        draft: ActionDraft,
        safe_text: &str,
        parent: Option<ActionId>,
        rstate: RecipientState,
        refers_to_usual: bool,
    ) -> Result<CommandOutcome, AgentError> {
        let pool = self.deps.store.pool();
        let tool = tool_spec(draft.kind());

        // ---- create the action row --------------------------------------------
        let action = {
            let mut c = pool.acquire().await?;
            let a = actions::create(
                &mut c,
                &NewAction {
                    trace_id: trace,
                    actor,
                    parent,
                    draft: &draft,
                    user_text: safe_text,
                },
            )
            .await?;
            audit::record(
                &mut c, trace, Some(a.action_id()), ev::ACTION_CREATED,
                json!({"kind": draft.kind(), "risk_class": draft.risk_class(), "missing_fields": draft.missing_fields(),
                       "parent_action_id": parent.map(|p| p.to_string()), "tool": {"executor": tool.executor, "requires_confirmation": tool.requires_confirmation, "requires_jev": tool.requires_jev}}),
            )
            .await?;
            actions::transition(
                &mut c,
                a.action_id(),
                ActionStatus::Proposed,
                ActionStatus::Validating,
            )
            .await?;
            a
        };
        let aid = action.action_id();
        let span = tracing::info_span!("action", action_id = %aid);
        self.decide_and_apply(actor, trace, action, draft, rstate, refers_to_usual)
            .instrument(span)
            .await
    }

    async fn decide_and_apply(
        &self,
        actor: &ActorContext,
        trace: TraceId,
        action: ActionRow,
        draft: ActionDraft,
        rstate: RecipientState,
        refers_to_usual: bool,
    ) -> Result<CommandOutcome, AgentError> {
        let pool = self.deps.store.pool();
        let aid = action.action_id();
        let tool = tool_spec(draft.kind());

        // ---- 3. authorization + hard policy (before any AI) ---------------------
        let t = Instant::now();
        let mut c = pool.acquire().await?;
        let mut hf = facts::load_hard_facts(&mut c, &draft).await?;
        if matches!(rstate, RecipientState::Unresolved) {
            hf.recipient = RecipientLookup::NotFound; // a reference was given, nothing matched
        }
        let report = evaluate(Some(actor), &draft, &hf, &self.cfg.limits);
        actions::set_hard_policy(&mut c, aid, &report).await?;
        let auth_failed = report.checks.iter().any(|ch| {
            ch.status == CheckStatus::Fail
                && matches!(
                    ch.reason,
                    Some(
                        HardPolicyReason::NotAuthenticated
                            | HardPolicyReason::MissingPermission
                            | HardPolicyReason::AccountNotOwned
                            | HardPolicyReason::CardNotOwned
                    )
                )
        });
        audit::record(
            &mut c,
            trace,
            Some(aid),
            if auth_failed {
                ev::AUTHORIZATION_FAILED
            } else {
                ev::AUTHORIZATION_PASSED
            },
            json!({}),
        )
        .await?;
        let hard_ms = ms(t);
        if let HardPolicyResult::Block(reason) = report.result {
            audit::record(&mut c, trace, Some(aid), ev::HARD_POLICY_BLOCKED, json!({"reason": reason.code(), "duration_ms": hard_ms,
                "failed": report.checks.iter().filter(|x| x.status == CheckStatus::Fail).map(|x| x.name.clone()).collect::<Vec<_>>()})).await?;
            audit::record(
                &mut c,
                trace,
                Some(aid),
                ev::JEV_SKIPPED,
                json!({"reason": "hard policy blocked first; no AI call made"}),
            )
            .await?;
            let decision = PolicyDecision::Block {
                reason: BlockReason::HardPolicy(reason),
            };
            let reasons = json!([{"code": "HARD_POLICY_BLOCK", "detail": reason.code()}]);
            actions::set_decision(&mut c, aid, &decision, &reasons).await?;
            audit::record(&mut c, trace, Some(aid), ev::POLICY_DECISION, json!({"decision": decision.code(), "reasons": reasons, "policy_version": POLICY_VERSION})).await?;
            actions::transition(&mut c, aid, ActionStatus::Validating, ActionStatus::Blocked)
                .await?;
            let fresh = actions::get(&mut c, aid).await?;
            return Ok(CommandOutcome {
                action: fresh,
                confirmation: None,
                replayed: false,
            });
        }
        audit::record(
            &mut c,
            trace,
            Some(aid),
            ev::HARD_POLICY_PASSED,
            json!({"duration_ms": hard_ms, "checks": report.checks.len(),
            "skipped": report.checks.iter().filter(|x| x.status == CheckStatus::Skipped).count()}),
        )
        .await?;
        actions::transition(
            &mut c,
            aid,
            ActionStatus::Validating,
            ActionStatus::RiskEvaluating,
        )
        .await?;

        // ---- 4. risk assessment (Jev) — only if policy needs it -----------------
        let mut signals = DeterministicSignals {
            recipient_known: true,
            recipient_age_days: None,
            amount_to_typical_ratio: None,
            device_trusted: true,
            location_familiar: true,
            transfers_last_hour: 0,
        };
        let mut risk_ctx: Option<RiskContext> = None;
        let mut recipient_first_name = match &rstate {
            RecipientState::Resolved(i) => Some(first_name(&i.display_name)),
            _ => None,
        };
        if draft.kind() == ActionKind::TransferFunds {
            let rf = facts::load_risk_facts(&mut c, actor, &draft, &action.user_text).await?;
            if recipient_first_name.is_none() {
                recipient_first_name = rf.recipient_display_name.as_deref().map(first_name);
            }
            let ctx = RiskContext::from_facts(&rf); // explicit minimisation step
            signals = DeterministicSignals {
                recipient_known: ctx.recipient_known,
                recipient_age_days: ctx.recipient_age_days,
                amount_to_typical_ratio: ctx.amount_to_typical_ratio,
                device_trusted: ctx.device_trusted,
                location_familiar: ctx.location_familiar,
                transfers_last_hour: ctx.transfers_in_last_hour,
            };
            risk_ctx = Some(ctx);
        }
        let jev = if tool.requires_jev {
            let ctx = risk_ctx
                .as_ref()
                .expect("transfers always build a risk context");
            audit::record(&mut c, trace, Some(aid), ev::JEV_STARTED, json!({"provider": self.deps.decision.name(), "question_set_version": RISK_QUESTION_SET_VERSION})).await?;
            drop(c); // do not hold a DB connection across a network call
            let t = Instant::now();
            let res = self.deps.decision.assess(ctx).await;
            let took = ms(t);
            let mut c2 = pool.acquire().await?;
            let status = match res {
                Ok(a) => {
                    evals::insert_jev(
                        &mut c2,
                        &JevRecord {
                            action_id: aid,
                            trace_id: trace,
                            provider: self.deps.decision.name(),
                            status: "OK",
                            assessment: Some(&a),
                            error: None,
                            latency_ms: took,
                        },
                    )
                    .await?;
                    audit::record(&mut c2, trace, Some(aid), ev::JEV_COMPLETED, json!({
                        "duration_ms": took, "overall_confidence": a.overall_confidence, "model": a.provider_metadata.model,
                        "provider": a.provider_metadata.provider, "credits_charged": a.provider_metadata.credits_charged,
                    })).await?;
                    JevStatus::Assessed(a)
                }
                Err(e) => {
                    tracing::warn!(
                        code = e.code(),
                        "decision provider failed; policy will fail closed"
                    );
                    evals::insert_jev(
                        &mut c2,
                        &JevRecord {
                            action_id: aid,
                            trace_id: trace,
                            provider: self.deps.decision.name(),
                            status: e.code(),
                            assessment: None,
                            error: Some(&e),
                            latency_ms: took,
                        },
                    )
                    .await?;
                    audit::record(
                        &mut c2,
                        trace,
                        Some(aid),
                        ev::JEV_FAILED,
                        json!({"code": e.code(), "duration_ms": took}),
                    )
                    .await?;
                    JevStatus::Unavailable(e)
                }
            };
            c = c2;
            status
        } else {
            audit::record(&mut c, trace, Some(aid), ev::JEV_SKIPPED, json!({"reason": "not required for this action class (deterministic code is enough)"})).await?;
            JevStatus::NotRequired
        };

        // ---- 5. policy stage one, optional escalation, final decision ------------
        let missing = draft.missing_fields();
        let pt = Instant::now();
        let pi = PolicyInput {
            tool,
            hard: &report,
            missing_fields: &missing,
            jev: &jev,
            signals: &signals,
            thresholds: &self.cfg.thresholds,
        };
        let mut candidate_minor: Option<i64> = None;
        let outcome = match stage_one(&pi) {
            Stage::Final(o) => o,
            Stage::Escalate { reason, reasons } => {
                let confidence = match &jev {
                    JevStatus::Assessed(a) => Some(a.overall_confidence),
                    _ => None,
                };
                audit::record(&mut c, trace, Some(aid), ev::CONFIDENCE_ESCALATION_TRIGGERED, json!({
                    "reason": reason.code(), "confidence": confidence, "threshold": self.cfg.thresholds.for_class(tool.risk_class),
                    "note": "low confidence means extra handling, not that the second model decides",
                })).await?;

                // Backend-derived evidence the clarification may cite (never model-invented).
                let mut candidate = None;
                let mut evidence = None;
                if refers_to_usual
                    && missing.contains(&MissingField::Amount)
                    && let RecipientState::Resolved(info) = &rstate
                    && let Some(u) = facts::usual_amount(&mut c, info.id).await?
                {
                    evidence = Some(format!(
                        "median of your last {} transfers to this recipient",
                        u.sample_size
                    ));
                    candidate_minor = Some(u.amount.amount_minor);
                    candidate = Some(u.amount);
                }
                let input = EscalationInput {
                    reason,
                    user_request: action.user_text.clone(),
                    proposed_action: risk_ctx
                        .as_ref()
                        .map(|r| r.proposed_action.clone())
                        .unwrap_or_else(|| draft.kind().label().to_owned()),
                    missing_fields: missing.clone(),
                    jev_summary: match &jev {
                        JevStatus::Assessed(a) => Some(jev_summary(a)),
                        _ => None,
                    },
                    recipient_first_name: recipient_first_name.clone(),
                    candidate_amount: candidate,
                    candidate_evidence: evidence,
                };
                audit::record(&mut c, trace, Some(aid), ev::GROQ_ESCALATION_STARTED, json!({"analyzer": self.deps.escalation.name(), "reason": reason.code(), "schema_version": ESCALATION_SCHEMA_VERSION})).await?;
                drop(c);
                let t = Instant::now();
                let res = self.deps.escalation.analyze(input.clone()).await;
                let took = ms(t);
                let mut c2 = pool.acquire().await?;
                let analysis = match res {
                    Ok(a) => {
                        evals::insert_groq(&mut c2, &GroqRecord {
                            action_id: Some(aid), trace_id: trace, purpose: "ESCALATION_ANALYSIS", schema_version: ESCALATION_SCHEMA_VERSION,
                            status: "OK", meta: a.meta.as_ref(),
                            result: Some(json!({
                                "intent_supported": a.intent_supported, "ambiguity_detected": a.ambiguity_detected,
                                "missing_information": a.missing_information, "clarification_needed": a.clarification_needed,
                                "clarification_question": a.clarification_question, "recommended_handling": a.recommended_handling,
                            })),
                            latency_ms: a.meta.as_ref().map(|m| m.latency_ms).unwrap_or(took),
                        }).await?;
                        audit::record(&mut c2, trace, Some(aid), ev::GROQ_ESCALATION_COMPLETED, json!({
                            "duration_ms": took, "recommended_handling": a.recommended_handling, "model": a.meta.as_ref().map(|m| m.model.clone()),
                        })).await?;
                        Some(a)
                    }
                    Err(EscalationError::Llm(e)) => {
                        tracing::warn!(code = e.code(), "escalation analyzer failed; failing safe");
                        evals::insert_groq(
                            &mut c2,
                            &GroqRecord {
                                action_id: Some(aid),
                                trace_id: trace,
                                purpose: "ESCALATION_ANALYSIS",
                                schema_version: ESCALATION_SCHEMA_VERSION,
                                status: e.code(),
                                meta: None,
                                result: None,
                                latency_ms: took,
                            },
                        )
                        .await?;
                        audit::record(
                            &mut c2,
                            trace,
                            Some(aid),
                            ev::GROQ_ESCALATION_FAILED,
                            json!({"code": e.code(), "duration_ms": took}),
                        )
                        .await?;
                        None
                    }
                };
                c = c2;
                finalize_after_escalation(
                    &pi,
                    reason,
                    reasons,
                    analysis.as_ref(),
                    fallback_question(&input),
                )
            }
        };
        let policy_ms = ms(pt);

        // ---- 6. apply the decision ---------------------------------------------------
        let reasons_json = serde_json::to_value(&outcome.reasons).unwrap_or(Value::Null);
        actions::set_decision(&mut c, aid, &outcome.decision, &reasons_json).await?;
        audit::record(&mut c, trace, Some(aid), ev::POLICY_DECISION, json!({
            "decision": outcome.decision.code(), "reasons": reasons_json, "policy_version": POLICY_VERSION,
            "escalation": outcome.escalation.map(|e| e.code()), "duration_ms": policy_ms,
        })).await?;

        let mut confirmation = None;
        match &outcome.decision {
            PolicyDecision::Execute => {
                if draft.kind() != ActionKind::ReadBalance {
                    // Only read-only tools may be auto-executed; this is a bug guard.
                    actions::transition(
                        &mut c,
                        aid,
                        ActionStatus::RiskEvaluating,
                        ActionStatus::Failed,
                    )
                    .await?;
                    return Err(AgentError::new(
                        ErrorCode::InternalError,
                        "policy attempted to auto-execute a non read-only action",
                    ));
                }
                actions::transition(
                    &mut c,
                    aid,
                    ActionStatus::RiskEvaluating,
                    ActionStatus::Approved,
                )
                .await?;
                drop(c);
                let done =
                    executor::execute_read_balance(pool, actor, aid, &self.cfg.limits).await?;
                return Ok(CommandOutcome {
                    action: done,
                    confirmation: None,
                    replayed: false,
                });
            }
            PolicyDecision::RequireConfirmation => {
                actions::transition(
                    &mut c,
                    aid,
                    ActionStatus::RiskEvaluating,
                    ActionStatus::AwaitingConfirmation,
                )
                .await?;
                let fresh = actions::get(&mut c, aid).await?;
                let fp = fresh.fingerprint.clone().ok_or_else(|| {
                    AgentError::new(
                        ErrorCode::InternalError,
                        "confirmable action has no fingerprint",
                    )
                })?;
                let conf = confirmations::create(
                    &mut c,
                    aid,
                    actor.user_id,
                    &fp,
                    self.cfg.confirmation_ttl_secs,
                )
                .await?;
                audit::record(&mut c, trace, Some(aid), ev::CONFIRMATION_REQUESTED, json!({"confirmation_id": ConfirmationId(conf.id).to_string(), "ttl_secs": self.cfg.confirmation_ttl_secs})).await?;
                confirmation = Some(conf);
            }
            PolicyDecision::AskForClarification { question } => {
                actions::transition(
                    &mut c,
                    aid,
                    ActionStatus::RiskEvaluating,
                    ActionStatus::AwaitingClarification,
                )
                .await?;
                if let Some(m) = candidate_minor {
                    actions::set_clarification_candidate(&mut c, aid, m).await?;
                }
                audit::record(&mut c, trace, Some(aid), ev::CLARIFICATION_REQUESTED, json!({"question": question, "has_suggested_amount": candidate_minor.is_some()})).await?;
            }
            PolicyDecision::ManualReview => {
                actions::transition(
                    &mut c,
                    aid,
                    ActionStatus::RiskEvaluating,
                    ActionStatus::ManualReview,
                )
                .await?;
                audit::record(
                    &mut c,
                    trace,
                    Some(aid),
                    ev::MANUAL_REVIEW_REQUIRED,
                    json!({"reasons": reasons_json, "funds_moved": false}),
                )
                .await?;
            }
            PolicyDecision::Block { .. } => {
                actions::transition(
                    &mut c,
                    aid,
                    ActionStatus::RiskEvaluating,
                    ActionStatus::Blocked,
                )
                .await?;
            }
        }
        let fresh = actions::get(&mut c, aid).await?;
        Ok(CommandOutcome {
            action: fresh,
            confirmation,
            replayed: false,
        })
    }

    // ------------------------------------------------------- clarification --

    /// The user answers a clarification. This does **not** mutate the old
    /// proposal: it supersedes it with a brand-new action (new id, new
    /// fingerprint) and re-runs the full policy on the now-explicit request.
    pub async fn clarify(
        &self,
        actor: &ActorContext,
        action_id: ActionId,
        answer: ClarifyAnswer,
        key: Option<IdempotencyKey>,
    ) -> Result<CommandOutcome, AgentError> {
        let pool = self.deps.store.pool();
        let mut c = pool.acquire().await?;
        let parent = actions::get(&mut c, action_id).await?;
        if parent.user_id != actor.user_id.as_uuid() {
            return Err(AgentError::new(
                ErrorCode::Forbidden,
                ErrorCode::Forbidden.default_message(),
            ));
        }
        if parent.status()? != ActionStatus::AwaitingClarification {
            return Err(AgentError::invalid(format!(
                "action is {}; it is not waiting for a clarification",
                parent.status
            )));
        }
        let ActionDraft::TransferFunds {
            from_account,
            recipient_id,
            amount,
        } = parent.draft()?
        else {
            return Err(AgentError::invalid("only transfers can be clarified"));
        };

        let mut new_amount = amount;
        let mut new_recipient = recipient_id;
        match &answer {
            ClarifyAnswer::AcceptSuggestion => {
                let m = parent.clarification_candidate_minor.ok_or_else(|| {
                    AgentError::invalid(
                        "there is no suggested amount to accept; please state the amount",
                    )
                })?;
                new_amount = Some(Money::php(m));
            }
            ClarifyAnswer::AmountMinor(m) => new_amount = Some(Money::php(*m)),
            ClarifyAnswer::Text(t) => {
                if let Some(m) = amounts_mentioned(t, Currency::Php).first() {
                    new_amount = Some(*m);
                }
                if new_recipient.is_none()
                    && let RecipientResolution::Found(info) =
                        facts::resolve_recipient(&mut c, actor.user_id, t).await?
                {
                    new_recipient = Some(info.id);
                }
            }
        }

        if let Some(k) = &key {
            let fp = idempotency::command_fingerprint(
                actor.user_id,
                &format!("clarify|{}|{:?}|{:?}", parent.id, new_amount, new_recipient),
            );
            match idempotency::claim(&mut c, actor.user_id, "clarify", k, &fp).await? {
                Claim::Fresh => {}
                Claim::Existing(rec) => {
                    if let Some(id) = rec.action_id.filter(|_| rec.status == "COMPLETED") {
                        let action = actions::get(&mut c, ActionId(id)).await?;
                        let confirmation =
                            confirmations::get_by_action(&mut c, action.action_id(), false).await?;
                        return Ok(CommandOutcome {
                            action,
                            confirmation,
                            replayed: true,
                        });
                    }
                    return Err(AgentError::new(
                        ErrorCode::IdempotencyConflict,
                        "a request with this key is still being processed",
                    ));
                }
            }
        }

        let new_trace = TraceId::new();
        let result = async {
            // Supersede the old proposal; it can never be confirmed afterwards.
            executor::cancel(pool, actor, action_id, "superseded by clarified proposal").await?;
            audit::record(
                &mut *pool.acquire().await?,
                new_trace,
                None,
                ev::CLARIFICATION_ANSWERED,
                json!({"parent_action_id": action_id.to_string()}),
            )
            .await?;
            let draft = ActionDraft::TransferFunds {
                from_account,
                recipient_id: new_recipient,
                amount: new_amount,
            };
            let rstate = match new_recipient {
                Some(r) => {
                    let mut c = pool.acquire().await?;
                    facts::list_recipients(&mut c, actor.user_id)
                        .await?
                        .into_iter()
                        .find(|i| i.id == r)
                        .map(RecipientState::Resolved)
                        .unwrap_or(RecipientState::NoneGiven)
                }
                None => RecipientState::NoneGiven,
            };
            let text = match new_amount {
                Some(a) => format!("{} [clarified: {}]", parent.user_text, a.pretty()),
                None => format!("{} [clarification incomplete]", parent.user_text),
            };
            self.process_draft(
                actor,
                new_trace,
                draft,
                &scrub_text(&text, 280),
                Some(action_id),
                rstate,
                false,
            )
            .await
        }
        .await;

        if let Some(k) = &key {
            let mut c = pool.acquire().await?;
            match &result {
                Ok(o) => {
                    idempotency::complete(
                        &mut c,
                        actor.user_id,
                        "clarify",
                        k,
                        o.action.action_id(),
                        &json!({"status": o.action.status}),
                    )
                    .await?
                }
                Err(_) => idempotency::release(&mut c, actor.user_id, "clarify", k).await?,
            }
        }
        result
    }

    // ----------------------------------------------------- confirm / cancel --

    pub async fn confirm(
        &self,
        actor: &ActorContext,
        action_id: ActionId,
        confirmation_id: ConfirmationId,
        key: Option<IdempotencyKey>,
    ) -> Result<ConfirmOutcome, AgentError> {
        let span = tracing::info_span!("confirm", action_id = %action_id);
        async {
            Ok(executor::confirm_and_execute(
                self.deps.store.pool(),
                ConfirmParams {
                    actor,
                    action_id,
                    confirmation_id,
                    idempotency_key: key.as_ref(),
                    limits: self.cfg.limits,
                },
            )
            .await?)
        }
        .instrument(span)
        .await
    }

    pub async fn cancel(
        &self,
        actor: &ActorContext,
        action_id: ActionId,
    ) -> Result<ActionRow, AgentError> {
        Ok(executor::cancel(
            self.deps.store.pool(),
            actor,
            action_id,
            "cancelled by user",
        )
        .await?)
    }
}

fn jev_summary(a: &RiskAssessment) -> String {
    let mut parts = vec![];
    if let Some(i) = &a.intent_alignment {
        parts.push(format!("intent alignment {:.0}% yes", i.yes * 100.0));
    }
    if let Some(c) = &a.context_sufficient {
        parts.push(format!("context sufficient {:.0}% yes", c.yes * 100.0));
    }
    if let Some(b) = &a.behavioral_anomaly {
        parts.push(format!(
            "behavioural anomaly score {:.1}/{}",
            b.score,
            b.max_level()
        ));
    }
    if let Some(r) = &a.recipient_risk {
        parts.push(format!(
            "recipient risk score {:.1}/{}",
            r.score,
            r.max_level()
        ));
    }
    parts.push(format!(
        "overall confidence {:.0}%",
        a.overall_confidence * 100.0
    ));
    parts.join("; ")
}
