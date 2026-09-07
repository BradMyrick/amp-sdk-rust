# amp-sdk-rust

**AMP SDK for Rust — ranked matchmaking, skill ratings, and on-chain settlement for games on Avalanche.**

For native game servers, headless bots, tooling, and Rust game engines (Bevy). Async-first on tokio.

## Quickstart

```rust
use amp_sdk::AmpClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = AmpClient::with_signer("https://amp.playwithamp.xyz", &private_key)?;

    // One gasless signature to log in
    let player = client.login().await?;

    // Join a ranked queue
    client.join_queue("amp-tactics", "ranked-1v1").await?;

    // Watch the live event stream
    let mut ws = client.events().await?;
    if let Some(found) = ws.next_event("match_found").await {
        println!("match found: {}", found["data"]["matchId"]);
    }

    Ok(())
}
```

## What AMP handles vs what you handle

| AMP handles | Your game handles |
|---|---|
| Skill ratings (Glicko-2) | Determining who won |
| Matchmaking queue + skill windows | Running the actual game |
| Match assignment (WebSocket push) | Game UI/UX |
| Result verification + settlement | Player experience |
| On-chain escrow + payouts | Your game's economy |
| Anti-collusion (commit-reveal) | Your game's rules |

## API

| Method | Endpoint |
|---|---|
| `login()` | `/v1/auth/challenge` → `/v1/auth/verify` (EIP-191) |
| `me()`, `get_player(wallet)` | `/v1/me`, `/v1/players/:wallet` |
| `games()` | `/v1/games` |
| `join_queue`, `leave_queue`, `queue_status`, `play_bot` | `/v1/queue/*` |
| `get_match`, `match_history`, `report_match` | `/v1/matches/*` (EIP-191 auto-sign) |
| `create_party`, `join_party`, `get_party`, `lock_party`, `disband_party` | `/v1/parties*` |
| `multi_commit`, `multi_reveal`, `get_multi_match`, `multi_report`, `multi_claim` | `/v1/multi/*` (EIP-712 auto-sign) |
| `submit_exit_cert`, `countersign_exit_cert` | `/v1/multi/*/exit*` death certs (EIP-191 auto-sign) |
| `verify_escrow` | `/v1/matches/*/escrow/verify` for staked 1v1 |
| `wait_for_match(timeout)` | One call: queue → wait → matchId (2s polling) |
| `events()` | `/v1/ws` WebSocket with keepalive |

The client is `Clone` and shareable across tasks (token state behind an `Arc<RwLock>`).

## Crypto

Built on battle-tested crates — no hand-rolled cryptography:

- **keccak256 / EIP-712 typed data**: `alloy-primitives` + `alloy-dyn-abi`
- **secp256k1 signing**: `alloy-signer-local` (k256, RFC 6979 deterministic nonces)
- **Commit-reveal salts**: `rand` + OS entropy

Cross-SDK test vectors (keccak256, commit hashes, EIP-712 domain) match the TS, C#, and C++ SDKs byte-for-byte.

## Tests

9 tests: 8 unit (keccak vectors, salts, report messages, commit hashes, EIP-712 typed data shape, canonical address derivation) + 1 live integration (login → games → queue → bot match → report → party → WebSocket hello → logout).

```sh
cargo test                    # unit
AMP_TEST_KEY=0x... cargo test # + live against production
```

## License

Apache-2.0
