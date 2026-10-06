//! Loading facts for policy. Deterministic code reads the database; models
//! never do. Execution re-loads (and locks) its own facts — see `executor`.

use crate::{Result, StoreError};
use finguard_domain::*;
use finguard_policy::{
    AccountFacts, CardFacts, CardState, HardFacts, NEW_RECIPIENT_DAYS, RecipientFacts,
    RecipientLookup,
};
use sqlx::PgConnection;
use uuid::Uuid;

/// Resolve the demo session into an `ActorContext`. In production this is
/// where a verified identity-provider token would be turned into an actor.
pub async fn load_actor(conn: &mut PgConnection, session_id: Uuid) -> Result<ActorContext> {
    let row: Option<(Uuid, Uuid, Vec<String>)> =
        sqlx::query_as("SELECT user_id, device_id, permissions FROM demo_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_optional(conn)
            .await?;
    let (user, device, perms) = row.ok_or(StoreError::NotFound)?;
    let permissions = perms
        .iter()
        .filter_map(|p| match p.as_str() {
            "READ_ACCOUNT" => Some(Permission::ReadAccount),
            "FREEZE_CARD" => Some(Permission::FreezeCard),
            "TRANSFER_FUNDS" => Some(Permission::TransferFunds),
            _ => None,
        })
        .collect();
    Ok(ActorContext {
        user_id: UserId(user),
        session_id: SessionId(session_id),
        permissions,
        device_id: DeviceId(device),
    })
}

/// The actor's primary customer account and active card ids (demo has one each).
pub async fn primary_ids(conn: &mut PgConnection, user: UserId) -> Result<(AccountId, CardId)> {
    let acc: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM accounts WHERE owner_user_id = $1 AND kind = 'CUSTOMER' ORDER BY created_at, id LIMIT 1",
    )
    .bind(user.as_uuid())
    .fetch_optional(&mut *conn)
    .await?;
    let card: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM cards WHERE user_id = $1 ORDER BY id LIMIT 1")
            .bind(user.as_uuid())
            .fetch_optional(conn)
            .await?;
    match (acc, card) {
        (Some((a,)), Some((c,))) => Ok((AccountId(a), CardId(c))),
        _ => Err(StoreError::NotFound),
    }
}

#[derive(Debug, Clone)]
pub struct RecipientInfo {
    pub id: RecipientId,
    pub display_name: String,
    pub age_days: i64,
}

#[derive(Debug, Clone)]
pub enum RecipientResolution {
    Found(RecipientInfo),
    NotFound,
    Ambiguous(Vec<String>),
}

pub async fn list_recipients(conn: &mut PgConnection, user: UserId) -> Result<Vec<RecipientInfo>> {
    let rows: Vec<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT id, display_name, FLOOR(EXTRACT(EPOCH FROM (clock_timestamp() - created_at)) / 86400)::bigint \
         FROM recipients WHERE user_id = $1 ORDER BY created_at",
    )
    .bind(user.as_uuid())
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, display_name, age_days)| RecipientInfo {
            id: RecipientId(id),
            display_name,
            age_days,
        })
        .collect())
}

/// Max saved payees per user (keeps the demo address book small).
pub const MAX_RECIPIENTS: i64 = 12;

/// Outcome of adding a payee.
pub enum AddRecipient {
    Added(RecipientId),
    Duplicate,
    LimitReached,
}

/// Add a payee to the user's address book. It starts "new" (age 0), so the
/// normal new-recipient risk rules apply to it until it has aged.
pub async fn add_recipient(
    conn: &mut PgConnection,
    user: UserId,
    display_name: &str,
) -> Result<AddRecipient> {
    let (count, dup): (i64, bool) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(BOOL_OR(LOWER(display_name) = LOWER($2)), FALSE) FROM recipients WHERE user_id = $1",
    )
    .bind(user.as_uuid())
    .bind(display_name)
    .fetch_one(&mut *conn)
    .await?;
    if dup {
        return Ok(AddRecipient::Duplicate);
    }
    if count >= MAX_RECIPIENTS {
        return Ok(AddRecipient::LimitReached);
    }
    let id = Uuid::now_v7();
    let masked = format!("•••• {:04}", id.as_u128() % 10_000);
    sqlx::query(
        "INSERT INTO recipients (id, user_id, display_name, account_masked, created_at) VALUES ($1, $2, $3, $4, clock_timestamp())",
    )
    .bind(id)
    .bind(user.as_uuid())
    .bind(display_name)
    .bind(masked)
    .execute(conn)
    .await?;
    Ok(AddRecipient::Added(RecipientId(id)))
}

