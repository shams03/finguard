//! Final policy: turns (hard result, risk evidence, optional escalation) into
//! one of five workflow decisions. Models supply *evidence*; this file decides.
//!
//! The logic is a pipeline of small functions rather than one giant `if`:
//!
//! ```text
//!  hard block?  ->  Jev unavailable?  ->  incomplete?  ->  severe signals?
//!      -> low confidence / ambiguity? -> ESCALATE (stage 1 ends here)
//!      -> base decision for the tool (confirmation / execute)
//! ```
//!
//! [`stage_one`] runs everything that needs no second opinion and either
//! returns a final outcome or asks for escalation. [`finalize_after_escalation`]
//! folds the escalation analysis back in. An escalation can only make the
//! outcome *more* cautious or ask the user a question; it can never produce
//! `Execute`, and money movement keeps its confirmation requirement.

use crate::thresholds::ConfidenceThresholds;
use finguard_domain::{
    BlockReason, DecisionProviderError, EscalationAnalysis, EscalationReason, HardPolicyReport,
    HardPolicyResult, MissingField, PolicyDecision, RecommendedHandling, RiskAssessment, RiskClass,
    ToolSpec,
};
use serde::{Deserialize, Serialize};

/// A Jev anomaly / recipient score at or above this level (0..=4 rubric) is
/// treated as "highly unusual" and held for manual review.
pub const SEVERE_SCORE: f64 = 3.0;
/// Both scores at or above this level ("extreme") are blocked outright.
pub const EXTREME_SCORE: f64 = 3.5;
/// Transfer this many times the user's usual maximum counts as far above normal.
pub const FAR_ABOVE_TYPICAL_RATIO: f64 = 5.0;
/// Recipients younger than this are "new".
pub const NEW_RECIPIENT_DAYS: i64 = 30;
/// Below this P(yes) a Noul answer is treated as "no".
pub const NOUL_YES_FLOOR: f64 = 0.5;

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum JevStatus {
    /// Policy does not use Jev for this tool (e.g. reading a balance).
    NotRequired,
    /// Jev was required but the call failed. Never silently bypassed.
    Unavailable(DecisionProviderError),
    Assessed(RiskAssessment),
}

/// Facts we computed ourselves (no model involved).
#[derive(Debug, Clone, Default)]
pub struct DeterministicSignals {
    pub recipient_known: bool,
    pub recipient_age_days: Option<i64>,
    pub amount_to_typical_ratio: Option<f64>,
    pub device_trusted: bool,
    pub location_familiar: bool,
    pub transfers_last_hour: u32,
}

pub struct PolicyInput<'a> {
    pub tool: &'a ToolSpec,
    pub hard: &'a HardPolicyReport,
    pub missing_fields: &'a [MissingField],
    pub jev: &'a JevStatus,
    pub signals: &'a DeterministicSignals,
    pub thresholds: &'a ConfidenceThresholds,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyReason {
    pub code: String,
    pub detail: String,
}

