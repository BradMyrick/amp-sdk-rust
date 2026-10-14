//! Avalanche Tournament & Match Protocol client.
//!
//! Interacts with the `AMPTournamentCup` smart contract on Avalanche C-Chain / Fuji testnet.
//! Provides tournament state inspection, winner prize claim eligibility checking,
//! ABI calldata encoding/decoding, and EIP-712 attestation digest computation.

use crate::{AmpError, Result};
use alloy_primitives::{Address, U256};
use serde_json::Value;
use std::fmt;

/// Deployed AMPTournamentCup address on Avalanche Fuji testnet.
pub const DEFAULT_CUP_ADDRESS: &str = "0x7C743c1C9AE3e7A65d030098F2249B7787d66dfF";

/// Avalanche Fuji C-Chain ID.
pub const DEFAULT_FUJI_CHAIN_ID: u64 = 43113;

/// Public Avalanche Fuji RPC endpoint.
pub const DEFAULT_FUJI_RPC: &str = "https://api.avax-test.network/ext/bc/C/rpc";

/// Function selectors on AMPTournamentCup contract.
pub const SIG_GET_TOURNAMENT: &str = "0x1a5bd7fc";
pub const SIG_CLAIMED: &str = "0xa54ab457";
pub const SIG_CLAIM_PRIZE: &str = "0x7b154140";
pub const SIG_NEXT_TOURNAMENT_ID: &str = "0x852efc3d";

/// EIP-712 domain name and version for AMPTournamentCup.
pub const EIP712_DOMAIN_NAME: &str = "AMPTournamentCup";
pub const EIP712_DOMAIN_VERSION: &str = "1";

/// On-chain tournament lifecycle state enum (mirrors AMPTournamentCup.sol).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum TournamentState {
    /// Funded, awaiting verifier finalization.
    Open = 0,
    /// Winners attested via EIP-712; claims open.
    Finalized = 1,
    /// All winners have claimed their prizes.
    Complete = 2,
    /// Sponsor refunded before finalization or after deadline.
    Cancelled = 3,
}

impl fmt::Display for TournamentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => write!(f, "Open"),
            Self::Finalized => write!(f, "Finalized"),
            Self::Complete => write!(f, "Complete"),
            Self::Cancelled => write!(f, "Cancelled"),
        }
    }
}

impl TryFrom<u8> for TournamentState {
    type Error = AmpError;

    fn try_from(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::Open),
            1 => Ok(Self::Finalized),
            2 => Ok(Self::Complete),
            3 => Ok(Self::Cancelled),
            other => Err(AmpError::Other(format!("unknown tournament state: {other}"))),
        }
    }
}

/// On-chain tournament data stored in AMPTournamentCup.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TournamentData {
    /// Tournament ID.
    pub id: u64,
    /// Sponsor wallet address that funded the prize pool.
    pub sponsor: String,
    /// Token address (0x0 for native AVAX).
    pub token: String,
    /// Total prize pool in wei (atomic units).
    pub prize_pool: U256,
    /// Per-placement payout basis points (index 0 = 1st place, sum = 10000).
    pub payout_bps: Vec<u32>,
    /// Verifier address authorized to attest winners.
    pub verifier: String,
    /// Attested winners list (populated once finalized).
    pub winners: Vec<String>,
    /// Lifecycle state of the tournament.
    pub state: TournamentState,
    /// Unix timestamp when created.
    pub created_at: u64,
    /// Unix timestamp deadline for finalization.
    pub finalize_deadline: u64,
}

impl TournamentData {
    /// Returns true if the tournament prize pool is in native AVAX.
    pub fn is_native_avax(&self) -> bool {
        self.token.is_empty()
            || self.token == "0x0000000000000000000000000000000000000000"
            || self
                .token
                .eq_ignore_ascii_case("0x0000000000000000000000000000000000000000")
    }

    /// Prize pool amount in decimal AVAX (1e18 wei = 1 AVAX).
    pub fn prize_pool_avax(&self) -> f64 {
        u256_to_avax(self.prize_pool)
    }

