#!/usr/bin/env bash
# Install the pinned stable toolchain named in rust-toolchain.toml (docs/development.md#工具链).
# Works on hosted runners (rustup preinstalled) and in `container: ubuntu:24.04` jobs (installs
# rustup first; the container step before this one installs curl and a C toolchain).
set -euo pipefail

file=rust-toolchain.toml
channel=$(sed -n 's/^channel *= *"\(.*\)".*/\1/p' "$file")
list() { sed -n "s/^$1 *= *\[\(.*\)\].*/\1/p" "$file" | tr -d '" '; }
components=$(list components)
targets=$(list targets)
[[ -n $channel ]] || { echo "::error::no channel in $file"; exit 2; }

if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --profile minimal --default-toolchain none --no-modify-path
  export PATH="$HOME/.cargo/bin:$PATH"
  echo "$HOME/.cargo/bin" >>"$GITHUB_PATH"
fi

args=(--profile minimal --no-self-update)
[[ -n $components ]] && args+=(--component "$components")
[[ -n $targets ]] && args+=(--target "$targets")
rustup toolchain install "$channel" "${args[@]}"
rustc --version
cargo --version
