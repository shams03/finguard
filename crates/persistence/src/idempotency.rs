//! Persisted idempotency. Same key + same fingerprint => the stored result;
//! same key + different fingerprint => IDEMPOTENCY_CONFLICT.

use crate::{Result, StoreError};
use finguard_domain::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IdemRecord {
    pub request_fingerprint: String,
    pub action_id: Option<Uuid>,
    pub status: String,
    pub response: Value,
}

/// Fingerprint of a *command* request: actor + normalised text.
pub fn command_fingerprint(user: UserId, text: &str) -> String {
    let norm = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    hex::encode(Sha256::digest(
        format!("finguard-cmd-v1\nactor={user}\ntext={norm}").as_bytes(),
    ))
}

/// Fingerprint of a *confirm* request: the exact action + confirmation.
pub fn confirm_fingerprint(
    user: UserId,
    action_id: Uuid,
    action_fingerprint: &str,
    confirmation: ConfirmationId,
) -> String {
    hex::encode(Sha256::digest(
        format!("finguard-confirm-v1\nactor={user}\naction={action_id}\nfp={action_fingerprint}\nconfirmation={confirmation}").as_bytes(),
    ))
}

pub async fn find(
    conn: &mut PgConnection,
    user: UserId,
    scope: &str,
    key: &IdempotencyKey,
) -> Result<Option<IdemRecord>> {
    Ok(sqlx::query_as::<_, IdemRecord>(
        "SELECT request_fingerprint, action_id, status, response FROM idempotency_records \
         WHERE user_id = $1 AND scope = $2 AND idempotency_key = $3",
    )
    .bind(user.as_uuid())
    .bind(scope)
    .bind(key.as_str())
    .fetch_optional(conn)
    .await?)
}

pub enum Claim {
    /// We own this key now (record inserted as IN_PROGRESS).
    Fresh,
    /// Someone already used this key with the same request.
    Existing(IdemRecord),
}

/// Claim a key before doing work. Race-safe: the UNIQUE constraint decides.
pub async fn claim(
    conn: &mut PgConnection,
    user: UserId,
    scope: &str,
    key: &IdempotencyKey,
    fingerprint: &str,
) -> Result<Claim> {
    let inserted = sqlx::query(
        "INSERT INTO idempotency_records (id, user_id, scope, idempotency_key, request_fingerprint, status) \
         VALUES ($1, $2, $3, $4, $5, 'IN_PROGRESS') ON CONFLICT ON CONSTRAINT idempotency_unique DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(user.as_uuid())
    .bind(scope)
    .bind(key.as_str())
    .bind(fingerprint)
    .execute(&mut *conn)
    .await?;
    if inserted.rows_affected() == 1 {
        return Ok(Claim::Fresh);
    }
    let rec = find(conn, user, scope, key)
        .await?
        .ok_or_else(|| StoreError::Corrupt("idempotency row vanished".into()))?;
    if rec.request_fingerprint != fingerprint {
        return Err(StoreError::domain(
            ErrorCode::IdempotencyConflict,
            "this idempotency key was already used with a different request",
        ));
    }
    Ok(Claim::Existing(rec))
}

pub async fn complete(
    conn: &mut PgConnection,
    user: UserId,
    scope: &str,
    key: &IdempotencyKey,
    action_id: ActionId,
    response: &Value,
) -> Result<()> {
    sqlx::query(
        "UPDATE idempotency_records SET status = 'COMPLETED', action_id = $4, response = $5 \
         WHERE user_id = $1 AND scope = $2 AND idempotency_key = $3",
    )
    .bind(user.as_uuid())
    .bind(scope)
    .bind(key.as_str())
    .bind(action_id.as_uuid())
    .bind(response)
    .execute(conn)
    .await?;
    Ok(())
}

/// Insert an already-completed record (used inside the execution transaction,
/// where the action row lock already serialises concurrent retries).
pub async fn insert_completed(
    conn: &mut PgConnection,
    user: UserId,
    scope: &str,
    key: &IdempotencyKey,
    fingerprint: &str,
    action_id: ActionId,
    response: &Value,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO idempotency_records (id, user_id, scope, idempotency_key, request_fingerprint, action_id, status, response) \
         VALUES ($1, $2, $3, $4, $5, $6, 'COMPLETED', $7)",
    )
    .bind(Uuid::now_v7())
    .bind(user.as_uuid())
    .bind(scope)
    .bind(key.as_str())
    .bind(fingerprint)
    .bind(action_id.as_uuid())
    .bind(response)
    .execute(conn)
    .await?;
    Ok(())
}

/// Drop an unfinished claim so the client can retry after a failure.
pub async fn release(
    conn: &mut PgConnection,
    user: UserId,
    scope: &str,
    key: &IdempotencyKey,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM idempotency_records WHERE user_id = $1 AND scope = $2 AND idempotency_key = $3 AND status = 'IN_PROGRESS'",
    )
    .bind(user.as_uuid())
    .bind(scope)
    .bind(key.as_str())
    .execute(conn)
    .await?;
    Ok(())
}
