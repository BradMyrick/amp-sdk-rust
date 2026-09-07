//! AMP Rust SDK — N-Player Multiplayer Example
//!
//! The full staked FFA lifecycle: commit-reveal anti-collusion, live play
//! with death certificates, EIP-712 ladder reporting, settlement.
//!
//! ```sh
//! AMP_TEST_KEY=0x... cargo run --example multiplayer
//! ```

use amp_sdk::AmpClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("AMP_TEST_KEY").expect("set AMP_TEST_KEY");
    let amp = AmpClient::with_signer("https://amp.playwithamp.xyz", &key)?;
    amp.login().await?;
    println!("logged in: {}", amp.wallet_address().await.unwrap());

    // ═══ 1. COMMIT — stake into the FFA queue ═════════════════
    // multi_commit generates the salt internally and returns it — KEEP IT.
    // The server only sees keccak256(wallet ‖ stake ‖ salt) until reveal,
    // so neither the server nor other players can front-run your stake.
    let (commit, salt) = amp.multi_commit("amp-tactics", 0, 4).await?;
    println!(
        "committed ({}/4 waiting), salt: {:.<18}…",
        commit["committedCount"], &salt[..16.min(salt.len())]
    );

    // ═══ 2. REVEAL — open your commit ═════════════════════════
    let reveal = amp.multi_reveal("amp-tactics", "ranked-1v1", &salt).await?;
    println!("revealed: {reveal}");

    // ═══ 3. WAIT FOR LOBBY (WebSocket push in production) ═════
    // let mut ws = amp.events().await?;
    // let lobby = tokio::time::timeout(
    //     Duration::from_secs(120),
    //     ws.next_event("multi_lobby_formed"),
    // ).await??.expect("lobby event");
    // let match_id = lobby["data"]["matchId"].as_str().unwrap().to_string();

    // ═══ 4. PLAY — death certs when players are eliminated ════
    // When YOUR player dies, sign and submit, then disconnect:
    //
    //   amp.submit_exit_cert(&match_id, /*rank=*/3, /*exit_frame=*/1200,
    //                        /*state_hash=*/"0x…").await?;
    //
    // Survivors countersign each cert against their own simulation:
    //
    //   amp.countersign_exit_cert(&match_id, eliminated_wallet, "0x…").await?;

    // ═══ 5. REPORT — last survivor submits the ladder ═════════
    // [winner, second, third, …] — best to worst. Auto-signs EIP-712:
    //
    //   amp.multi_report(&match_id,
    //       &[(winner_wallet.into(), 1), (second_wallet.into(), 2)],
    //       /*transcript_hash=*/"0x…", /*session_nonce=*/42,
    //       /*chain_id=*/43113,
    //       /*contract=*/"0x3BBb1812Ccafc4a8c849BfA340174a271e43B7D1").await?;

    // ═══ 6. CLAIM — trigger settlement ════════════════════════
    //   amp.multi_claim(&match_id).await?;

    amp.logout().await;
    println!("\ndone.");
    Ok(())
}
