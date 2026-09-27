//! Ironwood account derivation and Pirate Chain encoding.
//!
//! Uses the same ZIP-32 key hierarchy as the full node (6.0.7+) and light wallet.
//! The Orchard crate keeps its extended key metadata private, so this module
//! implements only the ZIP-32 envelope needed for Pirate's import format.

use bech32::{Bech32, Hrp};
use blake2b_simd::Params;
use orchard::keys::{FullViewingKey, Scope, SpendingKey};

const MASTER_PERSONALIZATION: &[u8; 16] = b"ZcashIP32Orchard";
const EXPAND_PERSONALIZATION: &[u8; 16] = b"Zcash_ExpandSeed";
const FVK_TAG_PERSONALIZATION: &[u8; 16] = b"ZcashOrchardFVFP";

pub struct IronwoodAccount {
    pub address: String,
    pub diversified: Vec<String>,
    pub viewing_key: String,
    pub private_key: String,
}

struct ExtendedSpendingKey {
    depth: u8,
    parent_fvk_tag: [u8; 4],
    child_index: u32,
    chain_code: [u8; 32],
    sk: SpendingKey,
}

impl ExtendedSpendingKey {
    fn master(seed: &[u8]) -> Result<Self, String> {
        if !(32..=252).contains(&seed.len()) {
            return Err("Ironwood seed must be 32 to 252 bytes".to_owned());
        }
        let hash = Params::new()
            .hash_length(64)
            .personal(MASTER_PERSONALIZATION)
            .hash(seed);
        let mut sk_bytes = [0; 32];
        sk_bytes.copy_from_slice(&hash.as_bytes()[..32]);
        let sk = SpendingKey::from_bytes(sk_bytes);
        if !bool::from(sk.is_some()) {
            return Err("Seed derived an invalid Ironwood spending key".to_owned());
        }
        let mut chain_code = [0; 32];
        chain_code.copy_from_slice(&hash.as_bytes()[32..]);
        Ok(Self {
            depth: 0,
            parent_fvk_tag: [0; 4],
            child_index: 0,
            chain_code,
            sk: sk.unwrap(),
        })
    }

    fn derive_hardened(&self, index: u32) -> Result<Self, String> {
        if index >= 1 << 31 {
            return Err("Ironwood child index must be below 2^31".to_owned());
        }
        let child_index = index | (1 << 31);
        let index_bytes = child_index.to_le_bytes();

        let mut hasher = Params::new()
            .hash_length(64)
            .personal(EXPAND_PERSONALIZATION)
            .to_state();
        hasher.update(&self.chain_code);
        hasher.update(&[0x81]);
        hasher.update(self.sk.to_bytes());
        hasher.update(&index_bytes);
        let hash = hasher.finalize();

        let mut sk_bytes = [0; 32];
        sk_bytes.copy_from_slice(&hash.as_bytes()[..32]);
        let sk = SpendingKey::from_bytes(sk_bytes);
        if !bool::from(sk.is_some()) {
            return Err("Child derived an invalid Ironwood spending key".to_owned());
        }
        let sk = sk.unwrap();
        let mut chain_code = [0; 32];
        chain_code.copy_from_slice(&hash.as_bytes()[32..]);

        // ZIP-32 authenticates the parent in the serialized child metadata.
        let fvk = FullViewingKey::from(&self.sk);
        let tag_hash = Params::new()
            .hash_length(32)
            .personal(FVK_TAG_PERSONALIZATION)
            .hash(&fvk.to_bytes());
        let mut parent_fvk_tag = [0; 4];
        parent_fvk_tag.copy_from_slice(&tag_hash.as_bytes()[..4]);

        Ok(Self {
            depth: self
                .depth
                .checked_add(1)
                .ok_or("Ironwood derivation depth overflow")?,
            parent_fvk_tag,
            child_index,
            chain_code,
            sk,
        })
    }

    fn to_bytes(&self) -> [u8; 73] {
        let mut bytes = [0; 73];
        bytes[0] = self.depth;
        bytes[1..5].copy_from_slice(&self.parent_fvk_tag);
        bytes[5..9].copy_from_slice(&self.child_index.to_le_bytes());
        bytes[9..41].copy_from_slice(&self.chain_code);
        bytes[41..].copy_from_slice(self.sk.to_bytes());
        bytes
    }

    fn extended_fvk_bytes(&self) -> [u8; 137] {
        let mut bytes = [0; 137];
        bytes[..41].copy_from_slice(&self.to_bytes()[..41]);
        bytes[41..].copy_from_slice(&FullViewingKey::from(&self.sk).to_bytes());
        bytes
    }
}

fn encode(hrp: &str, bytes: &[u8]) -> Result<String, String> {
    let hrp = Hrp::parse(hrp).map_err(|e| e.to_string())?;
    bech32::encode::<Bech32>(hrp, bytes).map_err(|e| e.to_string())
}

