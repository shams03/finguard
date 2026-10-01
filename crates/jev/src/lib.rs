//! Jev adapter. Vendor JSON stops at this crate's boundary: everything above
//! it sees only `finguard_domain::RiskAssessment`.
//!
//! * `questions` — the versioned atomic question set (`risk_questions_v1`)
//! * `parse`     — vendor response -> domain assessment (pure, unit tested)
//! * `client`    — `JevDecisionProvider` (reqwest, explicit timeouts, bounded retries)
//! * `mock`      — `MockDecisionProvider` for tests and offline demos

pub mod client;
pub mod mock;
pub mod parse;
pub mod questions;

pub use client::{JevConfig, JevDecisionProvider};
pub use mock::{MockDecisionProvider, MockFailure};
