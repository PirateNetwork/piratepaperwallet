use piratepaperlib::paper::{generate_wallet, SeedSource, WalletOptions};

#[test]
fn matches_updated_full_node_for_raw_and_bip39_seeds() {
    // Fixtures came from executing the full node's actual Rust source, then
    // comparing it independently with the light wallet's derivation formula.
    let fixtures: serde_json::Value =
        serde_json::from_str(include_str!("data/ironwood-vectors.json")).unwrap();
    let entropy = hex::decode(fixtures["entropy"].as_str().unwrap()).unwrap();
    for raw in [true, false] {
        let options = WalletOptions {
            count: 2,
            nobip39: raw,
            ..WalletOptions::default()
        };
        let wallets = generate_wallet(SeedSource::HdSeed(&entropy), options).unwrap();
        for fixture in fixtures["vectors"].as_array().unwrap() {
            if (fixture["mode"] == "raw") != raw {
                continue;
            }
            let account = fixture["account"].as_u64().unwrap() as usize;
            let wallet = &wallets[account];
            assert_eq!(wallet.seed.path, fixture["path"]);
            assert_eq!(
                hex::encode(bech32::decode(&wallet.private_key).unwrap().1),
                fixture["private_key_bytes"]
            );
            assert_eq!(
                hex::encode(bech32::decode(&wallet.viewing_key).unwrap().1),
                fixture["viewing_key_bytes"]
            );
            let addresses = [
                &wallet.address,
                &wallet.diversified.d1,
                &wallet.diversified.d2,
                &wallet.diversified.d3,
                &wallet.diversified.d4,
                &wallet.diversified.d5,
            ];
            for (index, address) in addresses.iter().enumerate() {
                assert_eq!(address.as_str(), fixture["addresses"][index]);
            }
        }
    }
}