/// Map a free-text reference ("Alex", "this new account") onto the user's own
/// address book. Deterministic: a model's string never becomes an id without
/// passing through here.
pub async fn resolve_recipient(
    conn: &mut PgConnection,
    user: UserId,
    reference: &str,
) -> Result<RecipientResolution> {
    let all = list_recipients(conn, user).await?;
    let r = reference.trim().to_lowercase();
    if r.is_empty() {
        return Ok(RecipientResolution::NotFound);
    }
    let tokens: Vec<&str> = r
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    // "new account" / "new recipient" / "unknown payee" -> the newest recipient.
    if tokens
        .iter()
        .any(|t| *t == "new" || *t == "unknown" || *t == "unverified")
    {
        return Ok(all
            .into_iter()
            .filter(|x| x.age_days < NEW_RECIPIENT_DAYS)
            .min_by_key(|x| x.age_days)
            .map(RecipientResolution::Found)
            .unwrap_or(RecipientResolution::NotFound));
    }
    let matches: Vec<RecipientInfo> = all
        .into_iter()
        .filter(|rec| {
            let name = rec.display_name.to_lowercase();
            let name_tokens: Vec<&str> = name
                .split(|c: char| !c.is_alphanumeric())
                .filter(|t| !t.is_empty())
                .collect();
            tokens
                .iter()
                .any(|t| t.len() >= 2 && name_tokens.contains(t))
        })
        .collect();
    Ok(match matches.len() {
        0 => RecipientResolution::NotFound,
        1 => RecipientResolution::Found(matches.into_iter().next().expect("len checked")),
        _ => RecipientResolution::Ambiguous(matches.into_iter().map(|m| m.display_name).collect()),
    })
}

#[derive(Debug, Clone)]
pub struct UsualAmount {
    pub amount: Money,
    pub sample_size: usize,
}

/// Median of the user's last (up to) 5 transfers to this recipient: the only
/// evidence a clarification may use to suggest "the usual amount".
pub async fn usual_amount(
    conn: &mut PgConnection,
    recipient: RecipientId,
) -> Result<Option<UsualAmount>> {
    let rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT -amount_minor FROM transactions WHERE recipient_id = $1 AND kind = 'TRANSFER_OUT' \
         ORDER BY occurred_at DESC LIMIT 5",
    )
    .bind(recipient.as_uuid())
    .fetch_all(conn)
    .await?;
    if rows.len() < 3 {
        return Ok(None); // not enough history to call anything "usual"
    }
    let mut v: Vec<i64> = rows.into_iter().map(|r| r.0).collect();
    v.sort_unstable();
    Ok(Some(UsualAmount {
        amount: Money::php(v[v.len() / 2]),
        sample_size: v.len(),
    }))
}

async fn account_facts(conn: &mut PgConnection, id: AccountId) -> Result<Option<AccountFacts>> {
    let row: Option<(Option<Uuid>, i64, String, String)> = sqlx::query_as(
        "SELECT owner_user_id, balance_minor, currency, status FROM accounts WHERE id = $1",
    )
    .bind(id.as_uuid())
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|(owner, bal, _cur, status)| AccountFacts {
        owner: owner.map(UserId),
        balance: Money::php(bal),
        active: status == "ACTIVE",
    }))
}