    /// Calculate the prize amount in wei for a 0-indexed placement.
    pub fn get_placement_prize(&self, placement: usize) -> U256 {
        if placement >= self.payout_bps.len() {
            return U256::ZERO;
        }
        (self.prize_pool * U256::from(self.payout_bps[placement])) / U256::from(10000)
    }

    /// Calculate the prize amount in decimal AVAX for a 0-indexed placement.
    pub fn get_placement_prize_avax(&self, placement: usize) -> f64 {
        u256_to_avax(self.get_placement_prize(placement))
    }
}

/// Result of evaluating whether a player won and can claim a prize.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CanClaimResult {
    /// True if the player is eligible to call `claimPrize` now.
    pub can_claim: bool,
    /// True if the player's wallet address is in the attested winners list.
    pub is_winner: bool,
    /// 0-indexed placement (0 = 1st place), or None if not an attested winner.
    pub placement: Option<u32>,
    /// Estimated prize amount in wei.
    pub estimated_prize: U256,
    /// True if the prize for this placement has already been claimed.
    pub already_claimed: bool,
    /// Explanation of the claim eligibility status.
    pub reason: String,
    /// Snapshot of the tournament data queried from the contract.
    pub tournament: TournamentData,
}

impl CanClaimResult {
    /// Estimated prize in decimal AVAX.
    pub fn estimated_prize_avax(&self) -> f64 {
        u256_to_avax(self.estimated_prize)
    }

    /// 0-indexed placement as integer, or -1 if not a winner.
    pub fn placement_index(&self) -> i32 {
        self.placement.map(|p| p as i32).unwrap_or(-1)
    }
}

/// Helper converting U256 wei to f64 AVAX.
fn u256_to_avax(wei: U256) -> f64 {
    let s = wei.to_string();
    s.parse::<f64>().unwrap_or(0.0) / 1e18
}

/// Client for interacting with the Avalanche Tournament Protocol (`AMPTournamentCup`).
#[derive(Clone, Debug)]
pub struct TournamentClient {
    http: reqwest::Client,
    pub rpc_url: String,
    pub contract_address: String,
    pub chain_id: u64,
}

impl TournamentClient {
    /// Create a new TournamentClient with custom RPC and contract address on Fuji (chain ID 43113).
    pub fn new(rpc_url: &str, contract_address: &str) -> Self {
        Self::with_chain_id(rpc_url, contract_address, DEFAULT_FUJI_CHAIN_ID)
    }

    /// Create a client targeting the official AMPTournamentCup deployed on Avalanche Fuji testnet.
    pub fn for_fuji() -> Self {
        Self::with_chain_id(DEFAULT_FUJI_RPC, DEFAULT_CUP_ADDRESS, DEFAULT_FUJI_CHAIN_ID)
    }

    /// Create a TournamentClient with explicit chain ID.
    pub fn with_chain_id(rpc_url: &str, contract_address: &str, chain_id: u64) -> Self {
        let http = reqwest::Client::builder()
            .user_agent("amp-sdk-rust/0.1")
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let clean_contract = contract_address
            .parse::<Address>()
            .map(|a| a.to_checksum(None))
            .unwrap_or_else(|_| contract_address.to_string());

        Self {
            http,
            rpc_url: rpc_url.trim_end_matches('/').to_string(),
            contract_address: clean_contract,
            chain_id,
        }
    }

    /// Fetch full tournament details by ID from the contract.
    pub async fn get_tournament(&self, tournament_id: u64) -> Result<TournamentData> {
        let calldata = Self::encode_get_tournament(tournament_id);
        let raw_hex = self.eth_call(&self.contract_address, &calldata).await?;
        Self::decode_tournament(tournament_id, &raw_hex)
    }

    /// Check if a specific placement's prize has already been claimed.
    pub async fn is_prize_claimed(&self, tournament_id: u64, placement: u32) -> Result<bool> {
        let calldata = Self::encode_claimed(tournament_id, placement);
        let raw_hex = self.eth_call(&self.contract_address, &calldata).await?;
        Self::decode_bool(&raw_hex)
    }

