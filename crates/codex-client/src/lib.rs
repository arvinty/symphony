//! Codex `app-server` JSON-RPC v2 client.
//!
//! Layered modules: `protocol` (types) → `transport` (framed I/O) →
//! `dispatcher` (correlation) → `client` (typed API).

pub mod client;
pub mod dispatcher;
pub mod error;
pub mod protocol;
pub mod transport;

pub use client::{Client, NotificationStream};
pub use error::{ClientError, ClientResult};
