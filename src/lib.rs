//! AMP SDK for Rust — core types, errors, and crypto helpers.

pub mod client;
pub mod crypto;
pub mod ws;

pub use alloy_signer_local::PrivateKeySigner;
pub use client::AmpClient;
pub use ws::AmpWebSocket;

use thiserror::Error;

/// Errors from the AMP SDK.
#[derive(Error, Debug)]
pub enum AmpError {
    #[error("network: {0}")]
    Network(#[from] reqwest::Error),

    #[error("ws: {0}")]
    Ws(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("http {status}: {message}")]
    Http { status: u16, message: String },

    #[error("crypto: {0}")]
    Crypto(String),

    #[error("not authenticated — call login() first")]
    NotAuthenticated,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, AmpError>;

/// A player identity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Player {
    pub wallet: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub language: String,
}

/// Glicko-2 rating for a game/ruleset.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlayerRating {
    #[serde(default)]
    pub rating: f64,
    #[serde(default)]
    pub deviation: f64,
    #[serde(default)]
    pub wins: u64,
    #[serde(default)]
    pub losses: u64,
    #[serde(default)]
    pub draws: u64,
}

/// WebSocket event names pushed by the matchmaker.
pub mod events {
    pub const HELLO: &str = "hello";
    pub const QUEUE_STATUS: &str = "queue_status";
    pub const MATCH_FOUND: &str = "match_found";
    pub const MATCH_RESULT: &str = "match_result";
    pub const MATCH_UPDATE: &str = "match_update";
    pub const MULTI_LOBBY_FORMED: &str = "multi_lobby_formed";
    pub const MULTI_RESULT: &str = "multi_result";
    pub const MULTI_CANCELLED: &str = "multi_cancelled";
}
