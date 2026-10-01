use crate::{Result, StoreError};
use finguard_domain::*;
use serde_json::Value;
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActionRow {
    pub id: Uuid,
    pub trace_id: Uuid,
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub parent_action_id: Option<Uuid>,
    pub kind: String,
    pub risk_class: String,
    pub status: String,
    pub user_text: String,
    pub account_id: Option<Uuid>,
    pub card_id: Option<Uuid>,
    pub recipient_id: Option<Uuid>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub missing_fields: Vec<String>,
    pub fingerprint: Option<String>,
    pub policy_version: String,
    pub policy_decision: Option<String>,
    pub decision_reasons: Value,
    pub hard_policy: Option<Value>,
    pub clarification_question: Option<String>,
    pub clarification_candidate_minor: Option<i64>,
    pub result: Option<Value>,
    pub ledger_transaction_id: Option<Uuid>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
}

impl ActionRow {
    pub fn action_id(&self) -> ActionId {
        ActionId(self.id)
    }
    pub fn trace(&self) -> TraceId {
        TraceId(self.trace_id)
    }
    pub fn status(&self) -> Result<ActionStatus> {
        self.status
            .parse()
            .map_err(|_| StoreError::Corrupt(format!("bad status {}", self.status)))
    }
    pub fn kind(&self) -> Result<ActionKind> {
        self.kind
            .parse()
            .map_err(|_| StoreError::Corrupt(format!("bad kind {}", self.kind)))
    }
    pub fn amount(&self) -> Option<Money> {
        match (self.amount_minor, self.currency.as_deref()) {
            (Some(a), Some("PHP")) => Some(Money::php(a)),
            _ => None,
        }
    }
    pub fn draft(&self) -> Result<ActionDraft> {
        let need = |v: Option<Uuid>, what: &str| {
            v.ok_or_else(|| StoreError::Corrupt(format!("{} missing {what}", self.kind)))
        };
        Ok(match self.kind()? {
            ActionKind::ReadBalance => ActionDraft::ReadBalance {
                account_id: AccountId(need(self.account_id, "account")?),
            },
            ActionKind::FreezeCard => ActionDraft::FreezeCard {
                card_id: CardId(need(self.card_id, "card")?),
            },
            ActionKind::TransferFunds => ActionDraft::TransferFunds {
                from_account: AccountId(need(self.account_id, "account")?),
                recipient_id: self.recipient_id.map(RecipientId),
                amount: self.amount(),
            },
        })
    }
    /// The executable action, if (and only if) the stored draft is complete.
    pub fn proposed(&self) -> Result<Option<ProposedAction>> {
        Ok(self.draft()?.complete().ok())
    }
}

pub struct NewAction<'a> {
    pub trace_id: TraceId,
    pub actor: &'a ActorContext,
    pub parent: Option<ActionId>,
    pub draft: &'a ActionDraft,
    /// Already scrubbed.
    pub user_text: &'a str,
}

