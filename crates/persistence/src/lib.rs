//! PostgreSQL persistence: schema access, the transactional executor, audit
//! log, confirmations, idempotency and demo seeding. Plain SQL via `sqlx`; no
//! vendor-specific features, so Docker Postgres and Supabase behave the same.

// Row tuples with `query_as` are idiomatic here; naming each one adds noise.
#![allow(clippy::type_complexity)]

pub mod actions;
pub mod audit;
pub mod confirmations;
pub mod db;
pub mod evals;
pub mod executor;
pub mod facts;
pub mod idempotency;
pub mod reads;
pub mod seed;
pub mod trace;

pub use db::{DbConfig, Store};
pub use sqlx::PgPool;

use finguard_domain::ErrorCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("illegal state change: {0}")]
    IllegalState(String),
    #[error("{code:?}: {message}")]
    Domain { code: ErrorCode, message: String },
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("invalid stored data: {0}")]
    Corrupt(String),
}

impl StoreError {
    pub fn domain(code: ErrorCode, message: impl Into<String>) -> Self {
        StoreError::Domain {
            code,
            message: message.into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, StoreError>;
