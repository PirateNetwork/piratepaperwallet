#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

if [[ $# -ne 2 || "$1" != "--version" || -z "$2" ]]; then
    echo "Usage: $0 --version VERSION" >&2
    exit 2
fi

version="$2"
if [[ ! "$version" =~ ^[0-9A-Za-z][0-9A-Za-z.+-]*$ ]]; then
    echo "Invalid release version: $version" >&2
    exit 2
fi
if [[ -z "${PIRATE_GPG_PASSPHRASE:-}" ]]; then
    echo "PIRATE_GPG_PASSPHRASE is required for release signing" >&2
    exit 1
fi

key_fingerprint="$(gpg --show-keys --with-colons --fingerprint public_key.asc | awk -F: '$1 == "fpr" { print $10; exit }')"
secret_fingerprint="$(gpg --list-secret-keys --with-colons --fingerprint "$key_fingerprint" | awk -F: '$1 == "fpr" { print $10; exit }')"
if [[ -z "$key_fingerprint" || "$secret_fingerprint" != "$key_fingerprint" ]]; then
    echo "The imported signing key does not match public_key.asc" >&2
    exit 1
fi

cd release
archives=()
for platform in linux-x86_64 linux-aarch64 windows-x86_64 macos-arm64 macos-x86_64; do
    archive="piratepaperwallet-$platform-v$version.zip"
    if [[ ! -f "$archive" ]]; then
        echo "Missing release archive: $archive" >&2
        exit 1
    fi
    archives+=("$archive")
done

signatures_dir="signatures-v$version"
if [[ -e "$signatures_dir" || -e "$signatures_dir.zip" ]]; then
    echo "Signature output already exists: $signatures_dir" >&2
    exit 1
fi
mkdir "$signatures_dir"

sha256sum "${archives[@]}" | sort -k2,2 > "$signatures_dir/sha256sum-v$version.txt"
for archive in "${archives[@]}"; do
    gpg --batch --yes --pinentry-mode loopback --passphrase-fd 3 \
        --local-user "$key_fingerprint" \
        --output "$signatures_dir/$archive.sig" --detach-sig "$archive" \
        3<<< "$PIRATE_GPG_PASSPHRASE"
done

gpg --batch --yes --pinentry-mode loopback --passphrase-fd 3 \
    --local-user "$key_fingerprint" \
    --output "$signatures_dir/sha256sum-v$version.txt.sig" \
    --detach-sig "$signatures_dir/sha256sum-v$version.txt" \
    3<<< "$PIRATE_GPG_PASSPHRASE"
cp ../SIGNATURES_README "$signatures_dir/README"
(cd "$signatures_dir" && zip -q "../$signatures_dir.zip" ./*)
