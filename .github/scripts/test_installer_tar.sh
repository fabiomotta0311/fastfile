#!/usr/bin/env bash
set -euo pipefail

# Mirror the release staging, archive and clean-install invocation without
# requiring a Rust release build or installing anything outside a temp dir.
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/stage/scripts" "$tmp/extracted" "$tmp/bin"
cp scripts/install.sh "$tmp/stage/scripts/"
cp scripts/install.sh "$tmp/stage/"
for binary in ff fastfile; do
    printf '#!/bin/sh\nexit 0\n' > "$tmp/stage/$binary"
    chmod +x "$tmp/stage/$binary"
done
tar -czf "$tmp/release.tar.gz" -C "$tmp/stage" .
tar -xzf "$tmp/release.tar.gz" -C "$tmp/extracted"
cd "$tmp/extracted"
for installer in ./scripts/install.sh ./install.sh; do
    INSTALL_DIR="$tmp/bin" "$installer"
done
test -x "$tmp/bin/ff"
test -x "$tmp/bin/fastfile"
