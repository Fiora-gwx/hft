# Agent Note: Restore the risk run boundary from EngineClock

Status: implemented

## Problem

Full CI found modes seed `0x8573133122fce369` failing INV-6 at step 7999. Replaying from snapshot 8846 omitted the MarginStress CapSet at seq 8970. The retained account-2 report predates the current run; live correctly treats it as stale, while the restored Risk retained its constructor's zero run boundary.

## Decision

After decoding snapshot section 25, restore Risk's run start from its EngineClock base before replaying any tail Input. The next EngineClock Input still establishes the next run through the existing path. This derived value needs no additional snapshot row or format version.

## Alternatives considered

Ignoring the Note mismatch would hide a different risk cap. Replaying from an earlier EngineClock would conceal incomplete checkpoint restoration and make checkpoints depend on preceding history. Persisting another copy of the clock boundary would create two owners for one fact.

## Consequences

Snapshot-based verification and recovery preserve the existing margin freshness and L3 confirmation rules. The recovery correction is marked 待评审 in [changes](../../../../docs/changes.md). The execution continuation format is unrelated to this defect.

## Testing

TK-115 retains the exact failing modes seed, exercises the stale-margin cap and checks every snapshot plus periodic journal reconstruction. The seed is pinned in modes.txt; no HashTrail golden is regenerated.