pub async fn create(conn: &mut PgConnection, n: &NewAction<'_>) -> Result<ActionRow> {
    let (account, card, recipient, amount) = match n.draft {
        ActionDraft::ReadBalance { account_id } => (Some(account_id.as_uuid()), None, None, None),
        ActionDraft::FreezeCard { card_id } => (None, Some(card_id.as_uuid()), None, None),
        ActionDraft::TransferFunds {
            from_account,
            recipient_id,
            amount,
        } => (
            Some(from_account.as_uuid()),
            None,
            recipient_id.map(|r| r.as_uuid()),
            *amount,
        ),
    };
    let fingerprint = n
        .draft
        .complete()
        .ok()
        .map(|a| a.fingerprint(n.actor.user_id));
    let missing: Vec<String> = n
        .draft
        .missing_fields()
        .iter()
        .map(|m| m.as_str().to_owned())
        .collect();
    let row = sqlx::query_as::<_, ActionRow>(
        "INSERT INTO actions (id, trace_id, user_id, session_id, parent_action_id, kind, risk_class, status, \
         user_text, account_id, card_id, recipient_id, amount_minor, currency, missing_fields, fingerprint, policy_version) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,'PROPOSED',$8,$9,$10,$11,$12,$13,$14,$15,$16) RETURNING *",
    )
    .bind(ActionId::new().as_uuid())
    .bind(n.trace_id.as_uuid())
    .bind(n.actor.user_id.as_uuid())
    .bind(n.actor.session_id.as_uuid())
    .bind(n.parent.map(|p| p.as_uuid()))
    .bind(n.draft.kind().as_str())
    .bind(n.draft.risk_class().as_str())
    .bind(n.user_text)
    .bind(account)
    .bind(card)
    .bind(recipient)
    .bind(amount.map(|a| a.amount_minor))
    .bind(amount.map(|a| a.currency.code()))
    .bind(missing)
    .bind(fingerprint)
    .bind(POLICY_VERSION)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

pub async fn get(conn: &mut PgConnection, id: ActionId) -> Result<ActionRow> {
    sqlx::query_as::<_, ActionRow>("SELECT * FROM actions WHERE id = $1")
        .bind(id.as_uuid())
        .fetch_optional(conn)
        .await?
        .ok_or(StoreError::NotFound)
}

/// Row lock: serialises concurrent confirms / cancels of the same action.
pub async fn get_for_update(conn: &mut PgConnection, id: ActionId) -> Result<ActionRow> {
    sqlx::query_as::<_, ActionRow>("SELECT * FROM actions WHERE id = $1 FOR UPDATE")
        .bind(id.as_uuid())
        .fetch_optional(conn)
        .await?
        .ok_or(StoreError::NotFound)
}

/// Move through the state machine. Illegal transitions are errors, and the
/// `WHERE status = from` guard catches concurrent changes.
pub async fn transition(
    conn: &mut PgConnection,
    id: ActionId,
    from: ActionStatus,
    to: ActionStatus,
) -> Result<()> {
    from.transition(to)
        .map_err(|e| StoreError::IllegalState(e.to_string()))?;
    let res = sqlx::query(
        "UPDATE actions SET status = $3, updated_at = clock_timestamp(), \
         completed_at = CASE WHEN $4 THEN clock_timestamp() ELSE completed_at END \
         WHERE id = $1 AND status = $2",
    )
    .bind(id.as_uuid())
    .bind(from.as_str())
    .bind(to.as_str())
    .bind(to.is_terminal())
    .execute(conn)
    .await?;
    if res.rows_affected() != 1 {
        return Err(StoreError::IllegalState(format!(
            "action {id} was not in {from} (concurrent change?)"
        )));
    }
    Ok(())
}

pub async fn transition_path(
    conn: &mut PgConnection,
    id: ActionId,
    from: ActionStatus,
    path: &[ActionStatus],
) -> Result<()> {
    let mut cur = from;
    for next in path {
        transition(conn, id, cur, *next).await?;
        cur = *next;
    }
    Ok(())
}

pub async fn set_hard_policy(
    conn: &mut PgConnection,
    id: ActionId,
    report: &HardPolicyReport,
) -> Result<()> {
    sqlx::query(
        "UPDATE actions SET hard_policy = $2, updated_at = clock_timestamp() WHERE id = $1",
    )
    .bind(id.as_uuid())
    .bind(serde_json::to_value(report).map_err(|e| StoreError::Corrupt(e.to_string()))?)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_decision(
    conn: &mut PgConnection,
    id: ActionId,
    decision: &PolicyDecision,
    reasons: &Value,
) -> Result<()> {
    let question = match decision {
        PolicyDecision::AskForClarification { question } => Some(question.as_str()),
        _ => None,
    };
    sqlx::query(
        "UPDATE actions SET policy_decision = $2, decision_reasons = $3, \
         clarification_question = COALESCE($4, clarification_question), updated_at = clock_timestamp() WHERE id = $1",
    )
    .bind(id.as_uuid())
    .bind(decision.code())
    .bind(reasons)
    .bind(question)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_clarification_candidate(
    conn: &mut PgConnection,
    id: ActionId,
    minor: i64,
) -> Result<()> {
    sqlx::query("UPDATE actions SET clarification_candidate_minor = $2 WHERE id = $1")
        .bind(id.as_uuid())
        .bind(minor)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn set_result(
    conn: &mut PgConnection,
    id: ActionId,
    result: &Value,
    ledger_tx: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "UPDATE actions SET result = $2, ledger_transaction_id = COALESCE($3, ledger_transaction_id), \
         updated_at = clock_timestamp() WHERE id = $1",
    )
    .bind(id.as_uuid())
    .bind(result)
    .bind(ledger_tx)
    .execute(conn)
    .await?;
    Ok(())
}
