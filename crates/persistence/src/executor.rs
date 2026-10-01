//! The only code in the system that changes financial state.
//!
//! Everything that matters happens in **one database transaction**:
//!
//! ```text
//! lock action row          (serialises duplicate/concurrent confirms)
//! idempotency check        (same key + same request -> stored result)
//! verify confirmation      (exact action, fingerprint, unexpired, unused)
//! lock customer account row (SELECT ... FOR NO KEY UPDATE)
//! re-run hard policy       (fresh balance; the earlier check was a snapshot)
//! write ledger entries     (append-only, balanced; DB trigger enforces)
//! update balances + action (state machine transitions)
//! audit events + idempotency record
//! COMMIT
//! ```
//!
//! If anything fails the whole transaction rolls back and no funds move.
//! A *blocked* revalidation is committed (so the Blocked status and audit
//! trail persist) but moves no money.

use crate::{
    Result, StoreError,
    actions::{self, ActionRow},
    audit::{self, ev},
    confirmations, idempotency,
};
use finguard_domain::*;
use finguard_policy::{
    AccountFacts, CardFacts, CardState, HardFacts, HardLimits, RecipientFacts, RecipientLookup,
    evaluate,
};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub struct ConfirmParams<'a> {
    pub actor: &'a ActorContext,
    pub action_id: ActionId,
    pub confirmation_id: ConfirmationId,
    pub idempotency_key: Option<&'a IdempotencyKey>,
    pub limits: HardLimits,
}

#[derive(Debug)]
pub enum ConfirmOutcome {
    /// `replayed` = this was a duplicate retry; nothing executed again.
    Executed { action: ActionRow, replayed: bool },
    /// Revalidation inside the transaction failed. No funds moved.
    Blocked {
        action: ActionRow,
        reason: HardPolicyReason,
        replayed: bool,
    },
}

enum Exec {
    Done {
        result: Value,
        ledger_tx: Option<Uuid>,
    },
    Blocked(HardPolicyReason),
}

fn invalid(msg: &str) -> StoreError {
    StoreError::domain(ErrorCode::ConfirmationInvalid, msg)
}

fn outcome_from_record(action: ActionRow, rec: &idempotency::IdemRecord) -> ConfirmOutcome {
    match rec.response.get("outcome").and_then(Value::as_str) {
        Some("BLOCKED") => {
            let reason = rec
                .response
                .get("reason")
                .and_then(Value::as_str)
                .and_then(|r| serde_json::from_value(Value::String(r.to_owned())).ok())
                .unwrap_or(HardPolicyReason::ActionNotAllowed);
            ConfirmOutcome::Blocked {
                action,
                reason,
                replayed: true,
            }
        }
        _ => ConfirmOutcome::Executed {
            action,
            replayed: true,
        },
    }
}

