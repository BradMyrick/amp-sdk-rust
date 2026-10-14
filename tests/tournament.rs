//! Tournament Protocol unit and live integration tests.

use alloy_primitives::U256;
use amp_sdk::tournament::{
    compute_finalize_digest_hex, TournamentClient, TournamentState, DEFAULT_CUP_ADDRESS,
    DEFAULT_FUJI_CHAIN_ID, DEFAULT_FUJI_RPC, SIG_CLAIMED, SIG_CLAIM_PRIZE, SIG_GET_TOURNAMENT,
    SIG_NEXT_TOURNAMENT_ID,
};

#[test]
fn test_tournament_state_display_and_conversion() {
    assert_eq!(TournamentState::try_from(0).unwrap(), TournamentState::Open);
    assert_eq!(
        TournamentState::try_from(1).unwrap(),
        TournamentState::Finalized
    );
    assert_eq!(
        TournamentState::try_from(2).unwrap(),
        TournamentState::Complete
    );
    assert_eq!(
        TournamentState::try_from(3).unwrap(),
        TournamentState::Cancelled
    );
    assert!(TournamentState::try_from(4).is_err());

    assert_eq!(format!("{}", TournamentState::Open), "Open");
    assert_eq!(format!("{}", TournamentState::Finalized), "Finalized");
    assert_eq!(format!("{}", TournamentState::Complete), "Complete");
    assert_eq!(format!("{}", TournamentState::Cancelled), "Cancelled");
}

#[test]
fn test_calldata_generation() {
    // 1. getTournament(uint256)
    let cd1 = TournamentClient::encode_get_tournament(1);
    assert_eq!(
        cd1,
        format!("{SIG_GET_TOURNAMENT}0000000000000000000000000000000000000000000000000000000000000001")
    );

    let cd8 = TournamentClient::encode_get_tournament(8);
    assert_eq!(
        cd8,
        format!("{SIG_GET_TOURNAMENT}0000000000000000000000000000000000000000000000000000000000000008")
    );

    // 2. claimed(uint256,uint256)
    let cd_claimed = TournamentClient::encode_claimed(8, 1);
    assert_eq!(
        cd_claimed,
        format!(
            "{SIG_CLAIMED}0000000000000000000000000000000000000000000000000000000000000008\
             0000000000000000000000000000000000000000000000000000000000000001"
        )
    );

    // 3. claimPrize(uint256,uint256)
    let cd_claim = TournamentClient::encode_claim_prize(8, 0);
    assert_eq!(
        cd_claim,
        format!(
            "{SIG_CLAIM_PRIZE}0000000000000000000000000000000000000000000000000000000000000008\
             0000000000000000000000000000000000000000000000000000000000000000"
        )
    );

    // 4. nextTournamentId()
    assert_eq!(
        TournamentClient::encode_next_tournament_id(),
        SIG_NEXT_TOURNAMENT_ID
    );
}

#[test]
fn test_decode_bool() {
    let true_hex = "0x0000000000000000000000000000000000000000000000000000000000000001";
    let false_hex = "0x0000000000000000000000000000000000000000000000000000000000000000";

    assert!(TournamentClient::decode_bool(true_hex).unwrap());
    assert!(!TournamentClient::decode_bool(false_hex).unwrap());
    assert!(TournamentClient::decode_bool("0x01").unwrap());
    assert!(!TournamentClient::decode_bool("0x00").unwrap());
    assert!(!TournamentClient::decode_bool("").unwrap());
}