    /// Query the `nextTournamentId` from the contract.
    pub async fn get_next_tournament_id(&self) -> Result<u64> {
        let calldata = Self::encode_next_tournament_id();
        let raw_hex = self.eth_call(&self.contract_address, &calldata).await?;
        let clean = raw_hex.trim_start_matches("0x");
        u64::from_str_radix(clean, 16)
            .map_err(|e| AmpError::Other(format!("invalid nextTournamentId hex: {e}")))
    }

    /// Check whether a player wallet is an attested winner, their placement, and whether they can claim now.
    pub async fn can_claim_prize(&self, tournament_id: u64, wallet: &str) -> Result<CanClaimResult> {
        let wallet_addr: Address = wallet
            .parse()
            .map_err(|e| AmpError::Crypto(format!("bad wallet address: {e}")))?;
        let wallet_norm = wallet_addr.to_checksum(None);

        let tournament = self.get_tournament(tournament_id).await?;

        let mut placement = None;
        for (i, w) in tournament.winners.iter().enumerate() {
            if let Ok(w_addr) = w.parse::<Address>() {
                if w_addr == wallet_addr {
                    placement = Some(i as u32);
                    break;
                }
            } else if w.eq_ignore_ascii_case(wallet) {
                placement = Some(i as u32);
                break;
            }
        }

        let Some(p) = placement else {
            return Ok(CanClaimResult {
                can_claim: false,
                is_winner: false,
                placement: None,
                estimated_prize: U256::ZERO,
                already_claimed: false,
                reason: format!(
                    "Address {wallet_norm} is not an attested winner in tournament #{tournament_id}"
                ),
                tournament,
            });
        };

        let is_claimed = self.is_prize_claimed(tournament_id, p).await?;
        let estimated_prize = tournament.get_placement_prize(p as usize);

        if is_claimed {
            return Ok(CanClaimResult {
                can_claim: false,
                is_winner: true,
                placement: Some(p),
                estimated_prize,
                already_claimed: true,
                reason: format!("Prize for placement #{} has already been claimed", p + 1),
                tournament,
            });
        }

        if tournament.state != TournamentState::Finalized
            && tournament.state != TournamentState::Complete
        {
            return Ok(CanClaimResult {
                can_claim: false,
                is_winner: true,
                placement: Some(p),
                estimated_prize,
                already_claimed: false,
                reason: format!(
                    "Tournament state is {}; claims only open when Finalized",
                    tournament.state
                ),
                tournament,
            });
        }

        Ok(CanClaimResult {
            can_claim: true,
            is_winner: true,
            placement: Some(p),
            estimated_prize,
            already_claimed: false,
            reason: "Eligible to claim".to_string(),
            tournament,
        })
    }

    /// Compute 32-byte EIP-712 digest for a `TournamentResult` attestation.
    pub fn compute_finalize_digest(
        &self,
        tournament_id: u64,
        winners: &[String],
    ) -> Result<[u8; 32]> {
        compute_finalize_digest(self.chain_id, &self.contract_address, tournament_id, winners)
    }

    /// Compute 0x-prefixed hex string of the EIP-712 digest for a `TournamentResult` attestation.
    pub fn compute_finalize_digest_hex(
        &self,
        tournament_id: u64,
        winners: &[String],
    ) -> Result<String> {
        let digest = self.compute_finalize_digest(tournament_id, winners)?;
        Ok(format!("0x{}", hex::encode(digest)))
    }

