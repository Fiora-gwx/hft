#!/usr/bin/env bash
# Python wheels (each package + the hft_py aarch64 wheel), the offline wheelhouse and the merged
# requirements.lock into <dist>/python (docs/development.md#发布工作流release-aarch64yml step 6).
# Packages land with their milestones (hft-py M2, pipeline and ops_bot M4-M5); none -> no-op.
#
# Usage: build-wheels.sh <dist> [--cross]
#   --cross: building on x86_64 for aarch64 (release-x86-cross.yml): maturin --target and a
#            wheelhouse downloaded for the aarch64 platform.
set -euo pipefail

dist=${1:?dist dir}
cross=${2:-}
mapfile -t pkgs < <(git ls-files 'python/*pyproject.toml' 'strategies/*/pipeline/pyproject.toml' |
  xargs -r -n1 dirname | sort -u)
if ((${#pkgs[@]} == 0)) && [[ ! -f crates/hft-py/Cargo.toml ]]; then
  echo "no Python packages yet; no wheels"
  exit 0
fi

out=$dist/python
mkdir -p "$out/wheels" "$out/wheelhouse"
python3 -m venv "${RUNNER_TEMP:-/tmp}/wheel-venv"
# shellcheck disable=SC1091
source "${RUNNER_TEMP:-/tmp}/wheel-venv/bin/activate"
if [[ -f python/requirements-dev.lock ]]; then
  python -m pip install -q -r python/requirements-dev.lock
else
  echo "::warning::python/requirements-dev.lock is missing; installing unpinned build tools"
  python -m pip install -q build maturin
fi

for p in "${pkgs[@]}"; do
  python -m build --wheel --outdir "$out/wheels" "$p"
done
if [[ -f crates/hft-py/Cargo.toml ]]; then
  margs=(build --release --locked --manifest-path crates/hft-py/Cargo.toml --out "$out/wheels")
  if [[ $cross == --cross ]]; then margs+=(--target aarch64-unknown-linux-gnu); fi
  maturin "${margs[@]}"
fi

: >"$out/requirements.lock"
for p in "${pkgs[@]}"; do
  if [[ -f $p/requirements.lock ]]; then grep -v '^\s*#' "$p/requirements.lock" >>"$out/requirements.lock" || true; fi
done
sort -u -o "$out/requirements.lock" "$out/requirements.lock"
if [[ -s $out/requirements.lock ]]; then
  dargs=(--only-binary=:all: --dest "$out/wheelhouse" --requirement "$out/requirements.lock")
  if [[ $cross == --cross ]]; then
    dargs+=(--platform manylinux_2_28_aarch64 --platform manylinux2014_aarch64 --python-version 3.12 --implementation cp)
  fi
  python -m pip download "${dargs[@]}"
fi
