//! AMP Rust SDK — Avalanche Tournament Protocol example.
//!
//! Run with:
//! ```sh
//! cargo run --example tournament
//! ```

use amp_sdk::tournament::TournamentClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== AMP Avalanche Tournament Protocol Client ===");

    // Initialize client targeting Avalanche Fuji testnet
    let client = TournamentClient::for_fuji();
    println!("Connected to Fuji RPC: {}", client.rpc_url);
    println!("Contract Address: {}", client.contract_address);

    // 1. Fetch nextTournamentId
    let next_id = client.get_next_tournament_id().await?;
    println!("\nCurrent nextTournamentId: {next_id}");

    // 2. Fetch Tournament #1
    let tournament_id = 1;
    let t = client.get_tournament(tournament_id).await?;
    println!("\nTournament #{}:", t.id);
    println!("  Sponsor:          {}", t.sponsor);
    println!("  State:            {}", t.state);
    println!("  Prize Pool:       {:.4} AVAX ({} wei)", t.prize_pool_avax(), t.prize_pool);
    println!("  Payout Splits:    {:?} (bps)", t.payout_bps);
    println!("  Verifier:         {}", t.verifier);
    println!("  Attested Winners: {:?}", t.winners);

    // 3. Check claim eligibility for winners
    for (i, winner) in t.winners.iter().enumerate() {
        let is_claimed = client.is_prize_claimed(tournament_id, i as u32).await?;
        let prize = t.get_placement_prize_avax(i);
        println!(
            "  Winner #{} ({}): {:.4} AVAX - Claimed: {}",
            i + 1,
            winner,
            prize,
            is_claimed
        );

        let check = client.can_claim_prize(tournament_id, winner).await?;
        println!("    Eligibility check: can_claim={}, reason={}", check.can_claim, check.reason);
    }

    // 4. Compute EIP-712 Attestation digest
    let digest = client.compute_finalize_digest_hex(tournament_id, &t.winners)?;
    println!("\nTournament #{} EIP-712 Attestation Digest:\n  {}", tournament_id, digest);

    // 5. ABI Calldata Encoders
    println!("\nGenerated Calldata:");
    println!("  getTournament(1): {}", TournamentClient::encode_get_tournament(1));
    println!("  claimed(1, 0):     {}", TournamentClient::encode_claimed(1, 0));
    println!("  claimPrize(1, 0):  {}", TournamentClient::encode_claim_prize(1, 0));

    Ok(())
}
