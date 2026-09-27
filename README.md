# Pirate Chain paper wallet

An offline command line generator for Pirate Chain shielded addresses. New wallets
use **Ironwood**. Use `--pool sapling` to recover legacy Sapling paper wallets.
The generator never connects to the chain or requires proving parameters.

## Ironwood activation and recovery

Ironwood addresses start with `pirate1`. They can be generated offline in advance,
but can receive funds only after the network activates Ironwood. Mainnet activation
occurs 60 blocks after the first block timestamp crossing **October 3, 2026,
19:00 UTC**; an offline clock cannot determine the activation block.

Derivation follows ZIP-32 `m/32'/141'/account'`. Each account has one default
address and five additional external receive addresses, all controlled by the
same spending key. Those addresses do not provide separate accounts or balances.

To spend, import the printed `pirate-secret-extended-key1...` extended spending
key into an Ironwood-enabled wallet and rescan from before the first payment.
In the full node, use `z_importkey` and allow the rescan to finish. The
`pirate-extended-viewing-key1...` key provides viewing access, not spending access;
keep it private because it reveals wallet activity.

The recovery phrase contains 24 English BIP39 words. Keep the words in their
original order to restore your wallet. **HDSeed** is the original 32-byte seed
represented by those words. **Bip39Seed** is the 64-byte seed derived from the
phrase and used to generate the account keys. Wallets generated with `--nobip39`
use **HDSeed** directly and must be restored with that option. You can also
restore an individual account by importing its extended spending key.

## Download and generate

Get signed platform archives from this repository's [releases](../../releases).
Linux x86_64 and ARM64, Windows x86_64, and macOS Intel and Apple Silicon are
built automatically. Linux binaries require glibc 2.35 or later.

```sh
./piratepaperwallet
./piratepaperwallet -z 3 --format pdf wallet.pdf
./piratepaperwallet -e "optional extra entropy" -o wallet.json
```

Random generation requires OS randomness. Optional typed entropy is mixed with it;
generation fails if the OS entropy source fails. `--nohd` creates an independent
seed for each wallet. Output files contain spending secrets: store them offline
and securely. The CLI refuses to overwrite existing files; Unix output files are
created with owner-only read/write permissions. PDF output uses two pages per
account: receiving/viewing and private spending/recovery. Keep both pages private.

## Recover existing paper wallets

```sh
./piratepaperwallet -p "your complete 24 word phrase here" -z 3
./piratepaperwallet -s YOUR_64_CHARACTER_HDSEED_HEX -z 3
./piratepaperwallet --pool sapling -p "your legacy 24 word phrase here"
./piratepaperwallet --pool sapling --nobip39 -s YOUR_HDSEED_HEX
```

Preserve the original pool, coin type (`--cointype`), account count, and BIP39
mode. Restoring a Sapling seed as Ironwood creates new Ironwood addresses; it does
not move the old Sapling balance. Spend or transfer the old funds with a wallet
supporting that pool. `--partialphrase` searches valid 24-word phrases with one
missing word; it can return many candidates. Identify the correct one using a
previously saved address before receiving funds.

## Vanity addresses

```sh
./piratepaperwallet --vanity arrr --threads 4
```

The prefix is the part after `pirate1` (or `zs1` for Sapling). Use lowercase Bech32
data characters: `qpzry9x8gf2tvdw0s3jn54khce6mua7l`. Vanity generation searches
fresh account seeds so the returned phrase and spending key recover the exact
printed address. Each added character multiplies the expected search by 32;
long prefixes can take a very long time.

## Build and test

Install Rust, then run from the repository root:

```sh
cargo build --release --locked -p piratepaperwallet
cargo test --workspace --locked
```

The executable is in `target/release`. Tests include official ZIP-32 vectors,
full-node address/key fixtures, phrase recovery, and legacy Sapling recovery. Run `--help` for all
options. On Linux, `unshare -n ./target/release/piratepaperwallet` can enforce
running without a network interface.
