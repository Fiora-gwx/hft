#!/usr/bin/env bash
# Hosted CI only. Executables are never run: systemd-analyze checks path existence.
set -euo pipefail
[[ ${CI:-} == true ]] || { echo 'CI fixture only' >&2; exit 1; }
for user in hft hft-guard; do
  id "$user" >/dev/null 2>&1 || useradd --system --no-create-home "$user"
done
for group in hft-ctl hft-stopmark hft-journal-ro; do
  getent group "$group" >/dev/null || groupadd --system "$group"
done
for name in hft-core hft-secrets; do
  install -D -m 0755 /usr/bin/true "/opt/hft/current-core/bin/$name"
done
for name in deploy/bin/hft-shm-release deploy/release/preflight.sh deploy/imds/hft-imds-lock.sh; do
  install -D -m 0755 /usr/bin/true "/opt/hft/current-core/$name"
done