/// Derive a full-node compatible m/32'/coin_type'/account' mainnet account.
fn key_from_account(
    seed: &[u8],
    account: u32,
    coin_type: u32,
) -> Result<ExtendedSpendingKey, String> {
    ExtendedSpendingKey::master(seed)?
        .derive_hardened(32)?
        .derive_hardened(coin_type)?
        .derive_hardened(account)
}

pub fn derive_account(
    seed: &[u8],
    account: u32,
    coin_type: u32,
) -> Result<IronwoodAccount, String> {
    let xsk = key_from_account(seed, account, coin_type)?;
    let fvk = FullViewingKey::from(&xsk.sk);
    let address = encode(
        "pirate",
        &fvk.address_at(0u32, Scope::External).to_raw_address_bytes(),
    )?;
    let diversified = (1u32..=5)
        .map(|index| {
            encode(
                "pirate",
                &fvk.address_at(index, Scope::External)
                    .to_raw_address_bytes(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(IronwoodAccount {
        address,
        diversified,
        viewing_key: encode("pirate-extended-viewing-key", &xsk.extended_fvk_bytes())?,
        private_key: encode("pirate-secret-extended-key", &xsk.to_bytes())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_and_address_payloads_round_trip() {
        let seed: Vec<u8> = (0..32).collect();
        let account = derive_account(&seed, 0, 141).unwrap();
        let (hrp, xsk) = bech32::decode(&account.private_key).unwrap();
        assert_eq!(hrp.as_str(), "pirate-secret-extended-key");
        assert_eq!(xsk.len(), 73);
        assert_eq!(xsk[0], 3);
        assert_eq!(u32::from_le_bytes(xsk[5..9].try_into().unwrap()), 1 << 31);
        let (hrp, xfvk) = bech32::decode(&account.viewing_key).unwrap();
        assert_eq!(hrp.as_str(), "pirate-extended-viewing-key");
        assert_eq!(xfvk.len(), 137);
        assert_eq!(&xsk[..41], &xfvk[..41]);
        let imported_sk = SpendingKey::from_bytes(xsk[41..].try_into().unwrap()).unwrap();
        let imported_fvk = FullViewingKey::from_bytes(&xfvk[41..].try_into().unwrap()).unwrap();
        assert_eq!(
            FullViewingKey::from(&imported_sk).to_bytes(),
            imported_fvk.to_bytes()
        );
        assert_eq!(
            encode(
                "pirate",
                &imported_fvk
                    .address_at(0u32, Scope::External)
                    .to_raw_address_bytes()
            )
            .unwrap(),
            account.address
        );
        for address in std::iter::once(&account.address).chain(account.diversified.iter()) {
            let (hrp, raw) = bech32::decode(address).unwrap();
            assert_eq!(hrp.as_str(), "pirate");
            assert_eq!(raw.len(), 43);
            assert!(bool::from(
                orchard::Address::from_raw_address_bytes(&raw.try_into().unwrap()).is_some()
            ));
        }
    }

    #[test]
    fn matches_official_zip32_orchard_vectors() {
        // Official ZIP-32 Orchard seed 00..1f, m through m/1'/2'/3'.
        let expected = [
            "000000000000000000ab8b7a00509ef20e469b5292b61d474b7cffcb1657924cda720250ae405266777eee3c1017870990a3dd6891b82f80be8976c1e7dc20d60817a5e88e8b2cd4b8",
            "01ff4cda50010000806a041dfb9cfebee97cb1854fdc481cc04f02c9577aa6f13b2c445b80a9669a2298d703fcb40504c95b3b6ed10ecd50082cff97dfd1dd9aa0913c78f977c962af",
            "0232bbdc92020000806da8b57a36c77ad6412a9dc0115f12aced0ee01c402a0cf0a507cb17fc7bbd1d99afd8894baad58784d0ec08f5148ee2c2a17b2b294b08ef9e0a0cf14bcc0920",
            "0336a57c4f03000080b196e9b5809d76577a8944c3f8c8a83f93f0c8f5ace6e7bc9ce4396c034d93fe96439ea348a4b2ce4ec7beb4543c70274c8f76495d60c5fa5f018b68f3c32367",
        ];
        let seed: Vec<u8> = (0..32).collect();
        let mut key = ExtendedSpendingKey::master(&seed).unwrap();
        assert_eq!(hex::encode(key.to_bytes()), expected[0]);
        for index in 1..=3 {
            key = key.derive_hardened(index).unwrap();
            assert_eq!(hex::encode(key.to_bytes()), expected[index as usize]);
        }
        let standard = SpendingKey::from_zip32_seed(&seed, 141, zip32::AccountId::ZERO).unwrap();
        assert_eq!(
            key_from_account(&seed, 0, 141).unwrap().sk.to_bytes(),
            standard.to_bytes()
        );
    }
}
