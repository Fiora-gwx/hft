#!/usr/bin/env bash
# Build the aarch64 release binaries (docs/development.md#发布工作流release-aarch64yml step 4,
# profiles: #构建剖面) and check each one is an aarch64 executable.
#   channel release: hft-core and kill-switch with --profile release-hot, every other binary with
#                    --release;
#   channel dev:     everything with --release.
# Binaries are those of the workspace members (xtask excluded); apps land with their milestones.
# Usage: build-release.sh <release|dev>     (cross linker env is set by the caller when needed)
set -euo pipefail

channel=${1:?channel}
target=aarch64-unknown-linux-gnu
mapfile -t members < <(cargo metadata --no-deps --format-version 1 |
  python3 -c 'import json,sys; [print(p["name"]) for p in json.load(sys.stdin)["packages"]]')
has() { printf '%s\n' "${members[@]}" | grep -qx -- "$1"; }

hot=()
for p in hft-core kill-switch; do
  if has "$p"; then hot+=("$p"); fi
done

excl=(--exclude xtask)
if [[ $channel == release ]] && ((${#hot[@]})); then
  args=()
  for p in "${hot[@]}"; do args+=(-p "$p"); excl+=(--exclude "$p"); done
  if has hft-strategy-registry; then args+=(-p hft-strategy-registry --all-features); fi
  cargo build --locked --profile release-hot --target "$target" "${args[@]}"
fi
cargo build --locked --release --target "$target" --workspace --all-features --bins "${excl[@]}"

n=0
for dir in "target/$target/release" "target/$target/release-hot"; do
  [[ -d $dir ]] || continue
  while IFS= read -r -d '' bin; do
    file "$bin" | tee /dev/stderr | grep -q 'ARM aarch64'
    n=$((n + 1))
  done < <(find "$dir" -maxdepth 1 -type f -executable -print0)
done
echo "$n aarch64 executable(s) checked"
