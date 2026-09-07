//! Unit tests — crypto vectors matching the TS/C#/C++ SDKs.

use amp_sdk::crypto;

#[test]
fn keccak_empty_matches_known_vector() {
    // keccak256("")
    assert_eq!(
        crypto::keccak_hex(b""),
        "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
    );
}

#[test]
fn keccak_abc_matches_cast_vector() {
    assert_eq!(
        crypto::keccak_hex(b"abc"),
        "0x4e03657aea45a94fc7d47ba826c8d667c0d1e6e33a64a036ec44f58fa12d6c45"
    );
}

#[test]
fn salts_are_unique_and_well_formed() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..100 {
        let s = crypto::generate_salt();
        assert!(s.starts_with("0x"));
        assert_eq!(s.len(), 66);
        assert!(seen.insert(s));
    }
}

#[test]
fn report_message_format() {
    assert_eq!(
        crypto::build_report_message("m1", "win"),
        "AMP_REPORT:v1:m1:win"
    );
}

#[test]
fn commit_hash_is_deterministic_and_sensitive() {
    let w = "0x95CC495dF579981d3Ffa4a8f77B93A17563E077a";
    let salt = "0x0102";
    let h1 = crypto::compute_commit_hash(w, 1000, salt).unwrap();
    let h2 = crypto::compute_commit_hash(w, 1000, salt).unwrap();
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 66);

    let h3 = crypto::compute_commit_hash(w, 2000, salt).unwrap();
    let h4 = crypto::compute_commit_hash(w, 1000, "0x0304").unwrap();
    assert_ne!(h1, h3);
    assert_ne!(h1, h4);
}

#[test]
fn ladder_typed_data_shape() {
    let td = crypto::build_ladder_typed_data(
        43113,
        "0xcabf7b626172fE55d54f03c346563671AbcC77f7",
        "0x0000000000000000000000000000000000000000000000000000000000000001",
        &["0x95CC495dF579981d3Ffa4a8f77B93A17563E077a".into()],
        "0x0000000000000000000000000000000000000000000000000000000000000002",
        42,
    )
    .unwrap();

    assert_eq!(td.domain.name, Some("AMPMultiplayer".into()));
    assert_eq!(td.domain.version, Some("1".into()));
    assert_eq!(td.domain.chain_id, Some(alloy_primitives::U256::from(43113u64)));
    assert_eq!(td.primary_type, "MultiplayerLadder");

    // Digest is 32 bytes and chain-sensitive
    let d1 = td.clone().eip712_signing_hash().unwrap();
    let mut td2 = td.clone();
    td2.domain.chain_id = Some(alloy_primitives::U256::from(1u64));
    let d2 = td2.eip712_signing_hash().unwrap();
    assert_eq!(d1.len(), 32);
    assert_ne!(d1, d2);
}

#[test]
fn signer_derives_canonical_address() {
    // privkey = 1 → 0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf
    let signer: amp_sdk::PrivateKeySigner =
        "0x0000000000000000000000000000000000000000000000000000000000000001"
            .parse()
            .unwrap();
    assert_eq!(
        signer.address().to_checksum(None).to_lowercase(),
        "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf"
    );
}

#[test]
fn exit_cert_message_matches_server_format() {
    // Load-bearing: the server recovers the signer from this exact string.
    let msg = crypto::build_exit_cert_message("m-42", 3, 1200, "0xabc");
    assert_eq!(
        msg,
        "AMP exit certificate\n\n\
         Match: m-42\n\
         Rank: 3\n\
         Exit frame: 1200\n\
         State hash: 0xabc\n\n\
         This signature is free. It certifies your elimination and unlocks your reporting bond."
    );
}
