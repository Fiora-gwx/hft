#!/usr/bin/env bash
# Free disk on a hosted x86_64 runner before the `rust` job's build (docs/development.md
# "质量门"): the ci-profile test tree, the aarch64 check and the benchmark links fill the ~20 GB
# free on `/` of the stock image, and a linker whose output file hits ENOSPC dies with SIGBUS
# (rust-lld "signal 7 [Bus error]"). Removes preinstalled toolchains the job never uses; the
# removal runs in the background so the build is not delayed (done long before the links that
# need the space). Prints `df -h /` before; the job prints it again before the benchmark smoke.
set -euo pipefail

df -h /
dirs=(
  /usr/local/lib/android
  /usr/share/dotnet
  /opt/ghc
  /usr/local/.ghcup
  /opt/hostedtoolcache/CodeQL
  /usr/local/share/powershell
  /usr/local/share/chromium
  /usr/share/swift
  /usr/local/julia*
)
for d in "${dirs[@]}"; do
  if [[ ! -e $d ]]; then echo "free-disk: $d not present"; fi
done
# setsid + nohup: detached from the step's process group, so the step returns at once.
sudo setsid nohup rm -rf "${dirs[@]}" >/dev/null 2>&1 </dev/null &
echo "free-disk: removal of ${#dirs[@]} preinstalled directories started in the background"