fn reason(code: &str, detail: impl Into<String>) -> PolicyReason {
    PolicyReason {
        code: code.into(),
        detail: detail.into(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyOutcome {
    pub decision: PolicyDecision,
    pub reasons: Vec<PolicyReason>,
    /// Set when escalation was part of how we got here.
    pub escalation: Option<EscalationReason>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    Final(PolicyOutcome),
    Escalate {
        reason: EscalationReason,
        reasons: Vec<PolicyReason>,
    },
}

fn done(decision: PolicyDecision, reasons: Vec<PolicyReason>) -> Stage {
    Stage::Final(PolicyOutcome {
        decision,
        reasons,
        escalation: None,
    })
}

// ---------------------------------------------------------------- rules ----

/// Rule: hard policy is authoritative.
fn rule_hard(input: &PolicyInput) -> Option<Stage> {
    match input.hard.result {
        HardPolicyResult::Pass => None,
        HardPolicyResult::Block(r) => Some(done(
            PolicyDecision::Block {
                reason: BlockReason::HardPolicy(r),
            },
            vec![reason("HARD_POLICY_BLOCK", r.code())],
        )),
    }
}

/// Rule: money movement must not silently bypass risk policy when Jev is down.
fn rule_provider_failure(input: &PolicyInput) -> Option<Stage> {
    if let (true, JevStatus::Unavailable(e)) = (input.tool.requires_jev, input.jev) {
        return Some(done(
            PolicyDecision::ManualReview,
            vec![reason(
                "JEV_UNAVAILABLE_FAIL_CLOSED",
                format!(
                    "risk assessment required but provider failed ({}); holding for review",
                    e.code()
                ),
            )],
        ));
    }
    None
}

/// Rule: an incomplete action can be clarified, never executed.
fn rule_incomplete(input: &PolicyInput) -> Option<Stage> {
    if input.missing_fields.is_empty() {
        return None;
    }
    let names: Vec<_> = input.missing_fields.iter().map(|m| m.as_str()).collect();
    Some(Stage::Escalate {
        reason: EscalationReason::MissingInformation,
        reasons: vec![reason(
            "MISSING_INFORMATION",
            format!("missing: {}", names.join(", ")),
        )],
    })
}

/// Deterministic risk signals that do not depend on any model.
pub fn deterministic_severe_signals(s: &DeterministicSignals) -> Vec<PolicyReason> {
    let new_recipient =
        !s.recipient_known || s.recipient_age_days.is_some_and(|d| d < NEW_RECIPIENT_DAYS);
    let far_above = s
        .amount_to_typical_ratio
        .is_some_and(|r| r >= FAR_ABOVE_TYPICAL_RATIO);
    let unfamiliar = !s.device_trusted || !s.location_familiar;
    let mut out = vec![];
    if new_recipient && far_above && unfamiliar {
        out.push(reason(
            "NEW_RECIPIENT_FAR_ABOVE_TYPICAL_UNFAMILIAR_CONTEXT",
            "new recipient, amount far above the usual range, and unfamiliar device/location",
        ));
    }
    out
}

/// Jev-derived severity: a high anomaly or recipient-risk score.
pub fn jev_severe_signals(a: &RiskAssessment) -> Vec<PolicyReason> {
    let mut out = vec![];
    if let Some(b) = &a.behavioral_anomaly
        && b.score >= SEVERE_SCORE
    {
        out.push(reason(
            "BEHAVIORAL_ANOMALY_HIGH",
            format!(
                "behavioural anomaly score {:.1}/{} at or above {SEVERE_SCORE}",
                b.score,
                b.max_level()
            ),
        ));
    }
    if let Some(r) = &a.recipient_risk
        && r.score >= SEVERE_SCORE
    {
        out.push(reason(
            "RECIPIENT_RISK_HIGH",
            format!(
                "recipient risk score {:.1}/{} at or above {SEVERE_SCORE}",
                r.score,
                r.max_level()
            ),
        ));
    }
    out
}

fn context_reasons(s: &DeterministicSignals) -> Vec<PolicyReason> {
    let mut out = vec![];
    if !s.recipient_known || s.recipient_age_days.is_some_and(|d| d < NEW_RECIPIENT_DAYS) {
        out.push(reason("RECIPIENT_IS_NEW", "recipient is new"));
    }
    if s.amount_to_typical_ratio
        .is_some_and(|r| r >= FAR_ABOVE_TYPICAL_RATIO)
    {
        out.push(reason(
            "AMOUNT_FAR_ABOVE_TYPICAL",
            "amount significantly exceeds the typical transfer",
        ));
    }
    if !s.device_trusted {
        out.push(reason(
            "DEVICE_NOT_TRUSTED",
            "request came from an unrecognised device",
        ));
    }
    if !s.location_familiar {
        out.push(reason(
            "LOCATION_UNFAMILIAR",
            "request came from an unfamiliar location",
        ));
    }
    out
}

/// Rule: severe signals. Extreme on both Jev scores blocks; anything else
/// severe is held for manual review. Jev can only push toward caution.
fn rule_severe(input: &PolicyInput) -> Option<Stage> {
    let mut severe = deterministic_severe_signals(input.signals);
    let mut extreme = false;
    if let JevStatus::Assessed(a) = input.jev {
        severe.extend(jev_severe_signals(a));
        extreme = matches!((&a.behavioral_anomaly, &a.recipient_risk),
            (Some(b), Some(r)) if b.score >= EXTREME_SCORE && r.score >= EXTREME_SCORE);
    }
    if severe.is_empty() {
        return None;
    }
    let mut reasons = severe;
    reasons.extend(context_reasons(input.signals));
    Some(if extreme {
        done(
            PolicyDecision::Block {
                reason: BlockReason::SevereRisk,
            },
            reasons,
        )
    } else {
        done(PolicyDecision::ManualReview, reasons)
    })
}

/// Rule: low confidence or semantic doubt triggers escalation (not trust).
fn rule_confidence(input: &PolicyInput) -> Option<Stage> {
    let JevStatus::Assessed(a) = input.jev else {
        return None;
    };
    let threshold = input.thresholds.for_class(input.tool.risk_class);
    if a.overall_confidence < threshold {
        return Some(Stage::Escalate {
            reason: EscalationReason::LowDecisionConfidence,
            reasons: vec![reason(
                "LOW_DECISION_CONFIDENCE",
                format!(
                    "confidence {:.0}% below required {:.0}%",
                    a.overall_confidence * 100.0,
                    threshold * 100.0
                ),
            )],
        });
    }
    if a.context_sufficient.is_some_and(|c| c.yes < NOUL_YES_FLOOR) {
        return Some(Stage::Escalate {
            reason: EscalationReason::MissingInformation,
            reasons: vec![reason(
                "CONTEXT_INSUFFICIENT",
                "assessment says available context is insufficient",
            )],
        });
    }
    if a.intent_alignment.is_some_and(|c| c.yes < NOUL_YES_FLOOR) {
        return Some(Stage::Escalate {
            reason: EscalationReason::ConflictingSignals,
            reasons: vec![reason(
                "INTENT_MISALIGNED",
                "proposed action may not match what the user asked for",
            )],
        });
    }
    None
}

/// The decision when nothing above objected.
pub fn base_decision(tool: &ToolSpec) -> (PolicyDecision, PolicyReason) {
    if tool.requires_confirmation {
        let why = match tool.risk_class {
            RiskClass::MoneyMovement | RiskClass::HighImpact => {
                "Money movement requires explicit user approval under policy_v1."
            }
            _ => "This sensitive action requires explicit user approval under policy_v1.",
        };
        (
            PolicyDecision::RequireConfirmation,
            reason("CONFIRMATION_REQUIRED", why),
        )
    } else {
        (
            PolicyDecision::Execute,
            reason(
                "READ_ONLY_EXECUTE",
                "Read-only action; deterministic execution.",
            ),
        )
    }
}

// ------------------------------------------------------------ pipelines ----

pub fn stage_one(input: &PolicyInput) -> Stage {
    let rules: [fn(&PolicyInput) -> Option<Stage>; 5] = [
        rule_hard,
        rule_provider_failure,
        rule_incomplete,
        rule_severe,
        rule_confidence,
    ];
    for rule in rules {
        if let Some(stage) = rule(input) {
            return stage;
        }
    }
    let (decision, why) = base_decision(input.tool);
    let mut reasons = vec![why];
    if let JevStatus::Assessed(a) = input.jev {
        reasons.push(reason(
            "CONFIDENCE_SUFFICIENT",
            format!(
                "confidence {:.0}% meets required {:.0}%",
                a.overall_confidence * 100.0,
                input.thresholds.for_class(input.tool.risk_class) * 100.0
            ),
        ));
    }
    done(decision, reasons)
}

/// Fold an escalation result (or its failure) into the final outcome.
///
/// `analysis = None` means the escalation provider failed: we fail safe.
/// `fallback_question` is a deterministic clarification used when the model
/// cannot (or may not) supply one.
pub fn finalize_after_escalation(
    input: &PolicyInput,
    reason_for_escalation: EscalationReason,
    stage_reasons: Vec<PolicyReason>,
    analysis: Option<&EscalationAnalysis>,
    fallback_question: String,
) -> PolicyOutcome {
    let mut reasons = stage_reasons;
    let esc = Some(reason_for_escalation);
    let incomplete = !input.missing_fields.is_empty();

    let decision = match analysis {
        None => {
            reasons.push(reason(
                "ESCALATION_UNAVAILABLE_FAIL_SAFE",
                "second-opinion provider unavailable; failing safe",
            ));
            if incomplete {
                PolicyDecision::AskForClarification {
                    question: fallback_question,
                }
            } else {
                PolicyDecision::ManualReview
            }
        }
        Some(a) => {
            reasons.push(reason(
                "ESCALATION_ANALYSIS",
                format!(
                    "second opinion recommended {:?}; it is advisory only",
                    a.recommended_handling
                ),
            ));
            // Deterministic override: an incomplete action is always clarified,
            // whatever the model recommends.
            if incomplete
                || a.clarification_needed
                || a.recommended_handling == RecommendedHandling::AskUser
            {
                let question = a
                    .clarification_question
                    .clone()
                    .filter(|q| !q.trim().is_empty())
                    .unwrap_or(fallback_question);
                PolicyDecision::AskForClarification { question }
            } else if a.recommended_handling == RecommendedHandling::Review {
                PolicyDecision::ManualReview
            } else {
                // ContinuePolicy: back to the base decision. This can never
                // be *less* strict than confirmation for money movement.
                let (d, why) = base_decision(input.tool);
                reasons.push(why);
                d
            }
        }
    };
    PolicyOutcome {
        decision,
        reasons,
        escalation: esc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hard::{self, HardLimits, test_support};
    use finguard_domain::*;

    fn pass_report() -> HardPolicyReport {
        HardPolicyReport {
            checks: vec![],
            result: HardPolicyResult::Pass,
        }
    }
    fn good_signals() -> DeterministicSignals {
        DeterministicSignals {
            recipient_known: true,
            recipient_age_days: Some(240),
            amount_to_typical_ratio: Some(0.25),
            device_trusted: true,
            location_familiar: true,
            transfers_last_hour: 0,
        }
    }
    fn scored(score: f64, conf: f64) -> ScoredAssessment {
        ScoredAssessment {
            score,
            labels: (0..5).map(|i| format!("L{i}")).collect(),
            probabilities: vec![0.2; 5],
            confidence: conf,
        }
    }
    fn assessment(anomaly: f64, recipient: f64, conf: f64) -> RiskAssessment {
        let mut a = RiskAssessment {
            intent_alignment: Some(ProbabilityAssessment::from_yes_probability(0.98)),
            behavioral_anomaly: Some(scored(anomaly, conf)),
            recipient_risk: Some(scored(recipient, conf)),
            context_sufficient: Some(ProbabilityAssessment::from_yes_probability(0.97)),
            overall_confidence: 0.0,
            provider_metadata: ProviderMetadata {
                provider: "mock".into(),
                model: None,
                latency_ms: 1,
                question_set_version: RISK_QUESTION_SET_VERSION.into(),
                input_tokens: None,
                output_tokens: None,
                credits_charged: None,
                credits_remaining: None,
            },
            raw: serde_json::Value::Null,
        };
        a.overall_confidence = a.weakest_link_confidence();
        a
    }

    fn run(
        tool: ActionKind,
        hard: &HardPolicyReport,
        missing: &[MissingField],
        jev: &JevStatus,
        s: &DeterministicSignals,
    ) -> Stage {
        stage_one(&PolicyInput {
            tool: tool_spec(tool),
            hard,
            missing_fields: missing,
            jev,
            signals: s,
            thresholds: &ConfidenceThresholds::default(),
        })
    }
    fn final_decision(stage: Stage) -> PolicyDecision {
        match stage {
            Stage::Final(o) => o.decision,
            Stage::Escalate { reason, .. } => panic!("unexpected escalation {reason:?}"),
        }
    }

    #[test]
    fn read_only_executes_without_jev() {
        let s = run(
            ActionKind::ReadBalance,
            &pass_report(),
            &[],
            &JevStatus::NotRequired,
            &good_signals(),
        );
        assert_eq!(final_decision(s), PolicyDecision::Execute);
    }

    #[test]
    fn freeze_requires_confirmation() {
        let s = run(
            ActionKind::FreezeCard,
            &pass_report(),
            &[],
            &JevStatus::NotRequired,
            &good_signals(),
        );
        assert_eq!(final_decision(s), PolicyDecision::RequireConfirmation);
    }

    #[test]
    fn money_movement_always_requires_confirmation_even_at_99_percent() {
        let mut a = assessment(0.1, 0.1, 0.99);
        a.overall_confidence = 0.99;
        let s = run(
            ActionKind::TransferFunds,
            &pass_report(),
            &[],
            &JevStatus::Assessed(a),
            &good_signals(),
        );
        assert_eq!(final_decision(s), PolicyDecision::RequireConfirmation);
    }

    #[test]
    fn hard_block_wins_and_never_needs_ai() {
        let draft = test_support::transfer(Some(Money::php_major(40_000)));
        let report = hard::evaluate(
            Some(&test_support::actor()),
            &draft,
            &test_support::facts(20_000),
            &HardLimits::default(),
        );
        // JevStatus::NotRequired stands in for "never called"; even a glowing
        // assessment could not change the outcome:
        let glowing = JevStatus::Assessed(assessment(0.0, 0.0, 0.99));
        for jev in [JevStatus::NotRequired, glowing] {
            let s = run(
                ActionKind::TransferFunds,
                &report,
                &[],
                &jev,
                &good_signals(),
            );
            assert_eq!(
                final_decision(s),
                PolicyDecision::Block {
                    reason: BlockReason::HardPolicy(HardPolicyReason::InsufficientFunds)
                }
            );
        }
    }

    #[test]
    fn confidence_below_threshold_escalates() {
        let a = assessment(0.2, 0.2, 0.63);
        let s = run(
            ActionKind::TransferFunds,
            &pass_report(),
            &[],
            &JevStatus::Assessed(a),
            &good_signals(),
        );
        assert!(matches!(
            s,
            Stage::Escalate {
                reason: EscalationReason::LowDecisionConfidence,
                ..
            }
        ));
    }

    #[test]
    fn confidence_above_threshold_does_not_escalate() {
        let a = assessment(0.2, 0.2, 0.95);
        let s = run(
            ActionKind::TransferFunds,
            &pass_report(),
            &[],
            &JevStatus::Assessed(a),
            &good_signals(),
        );
        assert!(matches!(s, Stage::Final(_)));
    }

    #[test]
    fn threshold_is_per_risk_class() {
        // 0.85 passes the 0.80 reversible threshold but fails the 0.90 money one.
        let mut a = assessment(0.2, 0.2, 0.85);
        a.overall_confidence = 0.85;
        let th = ConfidenceThresholds::default();
        for (tool, expect_escalation) in [
            (ActionKind::FreezeCard, false),
            (ActionKind::TransferFunds, true),
        ] {
            let jev = JevStatus::Assessed(a.clone());
            let stage = stage_one(&PolicyInput {
                tool: tool_spec(tool),
                hard: &pass_report(),
                missing_fields: &[],
                jev: &jev,
                signals: &good_signals(),
                thresholds: &th,
            });
            assert_eq!(
                matches!(stage, Stage::Escalate { .. }),
                expect_escalation,
                "{tool}"
            );
        }
    }

    #[test]
    fn jev_unavailable_on_money_movement_fails_closed() {
        for err in [
            DecisionProviderError::Timeout,
            DecisionProviderError::RateLimited {
                retry_after_ms: None,
            },
            DecisionProviderError::ProviderUnavailable("503".into()),
        ] {
            let s = run(
                ActionKind::TransferFunds,
                &pass_report(),
                &[],
                &JevStatus::Unavailable(err),
                &good_signals(),
            );
            assert_eq!(final_decision(s), PolicyDecision::ManualReview);
        }
    }

    #[test]
    fn jev_unavailable_does_not_stop_read_only() {
        let s = run(
            ActionKind::ReadBalance,
            &pass_report(),
            &[],
            &JevStatus::Unavailable(DecisionProviderError::Timeout),
            &good_signals(),
        );
        assert_eq!(final_decision(s), PolicyDecision::Execute);
    }

    #[test]
    fn high_anomaly_goes_to_manual_review_with_reasons() {
        let a = assessment(3.2, 2.8, 0.93);
        let sig = DeterministicSignals {
            recipient_known: false,
            recipient_age_days: Some(2),
            amount_to_typical_ratio: Some(9.4),
            device_trusted: false,
            location_familiar: false,
            transfers_last_hour: 3,
        };
        match run(
            ActionKind::TransferFunds,
            &pass_report(),
            &[],
            &JevStatus::Assessed(a),
            &sig,
        ) {
            Stage::Final(o) => {
                assert_eq!(o.decision, PolicyDecision::ManualReview);
                let codes: Vec<_> = o.reasons.iter().map(|r| r.code.as_str()).collect();
                assert!(codes.contains(&"BEHAVIORAL_ANOMALY_HIGH"));
                assert!(codes.contains(&"RECIPIENT_IS_NEW"));
                assert!(codes.contains(&"AMOUNT_FAR_ABOVE_TYPICAL"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn deterministic_signals_hold_a_transfer_even_if_jev_says_fine() {
        let a = assessment(0.1, 0.1, 0.99);
        let sig = DeterministicSignals {
            recipient_known: false,
            recipient_age_days: Some(1),
            amount_to_typical_ratio: Some(9.4),
            device_trusted: false,
            location_familiar: true,
            transfers_last_hour: 0,
        };
        let s = run(
            ActionKind::TransferFunds,
            &pass_report(),
            &[],
            &JevStatus::Assessed(a),
            &sig,
        );
        assert_eq!(final_decision(s), PolicyDecision::ManualReview);
    }

    #[test]
    fn extreme_on_both_scores_blocks() {
        let a = assessment(3.9, 3.8, 0.97);
        let s = run(
            ActionKind::TransferFunds,
            &pass_report(),
            &[],
            &JevStatus::Assessed(a),
            &good_signals(),
        );
        assert_eq!(
            final_decision(s),
            PolicyDecision::Block {
                reason: BlockReason::SevereRisk
            }
        );
    }

    #[test]
    fn known_recipient_vs_new_recipient_context() {
        // known + normal -> confirmation
        let a = assessment(0.2, 0.2, 0.95);
        let s = run(
            ActionKind::TransferFunds,
            &pass_report(),
            &[],
            &JevStatus::Assessed(a.clone()),
            &good_signals(),
        );
        assert_eq!(final_decision(s), PolicyDecision::RequireConfirmation);
        // new recipient but otherwise ordinary amount and trusted context -> still confirmation, not review
        let mut sig = good_signals();
        sig.recipient_known = false;
        sig.recipient_age_days = Some(1);
        sig.amount_to_typical_ratio = Some(0.3);
        let s = run(
            ActionKind::TransferFunds,
            &pass_report(),
            &[],
            &JevStatus::Assessed(a),
            &sig,
        );
        assert_eq!(final_decision(s), PolicyDecision::RequireConfirmation);
    }

    #[test]
    fn missing_amount_escalates_for_clarification_even_with_confident_jev() {
        let a = assessment(0.1, 0.1, 0.99);
        let s = run(
            ActionKind::TransferFunds,
            &pass_report(),
            &[MissingField::Amount],
            &JevStatus::Assessed(a),
            &good_signals(),
        );
        assert!(matches!(
            s,
            Stage::Escalate {
                reason: EscalationReason::MissingInformation,
                ..
            }
        ));
    }

    fn analysis(
        handling: RecommendedHandling,
        clarify: bool,
        q: Option<&str>,
    ) -> EscalationAnalysis {
        EscalationAnalysis {
            intent_supported: true,
            ambiguity_detected: clarify,
            missing_information: vec![],
            clarification_needed: clarify,
            clarification_question: q.map(Into::into),
            recommended_handling: handling,
            meta: None,
        }
    }
    fn fin(missing: &[MissingField], an: Option<&EscalationAnalysis>) -> PolicyOutcome {
        let jev = JevStatus::NotRequired;
        let input = PolicyInput {
            tool: tool_spec(ActionKind::TransferFunds),
            hard: &pass_report(),
            missing_fields: missing,
            jev: &jev,
            signals: &good_signals(),
            thresholds: &ConfidenceThresholds::default(),
        };
        finalize_after_escalation(
            &input,
            EscalationReason::LowDecisionConfidence,
            vec![],
            an,
            "How much?".into(),
        )
    }

    #[test]
    fn escalation_can_clarify_or_review_but_never_execute() {
        let o = fin(
            &[MissingField::Amount],
            Some(&analysis(
                RecommendedHandling::AskUser,
                true,
                Some("Send ₱5,000?"),
            )),
        );
        assert_eq!(
            o.decision,
            PolicyDecision::AskForClarification {
                question: "Send ₱5,000?".into()
            }
        );
        let o = fin(
            &[],
            Some(&analysis(RecommendedHandling::Review, false, None)),
        );
        assert_eq!(o.decision, PolicyDecision::ManualReview);
        // ContinuePolicy on a transfer is still just "require confirmation"
        let o = fin(
            &[],
            Some(&analysis(RecommendedHandling::ContinuePolicy, false, None)),
        );
        assert_eq!(o.decision, PolicyDecision::RequireConfirmation);
    }

    #[test]
    fn model_cannot_talk_policy_out_of_clarifying_an_incomplete_action() {
        let o = fin(
            &[MissingField::Amount],
            Some(&analysis(RecommendedHandling::ContinuePolicy, false, None)),
        );
        assert_eq!(
            o.decision,
            PolicyDecision::AskForClarification {
                question: "How much?".into()
            }
        );
    }

    #[test]
    fn escalation_provider_failure_fails_safe() {
        assert_eq!(
            fin(&[MissingField::Amount], None).decision,
            PolicyDecision::AskForClarification {
                question: "How much?".into()
            }
        );
        assert_eq!(fin(&[], None).decision, PolicyDecision::ManualReview);
    }
}
