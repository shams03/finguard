//! Synthetic demo data (fictional). `reset_demo` restores a known state so
//! demos are repeatable; scenarios are just parameterised resets.

use crate::{Result, StoreError};
use finguard_domain::*;
use sqlx::PgPool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub const USER: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0001);
pub const SESSION: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0002);
pub const DEVICE_TRUSTED: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0003);
pub const DEVICE_UNKNOWN: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0004);
pub const ACCOUNT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0010);
pub const SETTLEMENT: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0011);
pub const CARD: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0020);
pub const RECIPIENT_ALEX: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0030);
pub const RECIPIENT_NEW: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_0000_0031);

pub const NORMAL_LOCATION: &str = "Manila, PH";

/// Tunable starting state. `Default` is the canonical "Maya" demo.
#[derive(Debug, Clone)]
pub struct DemoSetup {
    pub balance_minor: i64,
    pub device_trusted: bool,
    pub location: String,
    /// Transfers made within the last hour (velocity).
    pub recent_transfers: u32,
}

impl Default for DemoSetup {
    fn default() -> Self {
        Self {
            balance_minor: Money::php_major(120_000).amount_minor,
            device_trusted: true,
            location: NORMAL_LOCATION.into(),
            recent_transfers: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoScenario {
    Normal,
    Suspicious,
    LowBalance,
}

impl DemoScenario {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "normal" => Some(Self::Normal),
            "suspicious" => Some(Self::Suspicious),
            "low_balance" => Some(Self::LowBalance),
            _ => None,
        }
    }
    pub fn setup(self) -> DemoSetup {
        match self {
            Self::Normal => DemoSetup::default(),
            Self::Suspicious => DemoSetup {
                device_trusted: false,
                location: "Lagos, NG".into(),
                recent_transfers: 3,
                ..DemoSetup::default()
            },
            Self::LowBalance => DemoSetup {
                balance_minor: Money::php_major(20_000).amount_minor,
                ..DemoSetup::default()
            },
        }
    }
}

/// Wipe every table and re-seed. Demo-only: TRUNCATE bypasses the
/// append-only row triggers on purpose, which is why the API disables this in
/// production.
pub async fn reset_demo(pool: &PgPool, setup: &DemoSetup) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "TRUNCATE audit_events, jev_evaluations, groq_evaluations, idempotency_records, confirmations, \
         ledger_entries, ledger_transactions, transactions, actions, cards, recipients, demo_sessions, \
         devices, accounts, users RESTART IDENTITY CASCADE",
    )
    .execute(&mut *tx)
    .await?;

    let now = OffsetDateTime::now_utc();
    let day = |d: i64| now - Duration::days(d);

    sqlx::query(
        "INSERT INTO users (id, display_name, primary_currency, typical_transfer_min_minor, \
         typical_transfer_max_minor, normal_location) VALUES ($1, 'Maya Santos', 'PHP', $2, $3, $4)",
    )
    .bind(USER)
    .bind(Money::php_major(2_000).amount_minor)
    .bind(Money::php_major(8_000).amount_minor)
    .bind(NORMAL_LOCATION)
    .execute(&mut *tx)
    .await?;

    for (id, label, trusted) in [
        (DEVICE_TRUSTED, "MacBook Pro (known device)", true),
        (DEVICE_UNKNOWN, "Unrecognised Android device", false),
    ] {
        sqlx::query("INSERT INTO devices (id, user_id, label, trusted) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(USER)
            .bind(label)
            .bind(trusted)
            .execute(&mut *tx)
            .await?;
    }
    let device = if setup.device_trusted {
        DEVICE_TRUSTED
    } else {
        DEVICE_UNKNOWN
    };
    sqlx::query(
        "INSERT INTO demo_sessions (id, user_id, device_id, current_location, permissions) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(SESSION)
    .bind(USER)
    .bind(device)
    .bind(&setup.location)
    .bind(vec![
        "READ_ACCOUNT".to_owned(),
        "FREEZE_CARD".to_owned(),
        "TRANSFER_FUNDS".to_owned(),
    ])
    .execute(&mut *tx)
    .await?;

    // Accounts: customer starts at 0 and receives an opening-balance ledger
    // transaction so balance == SUM(ledger entries) holds from the start.
    sqlx::query("INSERT INTO accounts (id, owner_user_id, name, kind, currency, balance_minor) VALUES ($1, $2, 'Main Account', 'CUSTOMER', 'PHP', 0)")
        .bind(ACCOUNT).bind(USER).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO accounts (id, owner_user_id, name, kind, currency, balance_minor) VALUES ($1, NULL, 'Demo settlement', 'SETTLEMENT', 'PHP', 0)")
        .bind(SETTLEMENT).execute(&mut *tx).await?;

    let opening = Uuid::now_v7();
    sqlx::query("INSERT INTO ledger_transactions (id, kind, description) VALUES ($1, 'OPENING_BALANCE', 'Demo opening balance')")
        .bind(opening).execute(&mut *tx).await?;
    for (acct, amt) in [
        (ACCOUNT, setup.balance_minor),
        (SETTLEMENT, -setup.balance_minor),
    ] {
        sqlx::query("INSERT INTO ledger_entries (ledger_transaction_id, account_id, amount_minor, currency) VALUES ($1, $2, $3, 'PHP')")
            .bind(opening).bind(acct).bind(amt).execute(&mut *tx).await?;
        sqlx::query("UPDATE accounts SET balance_minor = balance_minor + $2 WHERE id = $1")
            .bind(acct)
            .bind(amt)
            .execute(&mut *tx)
            .await?;
    }

    sqlx::query("INSERT INTO cards (id, user_id, account_id, last4, state) VALUES ($1, $2, $3, '4242', 'ACTIVE')")
        .bind(CARD).bind(USER).bind(ACCOUNT).execute(&mut *tx).await?;

    sqlx::query("INSERT INTO recipients (id, user_id, display_name, account_masked, created_at) VALUES ($1, $2, 'Alex Rivera', '•••• 7788', $3)")
        .bind(RECIPIENT_ALEX).bind(USER).bind(day(240)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO recipients (id, user_id, display_name, account_masked, created_at) VALUES ($1, $2, 'New payee (unverified)', '•••• 4417', $3)")
        .bind(RECIPIENT_NEW).bind(USER).bind(day(2)).execute(&mut *tx).await?;

    // History: (days ago, kind, category, description, signed centavos, recipient)
    let hist: Vec<(i64, &str, &str, &str, i64, Option<Uuid>)> = vec![
        (
            30,
            "SALARY",
            "income",
            "Salary - Northwind Ltd",
            8_500_000,
            None,
        ),
        (
            29,
            "UTILITY",
            "utilities",
            "Meralco electricity",
            -380_000,
            None,
        ),
        (
            27,
            "SUBSCRIPTION",
            "subscriptions",
            "Netflix",
            -54_900,
            None,
        ),
        (
            26,
            "SUBSCRIPTION",
            "subscriptions",
            "Spotify",
            -14_900,
            None,
        ),
        (
            21,
            "CARD_PURCHASE",
            "grocery",
            "SM Supermarket",
            -235_000,
            None,
        ),
        (
            14,
            "CARD_PURCHASE",
            "grocery",
            "Rustan's Fresh",
            -182_500,
            None,
        ),
        (9, "CARD_PURCHASE", "transport", "Grab ride", -32_000, None),
        (
            6,
            "CARD_PURCHASE",
            "coffee",
            "Toby's Estate Coffee",
            -18_500,
            None,
        ),
        (4, "CARD_PURCHASE", "transport", "Grab ride", -28_000, None),
        (2, "CARD_PURCHASE", "coffee", "Starbucks", -21_000, None),
        (1, "CARD_PURCHASE", "grocery", "7-Eleven", -9_500, None),
        // Transfers to Alex — the evidence for "the usual amount" (median = 5,000)
        (
            15,
            "TRANSFER_OUT",
            "transfer",
            "Transfer to Alex Rivera",
            -500_000,
            Some(RECIPIENT_ALEX),
        ),
        (
            30,
            "TRANSFER_OUT",
            "transfer",
            "Transfer to Alex Rivera",
            -800_000,
            Some(RECIPIENT_ALEX),
        ),
        (
            45,
            "TRANSFER_OUT",
            "transfer",
            "Transfer to Alex Rivera",
            -500_000,
            Some(RECIPIENT_ALEX),
        ),
        (
            60,
            "TRANSFER_OUT",
            "transfer",
            "Transfer to Alex Rivera",
            -200_000,
            Some(RECIPIENT_ALEX),
        ),
        (
            75,
            "TRANSFER_OUT",
            "transfer",
            "Transfer to Alex Rivera",
            -500_000,
            Some(RECIPIENT_ALEX),
        ),
        (
            105,
            "TRANSFER_OUT",
            "transfer",
            "Transfer to Alex Rivera",
            -500_000,
            Some(RECIPIENT_ALEX),
        ),
    ];
    for (d, kind, cat, desc, amt, rcp) in hist {
        insert_history(&mut tx, kind, cat, desc, amt, rcp, day(d)).await?;
    }
    for i in 0..setup.recent_transfers {
        insert_history(
            &mut tx,
            "TRANSFER_OUT",
            "transfer",
            "Transfer to Alex Rivera",
            -150_000,
            Some(RECIPIENT_ALEX),
            now - Duration::minutes(5 + 12 * i64::from(i)),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

async fn insert_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    kind: &str,
    cat: &str,
    desc: &str,
    amt: i64,
    rcp: Option<Uuid>,
    at: OffsetDateTime,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO transactions (id, account_id, kind, category, description, amount_minor, currency, recipient_id, occurred_at) \
         VALUES ($1, $2, $3, $4, $5, $6, 'PHP', $7, $8)",
    )
    .bind(Uuid::now_v7()).bind(ACCOUNT).bind(kind).bind(cat).bind(desc).bind(amt).bind(rcp).bind(at)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::from)?;
    Ok(())
}
