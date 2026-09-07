//! AMP REST client — the full matchmaker API surface.

use crate::crypto;
use crate::ws::AmpWebSocket;
use crate::{AmpError, Player, Result};
use alloy_signer::Signer;
use alloy_signer_local::PrivateKeySigner;
use serde_json::{json, Value};
use std::sync::Arc;

/// The main AMP client. Clone-safe; share across tasks.
#[derive(Clone)]
pub struct AmpClient {
    http: reqwest::Client,
    pub(crate) server: String,
    signer: Option<Arc<PrivateKeySigner>>,
    token: Arc<tokio::sync::RwLock<Option<String>>>,
    wallet: Arc<tokio::sync::RwLock<Option<String>>>,
}

impl AmpClient {
    /// Create a client with a self-custody signer (private key, 0x-hex).
    pub fn with_signer(server: &str, private_key: &str) -> Result<Self> {
        let signer = private_key.parse::<PrivateKeySigner>()
            .map_err(|e| AmpError::Crypto(format!("bad private key: {e}")))?;
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent("amp-sdk-rust/0.1")
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()?,
            server: server.trim_end_matches('/').to_string(),
            signer: Some(Arc::new(signer)),
            token: Arc::new(tokio::sync::RwLock::new(None)),
            wallet: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    /// Create a client without a signer (public reads only).
    pub fn anonymous(server: &str) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent("amp-sdk-rust/0.1")
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()?,
            server: server.trim_end_matches('/').to_string(),
            signer: None,
            token: Arc::new(tokio::sync::RwLock::new(None)),
            wallet: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    fn signer(&self) -> Result<&PrivateKeySigner> {
        self.signer.as_deref().ok_or_else(|| {
            AmpError::Other("no signer configured — use with_signer()".into())
        })
    }

    async fn wallet(&self) -> Result<String> {
        self.wallet
            .read()
            .await
            .clone()
            .ok_or(AmpError::NotAuthenticated)
    }

    // ── Internal request helpers ──────────────────────────────

    async fn request(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self.http.request(method, format!("{}{}", self.server, path));
        if let Some(token) = self.token.read().await.as_ref() {
            req = req.bearer_auth(token);
        }
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        if !status.is_success() {
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .or_else(|| v.get("message"))
                        .and_then(|e| e.as_str().map(String::from))
                })
                .unwrap_or_else(|| format!("HTTP {status} at {path}"));
            return Err(AmpError::Http { status: status.as_u16(), message });
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    async fn get(&self, path: &str) -> Result<Value> {
        self.request(reqwest::Method::GET, path, None).await
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        self.request(reqwest::Method::POST, path, Some(body)).await
    }

    // ── Auth ──────────────────────────────────────────────────

    /// Gasless wallet login: challenge → EIP-191 sign → verify.
    pub async fn login(&self) -> Result<Player> {
        let signer = self.signer()?;
        let wallet = signer.address().to_checksum(None);

        let challenge = self
            .post("/v1/auth/challenge", json!({ "wallet": wallet }))
            .await?;
        let challenge = challenge["challenge"]
            .as_str()
            .ok_or_else(|| AmpError::Other("server returned no challenge".into()))?
            .to_string();

        let signature = format!(
            "0x{}",
            hex::encode(
                signer
                    .sign_message(challenge.as_bytes())
                    .await
                    .map_err(|e| AmpError::Crypto(format!("signing failed: {e}")))?
                    .as_bytes(),
            )
        );

        let verify = self
            .post(
                "/v1/auth/verify",
                json!({ "wallet": wallet, "signature": signature, "challenge": challenge }),
            )
            .await?;
        let token = verify["token"]
            .as_str()
            .ok_or_else(|| AmpError::Other("server returned no token".into()))?
            .to_string();

        *self.token.write().await = Some(token);
        *self.wallet.write().await = Some(wallet.clone());

        Ok(Player { wallet, region: "na".into(), language: "en".into() })
    }

    /// Log out and clear the session.
    pub async fn logout(&self) {
        *self.token.write().await = None;
    }

    pub async fn authenticated(&self) -> bool {
        self.token.read().await.is_some()
    }

    pub async fn wallet_address(&self) -> Option<String> {
        self.wallet.read().await.clone()
    }

    // ── Player ────────────────────────────────────────────────

    pub async fn me(&self) -> Result<Value> {
        self.get("/v1/me").await
    }

    pub async fn get_player(&self, wallet: &str) -> Result<Value> {
        self.get(&format!("/v1/players/{wallet}")).await
    }

    // ── Games ─────────────────────────────────────────────────

    pub async fn games(&self) -> Result<Value> {
        self.get("/v1/games").await
    }

    // ── Queue ─────────────────────────────────────────────────

    pub async fn join_queue(&self, game_id: &str, ruleset_id: &str) -> Result<Value> {
        self.post("/v1/queue/join", json!({ "gameId": game_id, "rulesetId": ruleset_id }))
            .await
    }

    pub async fn leave_queue(&self) -> Result<Value> {
        self.post("/v1/queue/leave", json!({})).await
    }

    pub async fn queue_status(&self) -> Result<Value> {
        self.get("/v1/queue/status").await
    }

    pub async fn play_bot(&self) -> Result<Value> {
        self.post("/v1/queue/play-bot", json!({})).await
    }

    // ── Matches (1v1) ─────────────────────────────────────────

    pub async fn get_match(&self, match_id: &str) -> Result<Value> {
        self.get(&format!("/v1/matches/{match_id}")).await
    }

    pub async fn match_history(&self, limit: u32, offset: u32) -> Result<Value> {
        self.get(&format!("/v1/matches/history?limit={limit}&offset={offset}"))
            .await
    }

    /// Report a 1v1 result (auto-signs EIP-191 when a signer is present).
    pub async fn report_match(&self, match_id: &str, result: &str) -> Result<Value> {
        let signature = if let Some(signer) = &self.signer {
            let msg = crypto::build_report_message(match_id, result);
            Some(format!(
                "0x{}",
                hex::encode(
                    signer.sign_message(msg.as_bytes()).await
                        .map_err(|e| AmpError::Crypto(format!("signing failed: {e}")))?
                        .as_bytes(),
                )
            ))
        } else {
            None
        };

        self.post(
            &format!("/v1/matches/{match_id}/report"),
            json!({ "result": result, "signature": signature }),
        )
        .await
    }

    // ── Parties ───────────────────────────────────────────────

    pub async fn create_party(&self, game_id: &str, ruleset_id: &str) -> Result<Value> {
        self.post("/v1/parties", json!({ "game_id": game_id, "ruleset_id": ruleset_id }))
            .await
    }

    pub async fn join_party(&self, invite_code: &str) -> Result<Value> {
        self.post(
            "/v1/parties/join",
            json!({ "invite_code": invite_code.to_uppercase() }),
        )
        .await
    }

    pub async fn get_party(&self, party_id: &str) -> Result<Value> {
        self.get(&format!("/v1/parties/{party_id}")).await
    }

    pub async fn lock_party(&self, party_id: &str) -> Result<Value> {
        self.post(&format!("/v1/parties/{party_id}/lock"), json!({})).await
    }

    pub async fn disband_party(&self, party_id: &str) -> Result<Value> {
        self.post(&format!("/v1/parties/{party_id}/disband"), json!({})).await
    }

    // ── Multiplayer (N-player) ────────────────────────────────

    /// Commit to a staked FFA queue. Generates the salt internally and
    /// returns `(response, salt)` — keep the salt for the reveal.
    pub async fn multi_commit(
        &self,
        game_id: &str,
        stake_wei: u64,
        lobby_size: u32,
    ) -> Result<(Value, String)> {
        let wallet = self.wallet().await?;
        let salt = crypto::generate_salt();
        let commit_hash = crypto::compute_commit_hash(&wallet, stake_wei as u128, &salt)?;

        // stakeWei/lobbySize must be JSON numbers (serde i64/usize)
        let resp = self
            .post(
                "/v1/multi/commit",
                json!({
                    "gameId": game_id,
                    "commitHash": commit_hash,
                    "stakeWei": stake_wei,
                    "lobbySize": lobby_size,
                }),
            )
            .await?;
        Ok((resp, salt))
    }

    pub async fn multi_reveal(
        &self,
        game_id: &str,
        ruleset_id: &str,
        salt: &str,
    ) -> Result<Value> {
        self.post(
            "/v1/multi/reveal",
            json!({ "gameId": game_id, "rulesetId": ruleset_id, "salt": salt }),
        )
        .await
    }

    pub async fn get_multi_match(&self, match_id: &str) -> Result<Value> {
        self.get(&format!("/v1/multi/{match_id}")).await
    }

    /// Report an N-player ladder (auto-signs EIP-712 when a signer is present).
    pub async fn multi_report(
        &self,
        match_id: &str,
        ranked: &[(String, u32)],
        transcript_hash: &str,
        session_nonce: u64,
        chain_id: u64,
        contract_address: &str,
    ) -> Result<Value> {
        let signature = if let Some(signer) = &self.signer {
            let placements: Vec<String> = ranked.iter().map(|(a, _)| a.clone()).collect();
            let typed = crypto::build_ladder_typed_data(
                chain_id,
                contract_address,
                match_id,
                &placements,
                transcript_hash,
                session_nonce,
            )?;
            let sig = signer
                .sign_dynamic_typed_data(&typed)
                .await
                .map_err(|e| AmpError::Crypto(format!("EIP-712 signing failed: {e}")))?;
            Some(format!("0x{}", hex::encode(sig.as_bytes())))
        } else {
            None
        };

        let ranked_json: Vec<Value> = ranked
            .iter()
            .map(|(addr, place)| json!([addr, place]))
            .collect();

        self.post(
            &format!("/v1/multi/{match_id}/report"),
            json!({
                "ranked": ranked_json,
                "transcriptHash": transcript_hash,
                "sessionNonce": session_nonce,
                "signature": signature,
            }),
        )
        .await
    }

    pub async fn multi_claim(&self, match_id: &str) -> Result<Value> {
        self.post(&format!("/v1/multi/{match_id}/claim"), json!({})).await
    }

    // ── Exit certificates (multiplayer death certs) ────────────

    /// Submit an exit certificate: an eliminated player signs their rank,
    /// exit frame, and state hash, then disconnects. Auto-signs EIP-191.
    pub async fn submit_exit_cert(
        &self,
        match_id: &str,
        rank: u32,
        exit_frame: u64,
        state_hash: &str,
    ) -> Result<Value> {
        let signature = if let Some(signer) = &self.signer {
            let msg = crypto::build_exit_cert_message(match_id, rank, exit_frame, state_hash);
            Some(format!(
                "0x{}",
                hex::encode(
                    signer.sign_message(msg.as_bytes()).await
                        .map_err(|e| AmpError::Crypto(format!("signing failed: {e}")))?
                        .as_bytes(),
                )
            ))
        } else {
            None
        };

        self.post(
            &format!("/v1/multi/{match_id}/exit"),
            json!({ "rank": rank, "exitFrame": exit_frame, "stateHash": state_hash, "signature": signature }),
        )
        .await
    }

    /// A surviving player countersigns an exit certificate, verifying
    /// the eliminated player's state hash against their own simulation.
    pub async fn countersign_exit_cert(
        &self,
        match_id: &str,
        wallet: &str,
        state_hash: &str,
    ) -> Result<Value> {
        self.post(
            &format!("/v1/multi/{match_id}/exit/{wallet}"),
            json!({ "stateHash": state_hash }),
        )
        .await
    }

    // ── Staked 1v1 escrow ──────────────────────────────────────

    /// Verify on-chain escrow for a staked 1v1 match (participant only).
    /// Flips an escrow_pending match to live once both deposits check out.
    pub async fn verify_escrow(&self, match_id: &str) -> Result<Value> {
        self.post(&format!("/v1/matches/{match_id}/escrow/verify"), json!({}))
            .await
    }

    // ── Convenience ────────────────────────────────────────────

    /// One call: queue → wait → matchId. Polls `me().liveMatchId`
    /// every 2 s. Returns `Err(Other("timeout …"))` on expiry.
    pub async fn wait_for_match(&self, timeout: std::time::Duration) -> Result<String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Some(id) = self
                .me()
                .await?
                .get("liveMatchId")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                return Ok(id.to_string());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(AmpError::Other(format!(
                    "wait_for_match timed out after {:?}",
                    timeout
                )));
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    // ── Events (WebSocket) ────────────────────────────────────

    /// Connect to the live event stream (requires login first).
    pub async fn events(&self) -> Result<AmpWebSocket> {
        let token = self
            .token
            .read()
            .await
            .clone()
            .ok_or(AmpError::NotAuthenticated)?;
        AmpWebSocket::connect(&self.server, &token).await
    }
}
