use crate::config::*;
use finguard_agent::{Agent, AgentConfig, AgentDeps, DeterministicIntentParser};
use finguard_domain::*;
use finguard_groq::{
    GroqConfig, GroqEscalationAnalyzer, GroqHttpBackend, GroqIntentParser, MockEscalationAnalyzer,
};
use finguard_jev::{JevConfig, JevDecisionProvider, MockDecisionProvider};
use finguard_persistence::Store;
use std::{sync::Arc, time::Duration};

#[derive(Clone)]
pub struct AppState {
    pub agent: Arc<Agent>,
    pub store: Store,
    pub app_env: String,
    /// Present only when the corresponding provider is a mock; lets the demo
    /// panel inject timeouts / 429s / 5xx to show fail-safe behaviour.
    pub mock_jev: Option<MockDecisionProvider>,
    pub mock_groq: Option<MockEscalationAnalyzer>,
    pub provider_summary: Arc<serde_json::Value>,
}

impl AppState {
    pub fn is_production(&self) -> bool {
        self.app_env.eq_ignore_ascii_case("production")
    }
}

/// Wire providers from configuration. Real providers need keys (checked in
/// `Config::from_env`); mocks need nothing.
pub fn build_state(store: Store, cfg: &Config) -> Result<AppState, String> {
    let mk_groq = |g: &GroqSettings| -> Result<Arc<GroqHttpBackend>, String> {
        let mut c = GroqConfig::new(
            g.api_key.expose().clone(),
            g.base_url.clone(),
            g.model.clone(),
        );
        c.timeout = g.timeout;
        c.strict_schema = g.strict_schema;
        c.reasoning_effort = g.reasoning_effort.clone();
        GroqHttpBackend::new(c)
            .map(Arc::new)
            .map_err(|e| e.to_string())
    };

    let parser: Arc<dyn IntentParser> = match (cfg.parser, &cfg.groq) {
        (ParserKind::Groq, Some(g)) => Arc::new(GroqIntentParser::new(mk_groq(g)?)),
        _ => Arc::new(DeterministicIntentParser),
    };

    let (decision, mock_jev): (Arc<dyn DecisionProvider>, Option<MockDecisionProvider>) =
        match (cfg.decision, &cfg.jev) {
            (DecisionKind::Jev, Some(j)) => {
                let mut c = JevConfig::new(j.api_key.expose().clone(), j.endpoint.clone());
                c.model = j.model.clone();
                c.timeout = j.timeout;
                c.total_deadline = j.timeout * 2 + Duration::from_secs(2);
                (
                    Arc::new(JevDecisionProvider::new(c).map_err(|e| e.to_string())?),
                    None,
                )
            }
            _ => {
                let m = MockDecisionProvider::new().with_latency(Duration::from_millis(120));
                (Arc::new(m.clone()), Some(m))
            }
        };

    let (escalation, mock_groq): (Arc<dyn EscalationAnalyzer>, Option<MockEscalationAnalyzer>) =
        match (cfg.escalation, &cfg.groq) {
            (EscalationKind::Groq, Some(g)) => {
                (Arc::new(GroqEscalationAnalyzer::new(mk_groq(g)?)), None)
            }
            _ => {
                let m = MockEscalationAnalyzer::new();
                (Arc::new(m.clone()), Some(m))
            }
        };

    let summary = serde_json::json!({
        "intent_parser": parser.name(),
        "decision_provider": decision.name(),
        "escalation_analyzer": escalation.name(),
        "groq_model": cfg.groq.as_ref().map(|g| g.model.clone()),
        "policy_version": POLICY_VERSION,
        "risk_question_set": RISK_QUESTION_SET_VERSION,
        "thresholds": {
            "read_only": cfg.thresholds.for_class(RiskClass::ReadOnly),
            "reversible": cfg.thresholds.for_class(RiskClass::Reversible),
            "money_movement": cfg.thresholds.for_class(RiskClass::MoneyMovement),
            "high_impact": cfg.thresholds.for_class(RiskClass::HighImpact),
            "note": "demo parameters, not validated financial thresholds",
        },
    });

    let agent = Agent::new(
        AgentDeps {
            store: store.clone(),
            parser,
            decision,
            escalation,
        },
        AgentConfig {
            thresholds: cfg.thresholds,
            ..AgentConfig::default()
        },
    );
    Ok(AppState {
        agent: Arc::new(agent),
        store,
        app_env: cfg.app_env.clone(),
        mock_jev,
        mock_groq,
        provider_summary: Arc::new(summary),
    })
}