    /// Low-level JSON-RPC `eth_call` query.
    pub async fn eth_call(&self, to: &str, data: &str) -> Result<String> {
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [
                {
                    "to": to,
                    "data": data,
                },
                "latest"
            ]
        });

        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&payload)
            .send()
            .await
            .map_err(AmpError::Network)?;

        let val: Value = resp.json().await.map_err(AmpError::Network)?;
        if let Some(err) = val.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("RPC error");
            return Err(AmpError::Other(format!("RPC error: {msg}")));
        }

        let hex_str = val
            .get("result")
            .and_then(|r| r.as_str())
            .ok_or_else(|| AmpError::Other("missing result in RPC response".into()))?;

        Ok(hex_str.to_string())
    }

    // ── ABI Encoders ───────────────────────────────────────────

    /// Encode `getTournament(uint256)` calldata.
    pub fn encode_get_tournament(tournament_id: u64) -> String {
        format!("{SIG_GET_TOURNAMENT}{:064x}", tournament_id)
    }

    /// Encode `claimed(uint256,uint256)` calldata.
    pub fn encode_claimed(tournament_id: u64, placement: u32) -> String {
        format!("{SIG_CLAIMED}{:064x}{:064x}", tournament_id, placement)
    }

    /// Encode `claimPrize(uint256,uint256)` calldata.
    pub fn encode_claim_prize(tournament_id: u64, placement: u32) -> String {
        format!("{SIG_CLAIM_PRIZE}{:064x}{:064x}", tournament_id, placement)
    }

    /// Encode `nextTournamentId()` calldata.
    pub fn encode_next_tournament_id() -> String {
        SIG_NEXT_TOURNAMENT_ID.to_string()
    }

    // ── ABI Decoders ───────────────────────────────────────────

    /// Decode bool return data from `claimed(...)`.
    pub fn decode_bool(hex_data: &str) -> Result<bool> {
        let clean = hex_data.trim_start_matches("0x");
        if clean.is_empty() {
            return Ok(false);
        }
        let bytes = hex::decode(clean)
            .map_err(|e| AmpError::Crypto(format!("invalid hex in decode_bool: {e}")))?;
        Ok(bytes.iter().any(|&b| b != 0))
    }

    /// Decode `getTournament(uint256)` return data into `TournamentData`.
    pub fn decode_tournament(tournament_id: u64, hex_data: &str) -> Result<TournamentData> {
        let clean = hex_data.trim_start_matches("0x");
        let raw = hex::decode(clean)
            .map_err(|e| AmpError::Crypto(format!("invalid hex for tournament data: {e}")))?;

        if raw.len() < 32 * 9 {
            return Err(AmpError::Other(
                "invalid getTournament return data: payload too short".into(),
            ));
        }

        let mut struct_start = read_u64_from_word(&raw, 0) as usize;
        if struct_start + 32 * 9 > raw.len() {
            struct_start = 32;
        }
        if struct_start + 32 * 9 > raw.len() {
            return Err(AmpError::Other(
                "invalid getTournament return data: struct offset out of bounds".into(),
            ));
        }

        let sponsor = read_address(&raw, struct_start)?;
        let token = read_address(&raw, struct_start + 32)?;
        let prize_pool = U256::from_be_slice(&raw[struct_start + 64..struct_start + 96]);

        let payout_offset = struct_start + read_u64_from_word(&raw, struct_start + 96) as usize;
        let verifier = read_address(&raw, struct_start + 128)?;
        let winners_offset = struct_start + read_u64_from_word(&raw, struct_start + 160) as usize;
        let state_byte = raw[struct_start + 192 + 31];
        let created_at = read_u64_from_word(&raw, struct_start + 224);
        let finalize_deadline = read_u64_from_word(&raw, struct_start + 256);

        // Read payout_bps array
        let mut payout_bps = Vec::new();
        if payout_offset + 32 <= raw.len() {
            let payout_len = read_u64_from_word(&raw, payout_offset) as usize;
            for i in 0..payout_len {
                let elem_offset = payout_offset + 32 + i * 32;
                if elem_offset + 32 <= raw.len() {
                    let val = read_u64_from_word(&raw, elem_offset) as u32;
                    payout_bps.push(val);
                }
            }
        }

        // Read winners array
        let mut winners = Vec::new();
        if winners_offset + 32 <= raw.len() {
            let winners_len = read_u64_from_word(&raw, winners_offset) as usize;
            for i in 0..winners_len {
                let elem_offset = winners_offset + 32 + i * 32;
                if elem_offset + 32 <= raw.len() {
                    let addr = read_address(&raw, elem_offset)?;
                    winners.push(addr);
                }
            }
        }

        let state = TournamentState::try_from(state_byte)?;

        Ok(TournamentData {
            id: tournament_id,
            sponsor,
            token,
            prize_pool,
            payout_bps,
            verifier,
            winners,
            state,
            created_at,
            finalize_deadline,
        })
    }
}