/// Confirm an action and execute it. Safe to retry with the same idempotency key.
pub async fn confirm_and_execute(pool: &PgPool, p: ConfirmParams<'_>) -> Result<ConfirmOutcome> {
    let mut tx = pool.begin().await?;
    let user = p.actor.user_id;

    let action = actions::get_for_update(&mut tx, p.action_id).await?;
    if action.user_id != user.as_uuid() {
        return Err(StoreError::Forbidden);
    }
    let trace = action.trace();
    let aid = action.action_id();

    // ---- idempotency: duplicate retry returns the stored result -------------
    let req_fp = idempotency::confirm_fingerprint(
        user,
        action.id,
        action.fingerprint.as_deref().unwrap_or(""),
        p.confirmation_id,
    );
    if let Some(key) = p.idempotency_key
        && let Some(rec) = idempotency::find(&mut tx, user, "confirm", key).await?
    {
        if rec.request_fingerprint != req_fp {
            return Err(StoreError::domain(
                ErrorCode::IdempotencyConflict,
                "this idempotency key was already used with a different request",
            ));
        }
        audit::record(
            &mut tx,
            trace,
            Some(aid),
            ev::IDEMPOTENT_REPLAY,
            json!({"scope": "confirm"}),
        )
        .await?;
        tx.commit().await?;
        let mut c = pool.acquire().await?;
        let fresh = actions::get(&mut c, aid).await?;
        return Ok(outcome_from_record(fresh, &rec));
    }

    // ---- state gate ----------------------------------------------------------
    let status = action.status()?;
    if status != ActionStatus::AwaitingConfirmation {
        return Err(invalid(&format!(
            "action is {status}; a confirmation can only be used once, while the action awaits confirmation"
        )));
    }

    // ---- verify the confirmation --------------------------------------------
    let conf = confirmations::get_by_action(&mut tx, aid, true)
        .await?
        .ok_or_else(|| invalid("no confirmation exists for this action"))?;
    if conf.id != p.confirmation_id.as_uuid() || conf.user_id != user.as_uuid() {
        return Err(invalid("confirmation does not belong to this action/actor"));
    }
    if conf.used_at.is_some() {
        return Err(invalid("confirmation already used"));
    }
    if confirmations::is_expired(&mut tx, &conf).await? {
        actions::transition(&mut tx, aid, status, ActionStatus::Expired).await?;
        audit::record(
            &mut tx,
            trace,
            Some(aid),
            ev::CONFIRMATION_EXPIRED,
            json!({"confirmation_id": p.confirmation_id.to_string()}),
        )
        .await?;
        tx.commit().await?; // persist the expiry; nothing executed
        return Err(StoreError::domain(
            ErrorCode::ConfirmationExpired,
            "the confirmation expired",
        ));
    }

    // The confirmation covers exactly the action as it was shown. Recompute
    // the fingerprint from the stored parameters: if anything changed since
    // the user approved, refuse and block.
    let proposed = action
        .proposed()?
        .ok_or_else(|| invalid("action is incomplete and cannot be confirmed"))?;
    let fp_now = proposed.fingerprint(user);
    if Some(&fp_now) != action.fingerprint.as_ref() || fp_now != conf.action_fingerprint {
        actions::transition(&mut tx, aid, status, ActionStatus::Blocked).await?;
        audit::record(
            &mut tx,
            trace,
            Some(aid),
            ev::CONFIRMATION_REJECTED,
            json!({"reason": "action changed after confirmation was issued"}),
        )
        .await?;
        tx.commit().await?;
        return Err(invalid(
            "the action changed after the confirmation was issued",
        ));
    }

    // ---- approve, then execute ------------------------------------------------
    confirmations::mark_used(&mut tx, conf.id).await?;
    actions::transition_path(
        &mut tx,
        aid,
        status,
        &[ActionStatus::Approved, ActionStatus::Executing],
    )
    .await?;
    audit::record(
        &mut tx,
        trace,
        Some(aid),
        ev::CONFIRMATION_RECEIVED,
        json!({"confirmation_id": p.confirmation_id.to_string()}),
    )
    .await?;
    audit::record(
        &mut tx,
        trace,
        Some(aid),
        ev::EXECUTION_STARTED,
        json!({"executor": tool_spec(proposed.kind()).executor}),
    )
    .await?;

    let exec = match &proposed {
        ProposedAction::TransferFunds {
            from_account,
            recipient_id,
            amount,
        } => {
            transfer_funds(
                &mut tx,
                p.actor,
                &action,
                *from_account,
                *recipient_id,
                *amount,
                &p.limits,
            )
            .await?
        }
        ProposedAction::FreezeCard { card_id } => {
            freeze_card(&mut tx, p.actor, &action, *card_id, &p.limits).await?
        }
        ProposedAction::ReadBalance { .. } => {
            return Err(StoreError::IllegalState(
                "read-only actions are not confirmed".into(),
            ));
        }
    };

    let (response, blocked_reason) = match exec {
        Exec::Done { result, ledger_tx } => {
            actions::set_result(&mut tx, aid, &result, ledger_tx).await?;
            actions::transition(
                &mut tx,
                aid,
                ActionStatus::Executing,
                ActionStatus::Executed,
            )
            .await?;
            audit::record(
                &mut tx,
                trace,
                Some(aid),
                ev::EXECUTION_COMPLETED,
                json!({"result": result}),
            )
            .await?;
            (json!({"outcome": "EXECUTED"}), None)
        }
        Exec::Blocked(reason) => {
            let result = json!({"blocked": reason.code(), "note": "revalidation failed inside the transaction; no funds moved"});
            actions::set_result(&mut tx, aid, &result, None).await?;
            actions::transition(&mut tx, aid, ActionStatus::Executing, ActionStatus::Blocked)
                .await?;
            audit::record(
                &mut tx,
                trace,
                Some(aid),
                ev::EXECUTION_BLOCKED,
                json!({"reason": reason.code()}),
            )
            .await?;
            (
                json!({"outcome": "BLOCKED", "reason": reason}),
                Some(reason),
            )
        }
    };

    if let Some(key) = p.idempotency_key {
        idempotency::insert_completed(&mut tx, user, "confirm", key, &req_fp, aid, &response)
            .await?;
    }
    tx.commit().await?;

    // Post-commit marker (best effort: the in-transaction events are the
    // durable record; a crash here loses only this convenience event).
    if let Ok(mut c) = pool.acquire().await {
        let _ = audit::record(
            &mut c,
            trace,
            Some(aid),
            ev::TRANSACTION_COMMITTED,
            json!({"outcome": response["outcome"]}),
        )
        .await;
    }
    let mut c = pool.acquire().await?;
    let fresh = actions::get(&mut c, aid).await?;
    Ok(match blocked_reason {
        Some(reason) => ConfirmOutcome::Blocked {
            action: fresh,
            reason,
            replayed: false,
        },
        None => ConfirmOutcome::Executed {
            action: fresh,
            replayed: false,
        },
    })
}

