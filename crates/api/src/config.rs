use finguard_domain::Secret;
use finguard_persistence::DbConfig;
use finguard_policy::ConfidenceThresholds;
use std::{str::FromStr, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserKind {
    Deterministic,
    Groq,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionKind {
    Mock,
    Jev,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationKind {
    Mock,
    Groq,
}

#[derive(Clone)]
pub struct JevSettings {
    pub api_key: Secret<String>,
    pub endpoint: String,
    pub model: String,
    pub timeout: Duration,
}

#[derive(Clone)]
pub struct GroqSettings {
    pub api_key: Secret<String>,
    pub base_url: String,
    pub model: String,
    pub timeout: Duration,
    pub strict_schema: bool,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone)]
pub struct Config {
    pub app_env: String,
    pub bind_addr: String,
    pub cors_origin: String,
    pub db: DbConfig,
    pub jev: Option<JevSettings>,
    pub groq: Option<GroqSettings>,
    pub parser: ParserKind,
    pub decision: DecisionKind,
    pub escalation: EscalationKind,
    pub thresholds: ConfidenceThresholds,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("app_env", &self.app_env)
            .field("bind_addr", &self.bind_addr)
            .field(
                "database",
                &finguard_domain::redact::redact_database_url(&self.db.url),
            )
            .field("parser", &self.parser)
            .field("decision", &self.decision)
            .field("escalation", &self.escalation)
            .field("jev_configured", &self.jev.is_some())
            .field("groq_configured", &self.groq.is_some())
            .finish()
    }
}

fn var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}
fn parse<T: FromStr>(name: &str) -> Result<Option<T>, String> {
    match var(name) {
        None => Ok(None),
        Some(v) => v
            .parse()
            .map(Some)
            .map_err(|_| format!("{name} has an invalid value")),
    }
}

impl Config {
    pub fn is_production(&self) -> bool {
        self.app_env.eq_ignore_ascii_case("production")
    }

    /// All secret values, so the log writer can scrub them even if some code
    /// path were to print one.
    pub fn secret_values(&self) -> Vec<String> {
        let mut v = vec![];
        if let Some(j) = &self.jev {
            v.push(j.api_key.expose().clone());
        }
        if let Some(g) = &self.groq {
            v.push(g.api_key.expose().clone());
        }
        v
    }

    pub fn from_env() -> Result<Self, String> {
        let db_url = var("DATABASE_URL").ok_or("DATABASE_URL is required")?;
        let default_threshold: f64 = parse("JEV_DEFAULT_CONFIDENCE_THRESHOLD")?.unwrap_or(0.65);
        let d = ConfidenceThresholds::default();
        let thresholds = ConfidenceThresholds {
            default: default_threshold,
            read_only: parse("JEV_THRESHOLD_READ_ONLY")?.or(d.read_only),
            reversible: parse("JEV_THRESHOLD_REVERSIBLE")?.or(d.reversible),
            money_movement: parse("JEV_THRESHOLD_MONEY_MOVEMENT")?.or(d.money_movement),
            high_impact: parse("JEV_THRESHOLD_HIGH_IMPACT")?.or(d.high_impact),
        };
        for t in [
            Some(thresholds.default),
            thresholds.read_only,
            thresholds.reversible,
            thresholds.money_movement,
            thresholds.high_impact,
        ]
        .into_iter()
        .flatten()
        {
            if !(0.0..=1.0).contains(&t) {
                return Err("confidence thresholds must be within 0..1".into());
            }
        }

        let jev = var("JEVSTATION_API_KEY").map(|k| JevSettings {
            api_key: Secret::new(k),
            endpoint: var("JEVSTATION_BASE_URL")
                .unwrap_or_else(|| "https://jevstation.com/api/v1/systemone".into()),
            model: var("JEV_MODEL").unwrap_or_else(|| "jev-latest".into()),
            timeout: Duration::from_millis(
                parse("JEV_TIMEOUT_MS").ok().flatten().unwrap_or(10_000),
            ),
        });
        let groq = var("GROQ_API_KEY").map(|k| GroqSettings {
            api_key: Secret::new(k),
            base_url: var("GROQ_BASE_URL")
                .unwrap_or_else(|| "https://api.groq.com/openai/v1".into()),
            model: var("GROQ_MODEL").unwrap_or_else(|| "openai/gpt-oss-20b".into()),
            timeout: Duration::from_millis(
                parse("GROQ_TIMEOUT_MS").ok().flatten().unwrap_or(15_000),
            ),
            strict_schema: var("GROQ_STRICT_SCHEMA")
                .map(|v| !matches!(v.as_str(), "false" | "0"))
                .unwrap_or(true),
            reasoning_effort: var("GROQ_REASONING_EFFORT").or_else(|| Some("low".into())),
        });

        let parser = match var("INTENT_PARSER").as_deref() {
            None | Some("deterministic") => ParserKind::Deterministic,
            Some("groq") => ParserKind::Groq,
            Some(_) => return Err("INTENT_PARSER must be deterministic or groq".into()),
        };
        let decision = match var("DECISION_PROVIDER").as_deref() {
            None | Some("mock") => DecisionKind::Mock,
            Some("jev") | Some("real") => DecisionKind::Jev,
            Some(_) => return Err("DECISION_PROVIDER must be mock or jev".into()),
        };
        let escalation = match var("ESCALATION_ANALYZER").as_deref() {
            None | Some("mock") => EscalationKind::Mock,
            Some("groq") | Some("real") => EscalationKind::Groq,
            Some(_) => return Err("ESCALATION_ANALYZER must be mock or groq".into()),
        };
        // Fail fast with a clear message rather than silently running on mocks.
        if decision == DecisionKind::Jev && jev.is_none() {
            return Err("DECISION_PROVIDER=jev requires JEVSTATION_API_KEY".into());
        }
        if (parser == ParserKind::Groq || escalation == EscalationKind::Groq) && groq.is_none() {
            return Err("INTENT_PARSER/ESCALATION_ANALYZER=groq requires GROQ_API_KEY".into());
        }

        Ok(Config {
            app_env: var("APP_ENV").unwrap_or_else(|| "development".into()),
            bind_addr: var("BIND_ADDR").unwrap_or_else(|| "127.0.0.1:8080".into()),
            cors_origin: var("CORS_ORIGIN").unwrap_or_else(|| "http://localhost:5173".into()),
            db: DbConfig {
                url: db_url,
                migration_url: var("DATABASE_MIGRATION_URL"),
                max_connections: parse("DATABASE_MAX_CONNECTIONS")?.unwrap_or(10),
                statement_cache: parse("DATABASE_STATEMENT_CACHE")?,
            },
            jev,
            groq,
            parser,
            decision,
            escalation,
            thresholds,
        })
    }
}