#[test]
fn test_decode_tournament_sample_payload() {
    // Exact response hex from Fuji Testnet for Tournament #1
    let raw_hex = "\
        0x0000000000000000000000000000000000000000000000000000000000000020\
        00000000000000000000000095cc495df579981d3ffa4a8f77b93a17563e077a\
        0000000000000000000000000000000000000000000000000000000000000000\
        00000000000000000000000000000000000000000000000000b1a2bc2ec50000\
        0000000000000000000000000000000000000000000000000000000000000120\
        00000000000000000000000019e7e376e7c213b7e7e7e46cc70a5dd086daff2a\
        0000000000000000000000000000000000000000000000000000000000000180\
        0000000000000000000000000000000000000000000000000000000000000002\
        000000000000000000000000000000000000000000000000000000006a74be98\
        000000000000000000000000000000000000000000000000000000006a761014\
        0000000000000000000000000000000000000000000000000000000000000002\
        0000000000000000000000000000000000000000000000000000000000001b58\
        0000000000000000000000000000000000000000000000000000000000000bb8\
        0000000000000000000000000000000000000000000000000000000000000002\
        0000000000000000000000001563915e194d8cfba1943570603f7606a3115508\
        0000000000000000000000005cbdd86a2fa8dc4bddd8a8f69dba48572eec07fb";

    let t = TournamentClient::decode_tournament(1, raw_hex).unwrap();

    assert_eq!(t.id, 1);
    assert_eq!(
        t.sponsor.to_lowercase(),
        "0x95cc495df579981d3ffa4a8f77b93a17563e077a"
    );
    assert_eq!(
        t.token,
        "0x0000000000000000000000000000000000000000"
    );
    assert!(t.is_native_avax());

    // Prize pool: 0x00b1a2bc2ec50000 = 50,000,000,000,000,000 wei = 0.05 AVAX
    assert_eq!(t.prize_pool, U256::from(50_000_000_000_000_000u64));
    assert!((t.prize_pool_avax() - 0.05).abs() < 1e-9);

    assert_eq!(t.payout_bps, vec![7000, 3000]);
    assert_eq!(
        t.verifier.to_lowercase(),
        "0x19e7e376e7c213b7e7e7e46cc70a5dd086daff2a"
    );
    assert_eq!(t.winners.len(), 2);
    assert_eq!(
        t.winners[0].to_lowercase(),
        "0x1563915e194d8cfba1943570603f7606a3115508"
    );
    assert_eq!(
        t.winners[1].to_lowercase(),
        "0x5cbdd86a2fa8dc4bddd8a8f69dba48572eec07fb"
    );

    assert_eq!(t.state, TournamentState::Complete);
    assert_eq!(t.created_at, 0x6a74be98);
    assert_eq!(t.finalize_deadline, 0x6a761014);

    // Prize calculation tests
    // 70% of 0.05 AVAX = 0.035 AVAX (35,000,000,000,000,000 wei)
    let p0 = t.get_placement_prize(0);
    assert_eq!(p0, U256::from(35_000_000_000_000_000u64));
    assert!((t.get_placement_prize_avax(0) - 0.035).abs() < 1e-9);

    // 30% of 0.05 AVAX = 0.015 AVAX (15,000,000,000,000,000 wei)
    let p1 = t.get_placement_prize(1);
    assert_eq!(p1, U256::from(15_000_000_000_000_000u64));
    assert!((t.get_placement_prize_avax(1) - 0.015).abs() < 1e-9);

    // Out of bounds placement returns 0
    assert_eq!(t.get_placement_prize(2), U256::ZERO);
}

#[test]
fn test_eip712_digest_matches_golden_vector() {
    let tournament_id = 42u64;
    let winners = vec![
        "0x1563915e194D8CfBA1943570603F7606A3115508".to_string(),
        "0x5CbDd86a2FA8Dc4bDdd8a8f69dBa48572EeC07FB".to_string(),
    ];

    let client = TournamentClient::new(DEFAULT_FUJI_RPC, DEFAULT_CUP_ADDRESS);
    let digest_from_client = client
        .compute_finalize_digest_hex(tournament_id, &winners)
        .unwrap();

    let digest_from_fn = compute_finalize_digest_hex(
        DEFAULT_FUJI_CHAIN_ID,
        DEFAULT_CUP_ADDRESS,
        tournament_id,
        &winners,
    )
    .unwrap();

    // Golden vector verified with ethers.TypedDataEncoder.hash in JS SDK tests
    let expected = "0xb54613dbbd2cf573d412e5d39c98997982087b27df336c4af07662a5b94b5a8d";

    assert_eq!(digest_from_client, expected);
    assert_eq!(digest_from_fn, expected);
}

