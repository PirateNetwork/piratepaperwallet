//! Legacy Sapling account derivation for wallets created before Ironwood.
//!
//! The caller chooses either the 32-byte paper-wallet HD seed or its 64-byte
//! BIP-39 seed. These two inputs intentionally produce different accounts.

use bech32::{Bech32, Hrp};
use sapling::zip32::ExtendedSpendingKey;
use zip32::{ChildIndex, DiversifierIndex};

/// A Sapling ZIP-32 account and its first five additional receive addresses.
pub struct SaplingAccount {
    pub address: String,
    pub diversified: Vec<String>,
    pub viewing_key: String,
    pub private_key: String,
}

fn encode(hrp: &str, data: &[u8]) -> Result<String, String> {
    let hrp = Hrp::parse(hrp).map_err(|e| format!("Invalid Sapling HRP: {e}"))?;
    bech32::encode::<Bech32>(hrp, data).map_err(|e| format!("Failed to encode Sapling value: {e}"))
}

/// Derive `m/32'/coin_type'/account'` using the Pirate Sapling ZIP-32 fork.
///
/// The returned default address and five additional addresses all belong to
/// the external key. Invalid diversifier indices are skipped, as ZIP-32
/// requires; the full 88-bit index is incremented without wrapping.
pub fn derive_account(seed: &[u8], account: u32, coin_type: u32) -> Result<SaplingAccount, String> {
    if seed.len() != 32 && seed.len() != 64 {
        return Err("Sapling seed must contain 32 or 64 bytes".to_string());
    }
    if account >= 1 << 31 || coin_type >= 1 << 31 {
        return Err("Sapling ZIP-32 child indices must be below 2^31".to_string());
    }

    let xsk = ExtendedSpendingKey::from_path(
        &ExtendedSpendingKey::master(seed),
        &[
            ChildIndex::hardened(32),
            ChildIndex::hardened(coin_type),
            ChildIndex::hardened(account),
        ],
    );
    // The xFVK export contains ZIP-32 depth, parent tag, child index, and
    // chain code. A modern diversifiable FVK omits that import metadata.
    #[allow(deprecated)]
    let xfvk = xsk.to_extended_full_viewing_key();

    let (default_index, default_address) = xfvk.default_address();
    let address = encode("zs", &default_address.to_bytes())?;

    let mut diversified = Vec::with_capacity(5);
    let mut next_index: DiversifierIndex = default_index;
    for _ in 0..5 {
        next_index
            .increment()
            .map_err(|_| "Sapling diversifier index space exhausted".to_string())?;
        let (found_index, found_address) = xfvk
            .find_address(next_index)
            .ok_or_else(|| "Sapling diversifier index space exhausted".to_string())?;
        diversified.push(encode("zs", &found_address.to_bytes())?);
        next_index = found_index;
    }

    let mut xfvk_bytes = Vec::with_capacity(169);
    xfvk.write(&mut xfvk_bytes)
        .map_err(|e| format!("Failed to serialize Sapling viewing key: {e}"))?;

    Ok(SaplingAccount {
        address,
        diversified,
        viewing_key: encode("zxviews", &xfvk_bytes)?,
        private_key: encode("secret-extended-key-main", &xsk.to_bytes())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_seed_and_child_indices() {
        assert!(derive_account(&[0; 31], 0, 141).is_err());
        assert!(derive_account(&[0; 32], 1 << 31, 141).is_err());
        assert!(derive_account(&[0; 32], 0, 1 << 31).is_err());
    }

    #[test]
    fn derived_addresses_are_distinct_and_keys_match_the_network() {
        let account = derive_account(&[7; 32], 0, 141).unwrap();
        assert!(account.address.starts_with("zs1"));
        assert!(account.viewing_key.starts_with("zxviews1"));
        assert!(account.private_key.starts_with("secret-extended-key-main1"));
        assert_eq!(account.diversified.len(), 5);

        let mut addresses = vec![account.address];
        addresses.extend(account.diversified);
        addresses.sort();
        addresses.dedup();
        assert_eq!(addresses.len(), 6);
    }

    #[test]
    fn preserves_legacy_paper_wallet_account_vector() {
        // From the original paper wallet test added in f5f4e90. At that time
        // the application used raw HD seeds and coin type 133.
        let seed: [u8; 32] =
            hex::decode("023241db228975d6703d34e1cc900c66f63fa4b512894c10faeadbf42e109fc4")
                .unwrap()
                .try_into()
                .unwrap();
        let account = derive_account(&seed, 0, 133).unwrap();
        assert_eq!(
            account.address,
            "zs19qjkhwjzz03h4p3g0rca50tgeznuhzw9773m8ur64mtaqccyflgdhjsg0fgsxt0m3ljvs73rmc0"
        );
        assert_eq!(
            account.viewing_key,
            "zxviews1qvjtyprtqqqqpqyhjrw9eg0hhj7a3mfqae4vfnew6fmsj8a6qlssptk0n4lvz3dhk8p0uu6wvey6u479stpenfjjmsqf8udtjurx8d8ya4rj4l2pf4hxeg63ksf7rqtszg6chm7f00f4z9td7cn6a98sawm3u77hhlpqj6awq5zfjkfz97nmdtdrsdmz44murgm3ck3ra4ph4y9969js5vydh2xqe73z0zu6z2jydq9z2fzgfc5r0f7dyw9qkmw56wpccfc0lcrskmctxn48x"
        );
    }
}
