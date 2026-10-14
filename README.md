# amp-sdk-rust

**AMP SDK for Rust — official Avalanche Tournament & Match Protocol SDK.**

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Avalanche Fuji](https://img.shields.io/badge/Avalanche%20Fuji-43113-red.svg)](https://testnet.snowtrace.io/address/0x7C743c1C9AE3e7A65d030098F2249B7787d66dfF)
[![Avalanche Build Games](https://img.shields.io/badge/Avalanche-Build%20Games%20Winner-E84142.svg)](https://playwithamp.xyz)

> **Winner of the $15,000 Avalanche Build Games grant.**
>
> Official Rust SDK for decentralized esports tournaments, skill ratings, and ranked matchmaking on Avalanche. Manage tournament prize pools directly on-chain via `AMPTournamentCup.sol` and coordinate matchmaking with zero hassle.
> Built for native game servers, headless bots, verification tooling, and Rust game engines (Bevy). Async-first on tokio.

---

## 🏆 Avalanche Tournament Protocol

The Rust SDK includes direct integration with `AMPTournamentCup.sol`, the official smart contract powering sponsor-funded tournament escrow, EIP-712 verifier attestations, and anti-MEV pull-payment prize claims on Avalanche.

### Contracts & Networks
- **Network:** Avalanche Fuji Testnet (Chain ID `43113`)
- **Contract:** `0x7C743c1C9AE3e7A65d030098F2249B7787d66dfF`
- **RPC:** `https://api.avax-test.network/ext/bc/C/rpc`

### Tournament Client Example

```rust
use amp_sdk::{TournamentClient, TournamentState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize client targeting Avalanche Fuji testnet
    let client = TournamentClient::for_fuji();

    // 2. Fetch tournament details
    let tournament_id = 1;
    let tournament = client.get_tournament(tournament_id).await?;
    println!("Tournament #{} Status: {}", tournament.id, tournament.state);
    println!("Prize Pool: {:.4} AVAX", tournament.prize_pool_avax());
    println!("Attested Winners: {:?}", tournament.winners);

    // 3. Check if a player won and can claim their prize
    let player_wallet = "0x1563915e194D8CfBA1943570603F7606A3115508";
    let status = client.can_claim_prize(tournament_id, player_wallet).await?;

    if status.can_claim {
        let placement = status.placement.unwrap(); // 0 = 1st place
        println!(
            "Eligible to claim placement #{}! Prize: {:.4} AVAX",
            placement + 1,
            status.estimated_prize_avax()
        );

        // Generate calldata for winner's claimPrize transaction
        let calldata = TournamentClient::encode_claim_prize(tournament_id, placement);
        println!("Broadcast transaction with calldata: {calldata}");
    } else if status.already_claimed {
        println!("Prize already claimed: {}", status.reason);
    } else {
        println!("Cannot claim: {}", status.reason);
    }

    Ok(())
}
```

### EIP-712 Tournament Result Attestation

Verifiers attest final bracket results before claims unlock:

```rust
use amp_sdk::tournament::TournamentClient;

let client = TournamentClient::for_fuji();
let tournament_id = 42;
let winners = vec![
    "0x1563915e194D8CfBA1943570603F7606A3115508".to_string(),
    "0x5CbDd86a2FA8Dc4bDdd8a8f69dBa48572EeC07FB".to_string(),
];

// Compute the 32-byte EIP-712 digest matching AMPTournamentCup.sol
let digest_hex = client.compute_finalize_digest_hex(tournament_id, &winners)?;
println!("EIP-712 Digest: {digest_hex}");
```

---

## 🎮 Matchmaking & Glicko-2 Client

For real-time 1v1 and multiplayer matchmaking queues:

```rust
use amp_sdk::AmpClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = AmpClient::with_signer("https://amp.playwithamp.xyz", &private_key)?;

    // Gasless signature login
    let player = client.login().await?;

    // Join a ranked queue
    client.join_queue("amp-tactics", "ranked-1v1").await?;

    // Watch live event stream
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

## API Surface

### Tournament Protocol (`TournamentClient`)

| Method | Description |
|---|---|
| `TournamentClient::for_fuji()` | Initialize client with default Avalanche Fuji RPC & contract address |
| `TournamentClient::new(rpc, contract)` | Initialize client with custom RPC and contract address |
| `get_tournament(id)` | Query on-chain `TournamentData` from `getTournament(uint256)` |
| `is_prize_claimed(id, placement)` | Check if prize for placement is claimed from `claimed(uint256,uint256)` |
| `get_next_tournament_id()` | Fetch `nextTournamentId()` from contract |
| `can_claim_prize(id, wallet)` | Evaluate winner eligibility, placement, prize amount, and claim status |
| `compute_finalize_digest(id, winners)` | Compute 32-byte EIP-712 digest for `TournamentResult` attestation |
| `encode_get_tournament(id)` | Generate calldata for `getTournament(uint256)` (`0x1a5bd7fc`) |
| `encode_claimed(id, placement)` | Generate calldata for `claimed(uint256,uint256)` (`0xa54ab457`) |
| `encode_claim_prize(id, placement)` | Generate calldata for `claimPrize(uint256,uint256)` (`0x7b154140`) |
| `decode_tournament(id, hex)` | Decode ABI return payload into `TournamentData` |
| `decode_bool(hex)` | Decode ABI boolean return payload |

### Matchmaking Client (`AmpClient`)

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

## Crypto

Built on battle-tested crates — no hand-rolled cryptography:

- **keccak256 / EIP-712 typed data**: `alloy-primitives` + `alloy-dyn-abi`
- **secp256k1 signing**: `alloy-signer-local` (k256, RFC 6979 deterministic nonces)
- **Commit-reveal salts**: `rand` + OS entropy

Cross-SDK test vectors (keccak256, commit hashes, ladder EIP-712, tournament EIP-712) match the TS, C#, and C++ SDKs byte-for-byte.

## Testing

Comprehensive test suite covering unit tests, golden vectors, mock RPC responses, and live queries:

```sh
# Run all unit tests and live testnet queries
cargo test

# Run tests with output
cargo test -- --nocapture

# Run against production matchmaking server with private key
AMP_TEST_KEY=0x... cargo test --test live -- --nocapture
```

## License

Apache-2.0
