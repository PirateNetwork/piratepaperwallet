//! Offline paper-wallet generation and recovery.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

use bip39::{Language, Mnemonic};
use rand::{rngs::OsRng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ironwood, sapling};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pool {
    Ironwood,
    Sapling,
}

impl Pool {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ironwood => "Ironwood",
            Self::Sapling => "Sapling",
        }
    }
}

#[derive(Clone, Copy)]
pub struct WalletOptions {
    pub pool: Pool,
    pub count: u32,
    pub coin_type: u32,
    pub nohd: bool,
    pub nobip39: bool,
}

impl Default for WalletOptions {
    fn default() -> Self {
        Self {
            pool: Pool::Ironwood,
            count: 1,
            coin_type: 141,
            nohd: false,
            nobip39: false,
        }
    }
}

pub enum SeedSource<'a> {
    Random(&'a [u8]),
    HdSeed(&'a [u8]),
    Phrase(&'a str),
    PartialPhrase(&'a str),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiversifiedAddresses {
    pub d1: String,
    pub d2: String,
    pub d3: String,
    pub d4: String,
    pub d5: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SeedInfo {
    #[serde(rename = "Phrase")]
    pub phrase: String,
    #[serde(rename = "HDSeed")]
    pub hdseed: String,
    #[serde(rename = "Bip39Seed")]
    pub bip39_seed: String,
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WalletRecord {
    pub num: u32,
    pub pool: Pool,
    pub address: String,
    pub diversified: DiversifiedAddresses,
    pub viewing_key: String,
    pub private_key: String,
    pub seed: SeedInfo,
}

fn validate(options: WalletOptions) -> Result<(), String> {
    if options.count == 0 {
        return Err("Address count must be at least 1".to_owned());
    }
    if options.count >= 1 << 31 || options.coin_type >= 1 << 31 {
        return Err("Account count and coin type must be below 2^31".to_owned());
    }
    Ok(())
}

fn record_from_entropy(
    entropy: &[u8; 32],
    account: u32,
    num: u32,
    options: WalletOptions,
) -> Result<WalletRecord, String> {
    let mnemonic = Mnemonic::from_entropy(entropy).map_err(|e| e.to_string())?;
    let bip39_seed = mnemonic.to_seed("");
    let key_seed: &[u8] = if options.nobip39 {
        entropy
    } else {
        &bip39_seed
    };

    let (address, diversified, viewing_key, private_key) = match options.pool {
        Pool::Ironwood => {
            let a = ironwood::derive_account(key_seed, account, options.coin_type)?;
            (a.address, a.diversified, a.viewing_key, a.private_key)
        }
        Pool::Sapling => {
            let a = sapling::derive_account(key_seed, account, options.coin_type)?;
            (a.address, a.diversified, a.viewing_key, a.private_key)
        }
    };
    let [d1, d2, d3, d4, d5]: [String; 5] = diversified
        .try_into()
        .map_err(|_| "Expected five diversified addresses".to_owned())?;
    Ok(WalletRecord {
        num,
        pool: options.pool,
        address,
        diversified: DiversifiedAddresses { d1, d2, d3, d4, d5 },
        viewing_key,
        private_key,
        seed: SeedInfo {
            phrase: if options.nobip39 {
                "BIP39 disabled".to_owned()
            } else {
                mnemonic.to_string()
            },
            hdseed: hex::encode(entropy),
            bip39_seed: if options.nobip39 {
                "BIP39 disabled".to_owned()
            } else {
                hex::encode(bip39_seed)
            },
            path: format!("m/32'/{}'/{}'", options.coin_type, account),
        },
    })
}

fn generate_from_entropy(
    entropy: &[u8; 32],
    options: WalletOptions,
) -> Result<Vec<WalletRecord>, String> {
    (0..options.count)
        .map(|account| record_from_entropy(entropy, account, account, options))
        .collect()
}

fn random_seed(rng: &mut impl RngCore) -> [u8; 32] {
    let mut seed = [0; 32];
    rng.fill_bytes(&mut seed);
    seed
}

/// Generate wallets from OS randomness, an HD seed, or a BIP39 phrase.
/// Additional entropy is mixed with OS randomness and never used on its own.
pub fn generate_wallet(
    source: SeedSource<'_>,
    options: WalletOptions,
) -> Result<Vec<WalletRecord>, String> {
    validate(options)?;
    match source {
        SeedSource::Random(user_entropy) => {
            let mut system_entropy = [0; 32];
            OsRng
                .try_fill_bytes(&mut system_entropy)
                .map_err(|e| format!("OS randomness unavailable: {e}"))?;
            let mut hash = Sha256::new();
            hash.update(system_entropy);
            hash.update(user_entropy);
            let mut rng_seed = [0; 32];
            rng_seed.copy_from_slice(&hash.finalize());
            let mut rng = ChaCha20Rng::from_seed(rng_seed);
            if options.nohd {
                (0..options.count)
                    .map(|num| record_from_entropy(&random_seed(&mut rng), 0, num, options))
                    .collect()
            } else {
                generate_from_entropy(&random_seed(&mut rng), options)
            }
        }
        SeedSource::HdSeed(seed) => {
            if options.nohd {
                return Err("--nohd cannot be used when recovering an HD seed".to_owned());
            }
            let seed: [u8; 32] = seed
                .try_into()
                .map_err(|_| "HD seed must be exactly 32 bytes".to_owned())?;
            generate_from_entropy(&seed, options)
        }
        SeedSource::Phrase(phrase) => {
            if options.nohd || options.nobip39 {
                return Err("Phrase recovery requires HD and BIP39 mode".to_owned());
            }
            let mnemonic = Mnemonic::parse_in(Language::English, phrase)
                .map_err(|e| format!("Invalid English BIP39 phrase: {e}"))?;
            if mnemonic.word_count() != 24 {
                return Err("Recovery phrase must have 24 words".to_owned());
            }
            let entropy: [u8; 32] = mnemonic
                .to_entropy()
                .try_into()
                .map_err(|_| "Recovery phrase must encode 32 bytes".to_owned())?;
            generate_from_entropy(&entropy, options)
        }
        SeedSource::PartialPhrase(phrase) => {
            if options.nohd || options.nobip39 {
                return Err("Partial phrase recovery requires HD and BIP39 mode".to_owned());
            }
            let known: Vec<&str> = phrase.split_whitespace().collect();
            if known.len() != 23 {
                return Err("Partial recovery phrase must have 23 words".to_owned());
            }
            let mut seen = HashSet::new();
            let mut records = Vec::new();
            for position in 0..24 {
                for word in Language::English.word_list() {
                    let mut candidate = known.clone();
                    candidate.insert(position, word);
                    let phrase = candidate.join(" ");
                    if !seen.insert(phrase.clone()) {
                        continue;
                    }
                    if let Ok(mnemonic) = Mnemonic::parse_in(Language::English, &phrase) {
                        let entropy: [u8; 32] = mnemonic
                            .to_entropy()
                            .try_into()
                            .map_err(|_| "Recovery phrase must encode 32 bytes".to_owned())?;
                        records.extend(generate_from_entropy(&entropy, options)?);
                    }
                }
            }
            if records.is_empty() {
                return Err("No valid 24-word BIP39 phrase was found".to_owned());
            }
            Ok(records)
        }
    }
}

/// Grind full account seeds so the resulting default receive address has a
/// requested Bech32 data prefix. Recover with the phrase or raw HD seed,
/// according to the selected BIP39 mode.
pub fn generate_vanity_wallet(
    prefix: &str,
    threads: usize,
    options: WalletOptions,
) -> Result<Vec<WalletRecord>, String> {
    validate(options)?;
    if options.count != 1 || options.nohd {
        return Err("Vanity mode requires one HD account".to_owned());
    }
    if threads == 0 {
        return Err("Vanity mode requires at least one thread".to_owned());
    }
    const CHARSET: &str = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    if prefix.is_empty() || !prefix.chars().all(|c| CHARSET.contains(c)) {
        return Err("Vanity prefix must contain only lowercase Bech32 data characters".to_owned());
    }
    // A 43-byte receiver has 69 base32 symbols. The last symbol contains
    // four data bits and one zero padding bit; checksum symbols are excluded.
    if prefix.len() > 69 {
        return Err("Vanity prefix cannot exceed 69 address data characters".to_owned());
    }
    if prefix.len() == 69 && CHARSET.find(prefix.as_bytes()[68] as char).unwrap() % 2 != 0 {
        return Err("The final address data character must have a zero padding bit".to_owned());
    }
    let wanted = format!(
        "{}1{}",
        match options.pool {
            Pool::Ironwood => "pirate",
            Pool::Sapling => "zs",
        },
        prefix
    );
    let stop = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let sender = sender.clone();
            let stop = Arc::clone(&stop);
            let wanted = wanted.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let mut entropy = [0; 32];
                    if let Err(e) = OsRng.try_fill_bytes(&mut entropy) {
                        let _ = sender.send(Err(format!("OS randomness unavailable: {e}")));
                        break;
                    }
                    match record_from_entropy(&entropy, 0, 0, options) {
                        Ok(record) if record.address.starts_with(&wanted) => {
                            let _ = sender.send(Ok(record));
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            let _ = sender.send(Err(e));
                            break;
                        }
                    }
                }
            })
        })
        .collect();
    drop(sender);
    let result = receiver
        .recv()
        .map_err(|_| "All vanity workers stopped".to_owned())?;
    stop.store(true, Ordering::Relaxed);
    for handle in handles {
        handle
            .join()
            .map_err(|_| "Vanity worker panicked".to_owned())?;
    }
    result.map(|record| vec![record])
}