pub async fn recipient_lookup(
    conn: &mut PgConnection,
    id: Option<RecipientId>,
) -> Result<RecipientLookup> {
    let Some(id) = id else {
        return Ok(RecipientLookup::NotRequested);
    };
    let row: Option<(Uuid, bool)> =
        sqlx::query_as("SELECT user_id, blocked FROM recipients WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(conn)
            .await?;
    Ok(match row {
        Some((owner, blocked)) => RecipientLookup::Found(RecipientFacts {
            owner: UserId(owner),
            blocked,
        }),
        None => RecipientLookup::NotFound,
    })
}

/// Snapshot used by *pre-AI* hard policy (execution loads its own locked copy).
pub async fn load_hard_facts(conn: &mut PgConnection, draft: &ActionDraft) -> Result<HardFacts> {
    let mut facts = HardFacts {
        account: None,
        card: None,
        recipient: RecipientLookup::NotRequested,
    };
    match draft {
        ActionDraft::ReadBalance { account_id } => {
            facts.account = account_facts(conn, *account_id).await?
        }
        ActionDraft::FreezeCard { card_id } => {
            let row: Option<(Uuid, String)> =
                sqlx::query_as("SELECT user_id, state FROM cards WHERE id = $1")
                    .bind(card_id.as_uuid())
                    .fetch_optional(conn)
                    .await?;
            facts.card = row.map(|(owner, state)| CardFacts {
                owner: UserId(owner),
                state: if state == "FROZEN" {
                    CardState::Frozen
                } else {
                    CardState::Active
                },
            });
        }
        ActionDraft::TransferFunds {
            from_account,
            recipient_id,
            ..
        } => {
            facts.account = account_facts(conn, *from_account).await?;
            facts.recipient = recipient_lookup(conn, *recipient_id).await?;
        }
    }
    Ok(facts)
}

/// Build the (never-sent-to-vendors) full fact set for risk assessment.
pub async fn load_risk_facts(
    conn: &mut PgConnection,
    actor: &ActorContext,
    draft: &ActionDraft,
    user_text: &str,
) -> Result<RiskFacts> {
    let (name, tmin, tmax, normal_loc): (String, i64, i64, String) = sqlx::query_as(
        "SELECT display_name, typical_transfer_min_minor, typical_transfer_max_minor, normal_location FROM users WHERE id = $1",
    )
    .bind(actor.user_id.as_uuid())
    .fetch_one(&mut *conn)
    .await?;
    let (device_trusted,): (bool,) = sqlx::query_as("SELECT trusted FROM devices WHERE id = $1")
        .bind(actor.device_id.as_uuid())
        .fetch_one(&mut *conn)
        .await?;
    let (loc,): (String,) =
        sqlx::query_as("SELECT current_location FROM demo_sessions WHERE id = $1")
            .bind(actor.session_id.as_uuid())
            .fetch_one(&mut *conn)
            .await?;

    let (account_id, recipient_id, amount) = match draft {
        ActionDraft::TransferFunds {
            from_account,
            recipient_id,
            amount,
        } => (Some(*from_account), *recipient_id, *amount),
        ActionDraft::ReadBalance { account_id } => (Some(*account_id), None, None),
        ActionDraft::FreezeCard { .. } => (None, None, None),
    };
    let (mut tx_hour, mut transfer_hour, mut balance) = (0_i64, 0_i64, None);
    let (mut acct_number, mut account_name) = (String::new(), String::new());
    if let Some(a) = account_id {
        let (t, tr): (i64, i64) = sqlx::query_as(
            "SELECT COUNT(*), COUNT(*) FILTER (WHERE kind = 'TRANSFER_OUT') FROM transactions \
             WHERE account_id = $1 AND occurred_at > clock_timestamp() - interval '1 hour'",
        )
        .bind(a.as_uuid())
        .fetch_one(&mut *conn)
        .await?;
        tx_hour = t;
        transfer_hour = tr;
        if let Some(f) = account_facts(&mut *conn, a).await? {
            balance = Some(f.balance);
        }
        let (n,): (String,) = sqlx::query_as("SELECT name FROM accounts WHERE id = $1")
            .bind(a.as_uuid())
            .fetch_one(&mut *conn)
            .await?;
        account_name = n;
        acct_number = a.to_string();
    }
    let (mut known, mut age_days, mut display) = (false, None, None);
    if let Some(r) = recipient_id {
        let row: Option<(String, i64)> = sqlx::query_as(
            "SELECT display_name, FLOOR(EXTRACT(EPOCH FROM (clock_timestamp() - created_at)) / 86400)::bigint \
             FROM recipients WHERE id = $1",
        )
        .bind(r.as_uuid())
        .fetch_optional(&mut *conn)
        .await?;
        if let Some((dn, age)) = row {
            known = age >= NEW_RECIPIENT_DAYS;
            age_days = Some(age);
            display = Some(dn);
        }
    }
    let _ = account_name;
    Ok(RiskFacts {
        action_kind: draft.kind(),
        user_request: user_text.to_owned(),
        amount,
        recipient_known: known,
        recipient_age_days: age_days,
        recipient_display_name: display,
        device_trusted,
        location_familiar: loc == normal_loc,
        transactions_last_hour: tx_hour as u32,
        transfers_last_hour: transfer_hour as u32,
        usual_transfer_min: Some(Money::php(tmin)),
        usual_transfer_max: Some(Money::php(tmax)),
        missing_fields: draft.missing_fields(),
        customer_name: name,
        account_number: acct_number,
        balance,
    })
}
