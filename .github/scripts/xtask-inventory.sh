#!/usr/bin/env bash
# Which `cargo xtask` subcommands exist in this checkout, as step outputs `<subcommand>=true`.
#
# Workflow steps for subcommands that land in later milestones are guarded with
# `if: steps.xtask.outputs.<subcommand> == 'true'`. A subcommand whose milestone has been reached
# (current_milestone in tools/xtask/test-trace.toml) but that is missing fails this step, so a
# guard can never hide a regression. Milestones follow the cases in tools/TESTS.md.
set -euo pipefail

declare -A due=(
  [ci]=M1 [decoupling-gate]=M1 [deps]=M1 [hot-check]=M1 [gen-schema]=M1 [doc-check]=M1
  [test-trace]=M1 [journal-verify]=M1 [setup]=M1 [fixtures]=M1
  [bench-gate]=M1.5
  [scaffold-gate]=M2 [replay-verify]=M2 [dst]=M2 [new-strategy]=M2
  [gen-host]=M3 [fuzz]=M3
  [path-digest]=M6
)
order=(M1 M1.5 M2 M3 M4 M5 M6 M7 M8)
rank() { local i; for i in "${!order[@]}"; do [[ ${order[$i]} == "$1" ]] && { echo "$i"; return; }; done; echo 99; }

current=$(sed -n 's/^current_milestone *= *"\(.*\)".*/\1/p' tools/xtask/test-trace.toml 2>/dev/null || true)
current=${current:-M1}

# `cargo xtask help` prints "  <name>  <what>" lines under "subcommands:". A build failure of xtask
# fails here (set -e), never as "all subcommands missing".
help=$(cargo xtask help)
mapfile -t have < <(printf '%s\n' "$help" | awk '/^subcommands:/{f=1;next} f&&/^  [a-z]/{print $1} f&&/^$/{f=0}')

out=${GITHUB_OUTPUT:-/dev/stdout}
summary=${GITHUB_STEP_SUMMARY:-/dev/stderr}
missing=()
{
  echo "### xtask subcommands (current milestone $current)"
  echo
} >>"$summary"
for name in "${have[@]}"; do echo "$name=true" >>"$out"; done
for name in "${!due[@]}"; do
  if printf '%s\n' "${have[@]}" | grep -qx -- "$name"; then
    echo "- \`$name\`: present" >>"$summary"
  elif (( $(rank "${due[$name]}") <= $(rank "$current") )); then
    missing+=("$name")
    echo "- \`$name\`: **missing** (due ${due[$name]})" >>"$summary"
  else
    echo "- \`$name\`: not yet (lands in ${due[$name]}; its steps are skipped)" >>"$summary"
  fi
done
if ((${#missing[@]})); then
  echo "::error::xtask subcommands due by $current are missing: ${missing[*]}"
  exit 1
fi
