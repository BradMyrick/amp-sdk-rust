//! AMP crypto helpers — commit hashes, salts, report messages, EIP-712.
//!
//! Uses `alloy-primitives` keccak256 and `alloy-dyn-abi` typed data,
//! matching the TS/C#/C++ SDKs byte-for-byte.

use crate::{AmpError, Result};
use alloy_primitives::{keccak256, Address};
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

/// EIP-191 message for a multiplayer exit certificate (death cert).
/// Must match amp-server's `submit_exit_cert` format exactly.
pub fn build_exit_cert_message(
    match_id: &str,
    rank: u32,
    exit_frame: u64,
    state_hash: &str,
) -> String {
    format!(
        "AMP exit certificate\n\n\
         Match: {match_id}\n\
         Rank: {rank}\n\
         Exit frame: {exit_frame}\n\
         State hash: {state_hash}\n\n\
         This signature is free. It certifies your elimination and unlocks your reporting bond."
    )
}

/// Commit-reveal hash: keccak256(addr20 ‖ stake_u64_be(8) ‖ salt-utf8).
/// Matches amp-server's `compute_commit` byte-for-byte — including the
/// detail that the salt is hashed as its UTF-8 string bytes, not decoded.
pub fn compute_commit_hash(wallet: &str, stake_wei: u128, salt: &str) -> Result<String> {
    let addr: Address = wallet
        .parse()
        .map_err(|e| AmpError::Crypto(format!("bad wallet address: {e}")))?;

    let mut buf = Vec::with_capacity(20 + 8 + salt.len());
    buf.extend_from_slice(addr.as_slice());
    buf.extend_from_slice(&(stake_wei as u64).to_be_bytes());
    buf.extend_from_slice(salt.as_bytes());
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
            // Contract typehash (AMPMultiplayer.sol): gameId is bytes32.
            { "name": "gameId", "type": "bytes32" },
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
        "gameId": format!("0x{}1", "0".repeat(63)),
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