#[tokio::test]
async fn test_mock_rpc_can_claim_scenarios() {
    // Test tournament encoded hex:
    // State = Finalized (1), 1.0 AVAX prize pool, 70/30 split
    // Winner 0 = 0x1111111111111111111111111111111111111111
    // Winner 1 = 0x2222222222222222222222222222222222222222
    let finalized_hex = "\
        0x0000000000000000000000000000000000000000000000000000000000000020\
        00000000000000000000000095cc495df579981d3ffa4a8f77b93a17563e077a\
        0000000000000000000000000000000000000000000000000000000000000000\
        0000000000000000000000000000000000000000000000000de0b6b3a7640000\
        0000000000000000000000000000000000000000000000000000000000000120\
        00000000000000000000000019e7e376e7c213b7e7e7e46cc70a5dd086daff2a\
        0000000000000000000000000000000000000000000000000000000000000180\
        0000000000000000000000000000000000000000000000000000000000000001\
        000000000000000000000000000000000000000000000000000000006a74be98\
        000000000000000000000000000000000000000000000000000000006a761014\
        0000000000000000000000000000000000000000000000000000000000000002\
        0000000000000000000000000000000000000000000000000000000000001b58\
        0000000000000000000000000000000000000000000000000000000000000bb8\
        0000000000000000000000000000000000000000000000000000000000000002\
        0000000000000000000000001111111111111111111111111111111111111111\
        0000000000000000000000002222222222222222222222222222222222222222";

    // Setup local TCP mock server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let rpc_url = format!("http://{}", addr);

    let finalized_hex_owned = finalized_hex.to_string();
    let _server_handle = tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let hex_copy = finalized_hex_owned.clone();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = vec![0u8; 4096];
                let n = match socket.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };
                let req_str = String::from_utf8_lossy(&buf[..n]);
                if let Some(pos) = req_str.find("\r\n\r\n") {
                    let body = &req_str[pos + 4..];
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
                        let id = v.get("id").cloned().unwrap_or(serde_json::json!(1));
                        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
                        let result = if method == "eth_call" {
                            let calldata = v["params"][0]["data"].as_str().unwrap_or("");
                            if calldata.starts_with(SIG_GET_TOURNAMENT) {
                                serde_json::json!(hex_copy)
                            } else if calldata.starts_with(SIG_CLAIMED) {
                                // Claimed: placement 0 unclaimed, placement 1 claimed
                                if calldata.ends_with("0000000000000000000000000000000000000000000000000000000000000000") {
                                    serde_json::json!("0x0000000000000000000000000000000000000000000000000000000000000000")
                                } else {
                                    serde_json::json!("0x0000000000000000000000000000000000000000000000000000000000000001")
                                }
                            } else {
                                serde_json::json!("0x")
                            }
                        } else {
                            serde_json::json!("0x")
                        };

                        let resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": result
                        });
                        let resp_body = serde_json::to_string(&resp).unwrap();
                        let http_resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            resp_body.len(),
                            resp_body
                        );
                        let _ = socket.write_all(http_resp.as_bytes()).await;
                    }
                }
            });
        }
    });

    let client = TournamentClient::new(&rpc_url, DEFAULT_CUP_ADDRESS);

    // Case 1: Winner 0 (1st place, unclaimed, tournament finalized) -> can_claim = true
    let res0 = client
        .can_claim_prize(1, "0x1111111111111111111111111111111111111111")
        .await
        .unwrap();
    assert!(res0.can_claim);
    assert!(res0.is_winner);
    assert_eq!(res0.placement, Some(0));
    assert_eq!(res0.placement_index(), 0);
    assert!(!res0.already_claimed);
    assert_eq!(
        res0.estimated_prize,
        U256::from(700_000_000_000_000_000u64)
    ); // 70% of 1 AVAX
    assert!((res0.estimated_prize_avax() - 0.70).abs() < 1e-9);
    assert_eq!(res0.reason, "Eligible to claim");

    // Case 2: Winner 1 (2nd place, already claimed) -> can_claim = false, already_claimed = true
    let res1 = client
        .can_claim_prize(1, "0x2222222222222222222222222222222222222222")
        .await
        .unwrap();
    assert!(!res1.can_claim);
    assert!(res1.is_winner);
    assert_eq!(res1.placement, Some(1));
    assert_eq!(res1.placement_index(), 1);
    assert!(res1.already_claimed);
    assert!(res1.reason.contains("already been claimed"));

    // Case 3: Non-winner -> can_claim = false, is_winner = false, placement = None
    let res_non = client
        .can_claim_prize(1, "0x3333333333333333333333333333333333333333")
        .await
        .unwrap();
    assert!(!res_non.can_claim);
    assert!(!res_non.is_winner);
    assert_eq!(res_non.placement, None);
    assert_eq!(res_non.placement_index(), -1);
    assert!(!res_non.already_claimed);
    assert!(res_non.reason.contains("not an attested winner"));
}

#[tokio::test]
async fn test_live_fuji_query() {
    let client = TournamentClient::for_fuji();

    // Query nextTournamentId
    let next_id = match client.get_next_tournament_id().await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Skipping live Fuji test (RPC unreachable): {e}");
            return;
        }
    };
    assert!(next_id >= 1, "nextTournamentId should be >= 1");

    // Query tournament #1
    let t = client.get_tournament(1).await.unwrap();
    assert_eq!(t.id, 1);
    assert!(t.is_native_avax());
    assert_eq!(t.payout_bps.len(), 2);
    assert_eq!(t.winners.len(), 2);

    // Check claimed status
    let claimed_0 = client.is_prize_claimed(1, 0).await.unwrap();
    assert!(claimed_0, "Placement 0 of tournament 1 should be claimed");

    // Check can_claim_prize for winner
    let winner = &t.winners[0];
    let claim_res = client.can_claim_prize(1, winner).await.unwrap();
    assert!(claim_res.is_winner);
    assert_eq!(claim_res.placement, Some(0));
    assert!(claim_res.already_claimed);
    assert!(!claim_res.can_claim);

    // Check can_claim_prize for non-winner
    let non_winner = "0x000000000000000000000000000000000000dEaD";
    let non_winner_res = client.can_claim_prize(1, non_winner).await.unwrap();
    assert!(!non_winner_res.is_winner);
    assert_eq!(non_winner_res.placement, None);
    assert!(!non_winner_res.can_claim);
}
