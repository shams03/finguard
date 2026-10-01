//! HTTP layer. Thin by design: parse -> call `Agent` -> shape the response.
//! No business decisions are made here.

#![allow(clippy::type_complexity)]

pub mod config;
pub mod error;
pub mod logging;
pub mod routes;
pub mod state;

pub use routes::router;
pub use state::{AppState, build_state};
