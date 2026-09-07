//! Live integration tests — run against the production matchmaker.
//!
//! ```sh
//! AMP_TEST_KEY=0x... cargo test --test live -- --nocapture
//! ```
//! Falls back to /tmp/opencode/e2e-wallets/wallets.json if the env var
//! is unset. Skips silently when no key is available.

use amp_sdk::AmpClient;

fn test_key() -> Option<String> {
    if let Ok(k) = std::env::var("AMP_TEST_KEY") {
        return Some(k);
    }
    let data = std::fs::read_to_string("/tmp/opencode/e2e-wallets/wallets.json").ok()?;
    let v: serde_json::Value = serde_json::from_str(&data).ok()?;
    v.as_array()?
        .iter()
        .find(|w| w["index"] == 4)? // distinct wallet per SDK
        .get("key")?
        .as_str()
        .map(String::from)
}

fn server() -> String {
    std::env::var("AMP_SERVER").unwrap_or_else(|_| "https://amp.playwithamp.xyz".into())
}

#[tokio::test]
async fn full_live_flow() {
    let Some(key) = test_key() else {
        eprintln!("SKIP: no test key available");
        return;
    };

    let client = AmpClient::with_signer(&server(), &key).unwrap();

    // Login — proves the alloy EIP-191 signature is accepted
    let player = client.login().await.unwrap();
    let expected = key
        .parse::<amp_sdk::PrivateKeySigner>()
        .unwrap()
        .address()
        .to_checksum(None);
    assert_eq!(player.wallet, expected);
    assert!(client.authenticated().await);

    // Public reads
    let games = client.games().await.unwrap();
    assert!(games["games"].as_array().map(|a| !a.is_empty()).unwrap_or(false));

    let me = client.me().await.unwrap();
    assert!(me.get("wallet").is_some());

    // Queue lifecycle
    let join = client.join_queue("amp-tactics", "ranked-1v1").await.unwrap();
    assert!(join.get("ticketId").is_some() || join.get("queueDepth").is_some());

    let _status = client.queue_status().await.unwrap();
    let left = client.leave_queue().await.unwrap();
    assert!(left.get("left").is_some());

    // Bot match + report
    let bot = client.play_bot().await.unwrap();
    let match_id = bot["matchId"].as_str().expect("playBot returns matchId").to_string();
    assert!(!match_id.is_empty());

    let m = client.get_match(&match_id).await.unwrap();
    assert!(m.get("id").is_some() || m.get("matchId").is_some());

    let rep = client.report_match(&match_id, "win").await.unwrap();
    assert!(rep.get("state").is_some());

    // Party lifecycle
    let party = client.create_party("amp-tactics", "ranked-1v1").await.unwrap();
    let party_id = party["partyId"].as_str().expect("partyId").to_string();
    let _ = client.get_party(&party_id).await.unwrap();
    let disbanded = client.disband_party(&party_id).await.unwrap();
    assert!(disbanded.get("disbanded").is_some());

    // WebSocket hello
    let mut ws = client.events().await.unwrap();
    let hello = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        ws.next_event("hello"),
    )
    .await
    .expect("hello within 10s")
    .expect("hello event");
    assert!(hello["data"]["wallet"].as_str().is_some());

    client.logout().await;
    assert!(!client.authenticated().await);
}