async fn transfer_funds(
    tx: &mut PgConnection,
    actor: &ActorContext,
    action: &ActionRow,
    from: AccountId,
    recipient: RecipientId,
    amount: Money,
    limits: &HardLimits,
) -> Result<Exec> {
    let trace = action.trace();
    let aid = action.action_id();

    // Counter-account (demo settlement); located without locking.
    let (settlement,): (Uuid,) = sqlx::query_as(
        "SELECT id FROM accounts WHERE kind = 'SETTLEMENT' AND currency = $1 ORDER BY id LIMIT 1",
    )
    .bind(amount.currency.code())
    .fetch_one(&mut *tx)
    .await?;

    // THE lock. A competing transfer from the same account blocks here until
    // we commit, then reads the *new* balance. Only the customer's row is
    // locked explicitly; the settlement counter-balance is changed later by an
    // atomic `UPDATE ... SET balance = balance + $1`, so every transaction
    // acquires its row locks in the same order (customer, then settlement).
    let locked: Vec<(Uuid, Option<Uuid>, i64, String)> = sqlx::query_as(
        "SELECT id, owner_user_id, balance_minor, status FROM accounts WHERE id = $1 FOR NO KEY UPDATE",
    )
    .bind(from.as_uuid())
    .fetch_all(&mut *tx)
    .await?;
    audit::record(
        tx,
        trace,
        Some(aid),
        ev::ACCOUNT_ROW_LOCKED,
        json!({"accounts_locked": locked.len()}),
    )
    .await?;

    let account = locked
        .iter()
        .find(|r| r.0 == from.as_uuid())
        .map(|(_, owner, bal, status)| AccountFacts {
            owner: owner.map(UserId),
            balance: Money::new(*bal, amount.currency),
            active: status == "ACTIVE",
        });
    let rcp: Option<(Uuid, bool, String)> = sqlx::query_as(
        "SELECT user_id, blocked, display_name FROM recipients WHERE id = $1 FOR SHARE",
    )
    .bind(recipient.as_uuid())
    .fetch_optional(&mut *tx)
    .await?;
    let recipient_name = rcp.as_ref().map(|r| r.2.clone()).unwrap_or_default();
    let facts = HardFacts {
        account: account.clone(),
        card: None,
        recipient: match rcp {
            Some((owner, blocked, _)) => RecipientLookup::Found(RecipientFacts {
                owner: UserId(owner),
                blocked,
            }),
            None => RecipientLookup::NotFound,
        },
    };
    let draft = ActionDraft::from(ProposedAction::TransferFunds {
        from_account: from,
        recipient_id: recipient,
        amount,
    });
    let report = evaluate(Some(actor), &draft, &facts, limits);
    audit::record(
        tx,
        trace,
        Some(aid),
        ev::BALANCE_REVALIDATED,
        json!({
            "result": report.result,
            "available_minor": account.as_ref().map(|a| a.balance.amount_minor),
            "requested_minor": amount.amount_minor,
        }),
    )
    .await?;
    if let HardPolicyResult::Block(reason) = report.result {
        return Ok(Exec::Blocked(reason));
    }

    // Append-only double-entry: customer -amount, settlement +amount.
    let ltx = Uuid::now_v7();
    sqlx::query("INSERT INTO ledger_transactions (id, action_id, kind, description) VALUES ($1, $2, 'TRANSFER', $3)")
        .bind(ltx)
        .bind(aid.as_uuid())
        .bind(format!("Transfer {} to {}", amount.to_api_string(), recipient_name))
        .execute(&mut *tx)
        .await?;
    for (acct, delta) in [
        (from.as_uuid(), -amount.amount_minor),
        (settlement, amount.amount_minor),
    ] {
        sqlx::query("INSERT INTO ledger_entries (ledger_transaction_id, account_id, amount_minor, currency) VALUES ($1, $2, $3, $4)")
            .bind(ltx).bind(acct).bind(delta).bind(amount.currency.code())
            .execute(&mut *tx).await?;
        // Materialised balance; the CHECK constraint is the last line of defence.
        sqlx::query("UPDATE accounts SET balance_minor = balance_minor + $2 WHERE id = $1")
            .bind(acct)
            .bind(delta)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO transactions (id, account_id, kind, category, description, amount_minor, currency, recipient_id, ledger_transaction_id, occurred_at) \
         VALUES ($1, $2, 'TRANSFER_OUT', 'transfer', $3, $4, $5, $6, $7, clock_timestamp())",
    )
    .bind(Uuid::now_v7())
    .bind(from.as_uuid())
    .bind(format!("Transfer to {recipient_name}"))
    .bind(-amount.amount_minor)
    .bind(amount.currency.code())
    .bind(recipient.as_uuid())
    .bind(ltx)
    .execute(&mut *tx)
    .await?;
    let new_balance = account
        .map(|a| a.balance.amount_minor - amount.amount_minor)
        .unwrap_or(0);
    audit::record(
        tx, trace, Some(aid), ev::LEDGER_ENTRIES_WRITTEN,
        json!({"ledger_transaction_id": LedgerTransactionId(ltx).to_string(), "entries": 2, "amount_minor": amount.amount_minor}),
    )
    .await?;
    Ok(Exec::Done {
        result: json!({
            "transferred": amount.to_api_string(),
            "to": recipient_name,
            "new_balance_minor": new_balance,
            "new_balance": Money::php(new_balance).pretty(),
            "ledger_transaction_id": LedgerTransactionId(ltx).to_string(),
        }),
        ledger_tx: Some(ltx),
    })
}

