#![allow(dead_code)]
use finguard_agent::*;
use finguard_domain::*;
use finguard_groq::MockEscalationAnalyzer;
use finguard_jev::MockDecisionProvider;
use finguard_persistence::{
    Store,
    executor::ConfirmOutcome,
    facts,
    seed::{self, DemoSetup},
};
use sqlx::PgPool;
use std::sync::Arc;

pub struct H {
    pub agent: Arc<Agent>,
    pub actor: ActorContext,
    pub jev: MockDecisionProvider,
    pub esc: MockEscalationAnalyzer,
    pub pool: PgPool,
}

pub async fn setup(pool: PgPool, s: DemoSetup) -> H {
    setup_with(pool, s, Arc::new(DeterministicIntentParser)).await
}

pub async fn setup_with(pool: PgPool, s: DemoSetup, parser: Arc<dyn IntentParser>) -> H {
    seed::reset_demo(&pool, &s).await.unwrap();
    let jev = MockDecisionProvider::new();
    let esc = MockEscalationAnalyzer::new();
    let agent = Agent::new(
        AgentDeps {
            store: Store::from_pool(pool.clone()),
            parser,
            decision: Arc::new(jev.clone()),
            escalation: Arc::new(esc.clone()),
        },
        AgentConfig::default(),
    );
    let mut c = pool.acquire().await.unwrap();
    let actor = facts::load_actor(&mut c, seed::SESSION).await.unwrap();
    H {
        agent: Arc::new(agent),
        actor,
        jev,
        esc,
        pool: pool.clone(),
    }
}

impl H {
    pub async fn cmd(&self, text: &str) -> CommandOutcome {
        self.agent
            .handle_command(&self.actor, text, None, TraceId::new())
            .await
            .unwrap()
    }
    pub async fn cmd_key(&self, text: &str, key: &str) -> Result<CommandOutcome, AgentError> {
        self.agent
            .handle_command(
                &self.actor,
                text,
                Some(IdempotencyKey::new(key).unwrap()),
                TraceId::new(),
            )
            .await
    }
    pub async fn confirm(
        &self,
        o: &CommandOutcome,
        key: Option<&str>,
    ) -> Result<ConfirmOutcome, AgentError> {
        let c = o.confirmation.as_ref().expect("a confirmation was issued");
        self.agent
            .confirm(
                &self.actor,
                o.action.action_id(),
                ConfirmationId(c.id),
                key.map(|k| IdempotencyKey::new(k).unwrap()),
            )
            .await
    }
    pub async fn balance(&self) -> i64 {
        sqlx::query_scalar("SELECT balance_minor FROM accounts WHERE id = $1")
            .bind(seed::ACCOUNT)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
    pub async fn count(&self, sql: &'static str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.pool).await.unwrap()
    }
    pub async fn transfers_committed(&self) -> i64 {
        self.count("SELECT COUNT(*) FROM ledger_transactions WHERE kind = 'TRANSFER'")
            .await
    }
    pub async fn card_state(&self) -> String {
        sqlx::query_scalar("SELECT state FROM cards WHERE id = $1")
            .bind(seed::CARD)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }
    pub async fn events_by_action(&self, action_id: uuid::Uuid) -> Vec<String> {
        sqlx::query_scalar("SELECT event_type FROM audit_events WHERE action_id = $1 ORDER BY seq")
            .bind(action_id)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }
    pub async fn events(&self, a: &CommandOutcome) -> Vec<String> {
        sqlx::query_scalar("SELECT event_type FROM audit_events WHERE trace_id = $1 OR action_id = $2 ORDER BY seq")
            .bind(a.action.trace_id)
            .bind(a.action.id)
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }
}

pub fn setup_balance(php: i64) -> DemoSetup {
    DemoSetup {
        balance_minor: Money::php_major(php).amount_minor,
        ..DemoSetup::default()
    }
}
