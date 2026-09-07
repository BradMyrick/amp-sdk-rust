//! AMP crypto helpers — commit hashes, salts, report messages, EIP-712.
//!
//! Uses `alloy-primitives` keccak256 and `alloy-dyn-abi` typed data,
//! matching the TS/C#/C++ SDKs byte-for-byte.

use crate::{AmpError, Result};
use alloy_primitives::{keccak256, Address, U256};
use rand::RngCore;

/// keccak256 as 0x-hex.
pub fn keccak_hex(data: &[u8]) -> String {
    format!("0x{}", hex::encode(keccak256(data).as_slice()))
}

/// Generate a random 32-byte salt as 0x-hex.
pub fn generate_salt() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut buf);
    format!("0x{}", hex::encode(buf))
}

/// EIP-191 report message: `AMP_REPORT:v1:{matchId}:{result}`
pub fn build_report_message(match_id: &str, result: &str) -> String {
    format!("AMP_REPORT:v1:{match_id}:{result}")
}

/// Commit-reveal hash: keccak256(pad32(address) ‖ pad32(stake) ‖ salt-bytes).
pub fn compute_commit_hash(wallet: &str, stake_wei: u128, salt: &str) -> Result<String> {
    let addr: Address = wallet
        .parse()
        .map_err(|e| AmpError::Crypto(format!("bad wallet address: {e}")))?;
    let stake = U256::from(stake_wei);
    let salt_bytes = hex::decode(salt.trim_start_matches("0x"))
        .map_err(|e| AmpError::Crypto(format!("bad salt hex: {e}")))?;

    let mut buf = Vec::with_capacity(32 + 32 + salt_bytes.len());
    buf.extend_from_slice(&[0u8; 12]);
    buf.extend_from_slice(addr.as_slice());
    buf.extend_from_slice(&stake.to_be_bytes::<32>());
    buf.extend_from_slice(&salt_bytes);
    Ok(keccak_hex(&buf))
}

/// Build the EIP-712 `MultiplayerLadder` typed data for multiReport.
#[allow(clippy::too_many_arguments)]
pub fn build_ladder_typed_data(
    chain_id: u64,
    contract_address: &str,
    match_id: &str,
    ranked_placements: &[String],
    transcript_hash: &str,
    session_nonce: u64,
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
        "MultiplayerLadder".into(),
        serde_json::json!([
            { "name": "matchId", "type": "bytes32" },
            { "name": "gameId", "type": "uint256" },
            { "name": "rankedPlacements", "type": "address[]" },
            { "name": "transcriptHash", "type": "bytes32" },
            { "name": "sessionNonce", "type": "uint256" },
        ]),
    );

    let domain = serde_json::json!({
        "name": "AMPMultiplayer",
        "version": "1",
        "chainId": chain_id.to_string(),
        "verifyingContract": contract.to_checksum(None),
    });

    let message = serde_json::json!({
        "matchId": match_id,
        "gameId": "1",
        "rankedPlacements": ranked_placements,
        "transcriptHash": transcript_hash,
        "sessionNonce": session_nonce.to_string(),
    });

    let raw = serde_json::json!({
        "types": types,
        "primaryType": "MultiplayerLadder",
        "domain": domain,
        "message": message,
    });

    serde_json::from_value(raw)
        .map_err(|e| AmpError::Crypto(format!("typed data construction failed: {e}")))
}
