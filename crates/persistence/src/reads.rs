//! Read models for the dashboard. All derived from deterministic queries.

use crate::{Result, StoreError};
use finguard_domain::*;
use serde::Serialize;
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct AccountView {
    pub id: AccountId,
    pub name: String,
    pub currency: String,
    pub balance_minor: i64,
    pub balance: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct CardView {
    pub id: CardId,
    pub last4: String,
    pub state: String,
}

#[derive(Debug, Serialize)]
pub struct RecipientView {
    pub id: RecipientId,
    pub display_name: String,
    pub account_masked: String,
    pub age_days: i64,
    pub known: bool,
    pub blocked: bool,
}

#[derive(Debug, Serialize)]
pub struct TransactionView {
    pub id: Uuid,
    pub kind: String,
    pub category: String,
    pub description: String,
    pub amount_minor: i64,
    pub amount: String,
    #[serde(with = "time::serde::rfc3339")]
    pub occurred_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
pub struct ProfileView {
    pub customer_name: String,
    pub device_label: String,
    pub device_trusted: bool,
    pub current_location: String,
    pub normal_location: String,
    pub location_familiar: bool,
    pub typical_transfer_min: String,
    pub typical_transfer_max: String,
}

pub async fn account(conn: &mut PgConnection, user: UserId) -> Result<AccountView> {
    let (id, name, currency, bal, status): (Uuid, String, String, i64, String) = sqlx::query_as(
        "SELECT id, name, currency, balance_minor, status FROM accounts \
         WHERE owner_user_id = $1 AND kind = 'CUSTOMER' ORDER BY created_at, id LIMIT 1",
    )
    .bind(user.as_uuid())
    .fetch_optional(conn)
    .await?
    .ok_or(StoreError::NotFound)?;
    Ok(AccountView {
        id: AccountId(id),
        name,
        currency,
        balance_minor: bal,
        balance: Money::php(bal).pretty(),
        status,
    })
}

pub async fn cards(conn: &mut PgConnection, user: UserId) -> Result<Vec<CardView>> {
    let rows: Vec<(Uuid, String, String)> =
        sqlx::query_as("SELECT id, last4, state FROM cards WHERE user_id = $1 ORDER BY id")
            .bind(user.as_uuid())
            .fetch_all(conn)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(id, last4, state)| CardView {
            id: CardId(id),
            last4,
            state,
        })
        .collect())
}

pub async fn recipients(conn: &mut PgConnection, user: UserId) -> Result<Vec<RecipientView>> {
    let rows: Vec<(Uuid, String, String, i64, bool)> = sqlx::query_as(
        "SELECT id, display_name, account_masked, \
         FLOOR(EXTRACT(EPOCH FROM (clock_timestamp() - created_at)) / 86400)::bigint, blocked \
         FROM recipients WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user.as_uuid())
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, display_name, account_masked, age_days, blocked)| RecipientView {
                id: RecipientId(id),
                display_name,
                account_masked,
                known: age_days >= finguard_policy::NEW_RECIPIENT_DAYS,
                age_days,
                blocked,
            },
        )
        .collect())
}

pub async fn transactions(
    conn: &mut PgConnection,
    user: UserId,
    limit: i64,
) -> Result<Vec<TransactionView>> {
    let rows: Vec<(Uuid, String, String, String, i64, OffsetDateTime)> = sqlx::query_as(
        "SELECT t.id, t.kind, t.category, t.description, t.amount_minor, t.occurred_at \
         FROM transactions t JOIN accounts a ON a.id = t.account_id \
         WHERE a.owner_user_id = $1 ORDER BY t.occurred_at DESC LIMIT $2",
    )
    .bind(user.as_uuid())
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, kind, category, description, amount_minor, occurred_at)| TransactionView {
                id,
                kind,
                category,
                description,
                amount_minor,
                amount: Money::php(amount_minor).pretty(),
                occurred_at,
            },
        )
        .collect())
}

pub async fn profile(conn: &mut PgConnection, actor: &ActorContext) -> Result<ProfileView> {
    let (name, tmin, tmax, normal): (String, i64, i64, String) = sqlx::query_as(
        "SELECT display_name, typical_transfer_min_minor, typical_transfer_max_minor, normal_location FROM users WHERE id = $1",
    )
    .bind(actor.user_id.as_uuid())
    .fetch_one(&mut *conn)
    .await?;
    let (label, trusted): (String, bool) =
        sqlx::query_as("SELECT label, trusted FROM devices WHERE id = $1")
            .bind(actor.device_id.as_uuid())
            .fetch_one(&mut *conn)
            .await?;
    let (loc,): (String,) =
        sqlx::query_as("SELECT current_location FROM demo_sessions WHERE id = $1")
            .bind(actor.session_id.as_uuid())
            .fetch_one(conn)
            .await?;
    Ok(ProfileView {
        customer_name: name,
        device_label: label,
        device_trusted: trusted,
        location_familiar: loc == normal,
        current_location: loc,
        normal_location: normal,
        typical_transfer_min: Money::php(tmin).pretty(),
        typical_transfer_max: Money::php(tmax).pretty(),
    })
}

/// Ledger integrity check used by tests and the demo: balance == SUM(entries).
pub async fn ledger_matches_balances(conn: &mut PgConnection) -> Result<bool> {
    let (bad,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM accounts a WHERE a.balance_minor <> \
         COALESCE((SELECT SUM(amount_minor) FROM ledger_entries e WHERE e.account_id = a.id), 0)",
    )
    .fetch_one(conn)
    .await?;
    Ok(bad == 0)
}
