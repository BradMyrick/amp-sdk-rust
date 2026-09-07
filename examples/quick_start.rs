//! AMP Rust SDK — quick start.
//!
//! ```sh
//! AMP_TEST_KEY=0x... cargo run --example quick_start
//! ```

use amp_sdk::AmpClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let key = std::env::var("AMP_TEST_KEY")
        .expect("set AMP_TEST_KEY to a funded test private key");

    let client = AmpClient::with_signer("https://amp.playwithamp.xyz", &key)?;

    // One gasless signature to log in
    let player = client.login().await?;
    println!("logged in as {}", player.wallet);

    // Join a ranked queue
    client.join_queue("amp-tactics", "ranked-1v1").await?;
    println!("queued — waiting for an opponent...");

    // Watch the live event stream
    let mut ws = client.events().await?;
    if let Some(found) = ws.next_event("match_found").await {
        println!("match found: {}", found["data"]["matchId"]);
    }

    Ok(())
}
