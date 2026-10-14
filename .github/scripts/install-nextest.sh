#!/usr/bin/env bash
# Install the pinned cargo-nextest release binary into ~/.cargo/bin after checking its SHA-256
# (PR fast lane, docs/development.md#质量门quality-gatesyml). Bump version and digest together.
set -euo pipefail

ver=0.9.146
sha256=682c21b777c333e96fd532e114d3a5a894e0729ab88d94c0a9f20f8419695428
f=cargo-nextest-${ver}-x86_64-unknown-linux-gnu.tar.gz
tmp=${RUNNER_TEMP:-/tmp}
curl -fsSL --retry 3 -o "$tmp/$f" "https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-${ver}/$f"
echo "$sha256  $tmp/$f" | sha256sum -c -
mkdir -p "$HOME/.cargo/bin"
tar -xzf "$tmp/$f" -C "$HOME/.cargo/bin" cargo-nextest
cargo nextest --version