async fn freeze_card(
    tx: &mut PgConnection,
    actor: &ActorContext,
    action: &ActionRow,
    card: CardId,
    limits: &HardLimits,
) -> Result<Exec> {
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT user_id, state FROM cards WHERE id = $1 FOR UPDATE")
            .bind(card.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
    let facts = HardFacts {
        account: None,
        card: row.as_ref().map(|(owner, state)| CardFacts {
            owner: UserId(*owner),
            state: if state == "FROZEN" {
                CardState::Frozen
            } else {
                CardState::Active
            },
        }),
        recipient: RecipientLookup::NotRequested,
    };
    let report = evaluate(
        Some(actor),
        &ActionDraft::FreezeCard { card_id: card },
        &facts,
        limits,
    );
    audit::record(
        tx,
        action.trace(),
        Some(action.action_id()),
        ev::BALANCE_REVALIDATED,
        json!({"result": report.result, "subject": "card"}),
    )
    .await?;
    if let HardPolicyResult::Block(reason) = report.result {
        return Ok(Exec::Blocked(reason));
    }
    let already = row.map(|r| r.1 == "FROZEN").unwrap_or(false);
    if !already {
        sqlx::query(
            "UPDATE cards SET state = 'FROZEN', updated_at = clock_timestamp() WHERE id = $1",
        )
        .bind(card.as_uuid())
        .execute(&mut *tx)
        .await?;
    }
    // Freezing is idempotent: an already-frozen card returns the existing state.
    Ok(Exec::Done {
        result: json!({"card_state": "FROZEN", "already_frozen": already}),
        ledger_tx: None,
    })
}

/// Policy-approved read-only execution (no confirmation, no Jev).
pub async fn execute_read_balance(
    pool: &PgPool,
    actor: &ActorContext,
    action_id: ActionId,
    limits: &HardLimits,
) -> Result<ActionRow> {
    let mut tx = pool.begin().await?;
    let action = actions::get_for_update(&mut tx, action_id).await?;
    if action.user_id != actor.user_id.as_uuid() {
        return Err(StoreError::Forbidden);
    }
    let status = action.status()?;
    let trace = action.trace();
    let ActionDraft::ReadBalance { account_id } = action.draft()? else {
        return Err(StoreError::IllegalState("not a read-balance action".into()));
    };
    actions::transition_path(&mut tx, action_id, status, &[ActionStatus::Executing]).await?;
    audit::record(
        &mut tx,
        trace,
        Some(action_id),
        ev::EXECUTION_STARTED,
        json!({"executor": "executor::read_balance"}),
    )
    .await?;
    let row: Option<(Option<Uuid>, i64, String)> =
        sqlx::query_as("SELECT owner_user_id, balance_minor, status FROM accounts WHERE id = $1")
            .bind(account_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
    // Re-check ownership at execution time too.
    let facts = HardFacts {
        account: row.as_ref().map(|(o, b, s)| AccountFacts {
            owner: o.map(UserId),
            balance: Money::php(*b),
            active: s == "ACTIVE",
        }),
        card: None,
        recipient: RecipientLookup::NotRequested,
    };
    let report = evaluate(
        Some(actor),
        &ActionDraft::ReadBalance { account_id },
        &facts,
        limits,
    );
    if let HardPolicyResult::Block(reason) = report.result {
        actions::set_result(&mut tx, action_id, &json!({"blocked": reason.code()}), None).await?;
        actions::transition(
            &mut tx,
            action_id,
            ActionStatus::Executing,
            ActionStatus::Blocked,
        )
        .await?;
        audit::record(
            &mut tx,
            trace,
            Some(action_id),
            ev::EXECUTION_BLOCKED,
            json!({"reason": reason.code()}),
        )
        .await?;
        tx.commit().await?;
        return Err(StoreError::domain(ErrorCode::Forbidden, reason.code()));
    }
    let bal = row.map(|r| r.1).unwrap_or(0);
    let result =
        json!({"balance_minor": bal, "currency": "PHP", "balance": Money::php(bal).pretty()});
    actions::set_result(&mut tx, action_id, &result, None).await?;
    actions::transition(
        &mut tx,
        action_id,
        ActionStatus::Executing,
        ActionStatus::Executed,
    )
    .await?;
    audit::record(
        &mut tx,
        trace,
        Some(action_id),
        ev::EXECUTION_COMPLETED,
        json!({"result": result}),
    )
    .await?;
    tx.commit().await?;
    let mut c = pool.acquire().await?;
    actions::get(&mut c, action_id).await
}

/// User-initiated cancel (confirmation declined, or clarification abandoned).
pub async fn cancel(
    pool: &PgPool,
    actor: &ActorContext,
    action_id: ActionId,
    reason: &str,
) -> Result<ActionRow> {
    let mut tx = pool.begin().await?;
    let action = actions::get_for_update(&mut tx, action_id).await?;
    if action.user_id != actor.user_id.as_uuid() {
        return Err(StoreError::Forbidden);
    }
    let status = action.status()?;
    if status.is_terminal() {
        // Cancelling something already finished is a no-op for Cancelled,
        // an error for anything else (you cannot cancel an executed transfer).
        if status == ActionStatus::Cancelled {
            let mut c = pool.acquire().await?;
            return actions::get(&mut c, action_id).await;
        }
        return Err(StoreError::domain(
            ErrorCode::InvalidRequest,
            format!("action is already {status}"),
        ));
    }
    actions::transition(&mut tx, action_id, status, ActionStatus::Cancelled).await?;
    audit::record(
        &mut tx,
        action.trace(),
        Some(action_id),
        ev::ACTION_CANCELLED,
        json!({"reason": reason, "from": status}),
    )
    .await?;
    tx.commit().await?;
    let mut c = pool.acquire().await?;
    actions::get(&mut c, action_id).await
}
