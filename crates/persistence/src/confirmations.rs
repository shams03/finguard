use crate::{Result, StoreError};
use finguard_domain::*;
use sqlx::PgConnection;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ConfirmationRow {
    pub id: Uuid,
    pub action_id: Uuid,
    pub user_id: Uuid,
    pub action_fingerprint: String,
    pub created_at: OffsetDateTime,
    pub expires_at: OffsetDateTime,
    pub used_at: Option<OffsetDateTime>,
}

/// A confirmation binds to **one action and its fingerprint** and is
/// single-use. If the action changes, the fingerprint no longer matches.
pub async fn create(
    conn: &mut PgConnection,
    action_id: ActionId,
    user: UserId,
    fingerprint: &str,
    ttl_secs: i64,
) -> Result<ConfirmationRow> {
    Ok(sqlx::query_as::<_, ConfirmationRow>(
        "INSERT INTO confirmations (id, action_id, user_id, action_fingerprint, expires_at) \
         VALUES ($1, $2, $3, $4, clock_timestamp() + make_interval(secs => $5)) RETURNING *",
    )
    .bind(ConfirmationId::new().as_uuid())
    .bind(action_id.as_uuid())
    .bind(user.as_uuid())
    .bind(fingerprint)
    .bind(ttl_secs as f64)
    .fetch_one(conn)
    .await?)
}

pub async fn get_by_action(
    conn: &mut PgConnection,
    action_id: ActionId,
    lock: bool,
) -> Result<Option<ConfirmationRow>> {
    let sql = if lock {
        "SELECT * FROM confirmations WHERE action_id = $1 FOR UPDATE"
    } else {
        "SELECT * FROM confirmations WHERE action_id = $1"
    };
    Ok(sqlx::query_as::<_, ConfirmationRow>(sql)
        .bind(action_id.as_uuid())
        .fetch_optional(conn)
        .await?)
}

pub async fn mark_used(conn: &mut PgConnection, id: Uuid) -> Result<()> {
    let r = sqlx::query(
        "UPDATE confirmations SET used_at = clock_timestamp() WHERE id = $1 AND used_at IS NULL",
    )
    .bind(id)
    .execute(conn)
    .await?;
    if r.rows_affected() != 1 {
        return Err(StoreError::domain(
            ErrorCode::ConfirmationInvalid,
            "confirmation already used",
        ));
    }
    Ok(())
}

pub async fn is_expired(conn: &mut PgConnection, c: &ConfirmationRow) -> Result<bool> {
    let (expired,): (bool,) = sqlx::query_as("SELECT clock_timestamp() >= $1")
        .bind(c.expires_at)
        .fetch_one(conn)
        .await?;
    Ok(expired)
}
