//! Deterministic policy. No I/O, no models: given facts in, decision out.
//!
//! * [`hard`] — invariants checked **before** any AI is called, and again at
//!   execution time inside the DB transaction.
//! * [`thresholds`] — per-risk-class confidence thresholds (demo values).
//! * [`decide`] — the final workflow decision, split into small testable rules.

pub mod decide;
pub mod hard;
pub mod thresholds;

pub use decide::*;
pub use hard::*;
pub use thresholds::*;