// ── Helpers ────────────────────────────────────────────────────

fn read_u64_from_word(data: &[u8], offset: usize) -> u64 {
    if offset + 32 > data.len() {
        return 0;
    }
    let slice = &data[offset + 24..offset + 32];
    u64::from_be_bytes(slice.try_into().unwrap_or([0u8; 8]))
}

fn read_address(data: &[u8], offset: usize) -> Result<String> {
    if offset + 32 > data.len() {
        return Err(AmpError::Other("slice too short for address".into()));
    }
    let addr = Address::from_slice(&data[offset + 12..offset + 32]);
    Ok(addr.to_checksum(None))
}

/// Build EIP-712 `TypedData` structure for a `TournamentResult` attestation.
pub fn build_tournament_typed_data(
    chain_id: u64,
    contract_address: &str,
    tournament_id: u64,
    winners: &[String],
) -> Result<alloy_dyn_abi::TypedData> {
    let contract: Address = contract_address
        .parse()
        .map_err(|e| AmpError::Crypto(format!("bad contract address: {e}")))?;

    let mut types = serde_json::Map::new();
    types.insert(
        "EIP712Domain".into(),
        serde_json::json!([
            { "name": "name", "type": "string" },
            { "name": "version", "type": "string" },
            { "name": "chainId", "type": "uint256" },
            { "name": "verifyingContract", "type": "address" },
        ]),
    );
    types.insert(
        "TournamentResult".into(),
        serde_json::json!([
            { "name": "tournamentId", "type": "uint256" },
            { "name": "winners", "type": "address[]" },
        ]),
    );

    let domain = serde_json::json!({
        "name": EIP712_DOMAIN_NAME,
        "version": EIP712_DOMAIN_VERSION,
        "chainId": chain_id.to_string(),
        "verifyingContract": contract.to_checksum(None),
    });

    let checksummed_winners: std::result::Result<Vec<String>, AmpError> = winners
        .iter()
        .map(|w| {
            let addr: Address = w
                .parse()
                .map_err(|e| AmpError::Crypto(format!("bad winner address: {e}")))?;
            Ok(addr.to_checksum(None))
        })
        .collect();

    let message = serde_json::json!({
        "tournamentId": tournament_id.to_string(),
        "winners": checksummed_winners?,
    });

    let raw = serde_json::json!({
        "types": types,
        "primaryType": "TournamentResult",
        "domain": domain,
        "message": message,
    });

    serde_json::from_value(raw)
        .map_err(|e| AmpError::Crypto(format!("typed data construction failed: {e}")))
}

/// Compute 32-byte EIP-712 digest for tournament finalization attestation.
pub fn compute_finalize_digest(
    chain_id: u64,
    contract_address: &str,
    tournament_id: u64,
    winners: &[String],
) -> Result<[u8; 32]> {
    let td = build_tournament_typed_data(chain_id, contract_address, tournament_id, winners)?;
    let hash = td
        .eip712_signing_hash()
        .map_err(|e| AmpError::Crypto(format!("eip712 signing hash failed: {e}")))?;
    Ok(hash.0)
}

/// Compute 0x-prefixed hex string of the EIP-712 digest for tournament finalization attestation.
pub fn compute_finalize_digest_hex(
    chain_id: u64,
    contract_address: &str,
    tournament_id: u64,
    winners: &[String],
) -> Result<String> {
    let digest = compute_finalize_digest(chain_id, contract_address, tournament_id, winners)?;
    Ok(format!("0x{}", hex::encode(digest)))
}