pub fn to_json(records: &[WalletRecord]) -> Result<String, String> {
    serde_json::to_string_pretty(records).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdseed_and_phrase_recover_same_ironwood_accounts() {
        let seed: Vec<u8> = (0..32).collect();
        let options = WalletOptions {
            count: 2,
            ..WalletOptions::default()
        };
        let from_seed = generate_wallet(SeedSource::HdSeed(&seed), options).unwrap();
        let phrase = from_seed[0].seed.phrase.clone();
        let from_phrase = generate_wallet(SeedSource::Phrase(&phrase), options).unwrap();
        for i in 0..2 {
            assert_eq!(from_seed[i].address, from_phrase[i].address);
            assert_eq!(from_seed[i].private_key, from_phrase[i].private_key);
            assert_eq!(from_seed[i].seed.path, format!("m/32'/141'/{}'", i));
        }
    }

    #[test]
    fn raw_seed_and_bip39_seed_select_distinct_accounts() {
        let seed = [7; 32];
        let bip39 = generate_wallet(SeedSource::HdSeed(&seed), WalletOptions::default()).unwrap();
        let raw = generate_wallet(
            SeedSource::HdSeed(&seed),
            WalletOptions {
                nobip39: true,
                ..WalletOptions::default()
            },
        )
        .unwrap();
        assert_ne!(bip39[0].address, raw[0].address);
        assert_eq!(raw[0].seed.hdseed, hex::encode(seed));
    }

    #[test]
    fn vanity_rejects_invalid_inputs_before_launching_workers() {
        assert!(generate_vanity_wallet("B", 1, WalletOptions::default()).is_err());
        assert!(generate_vanity_wallet("q", 0, WalletOptions::default()).is_err());
        assert!(generate_vanity_wallet(&"q".repeat(70), 1, WalletOptions::default()).is_err());
        assert!(generate_vanity_wallet(
            &format!("{}p", "q".repeat(68)),
            1,
            WalletOptions::default()
        )
        .is_err());
    }

    #[test]
    fn independent_wallets_use_separate_seeds_at_account_zero() {
        let wallets = generate_wallet(
            SeedSource::Random(&[]),
            WalletOptions {
                count: 3,
                nohd: true,
                ..WalletOptions::default()
            },
        )
        .unwrap();
        let seeds: HashSet<_> = wallets.iter().map(|w| &w.seed.hdseed).collect();
        let addresses: HashSet<_> = wallets.iter().map(|w| &w.address).collect();
        assert_eq!(seeds.len(), 3);
        assert_eq!(addresses.len(), 3);
        assert!(wallets.iter().all(|w| w.seed.path == "m/32'/141'/0'"));
    }
}
