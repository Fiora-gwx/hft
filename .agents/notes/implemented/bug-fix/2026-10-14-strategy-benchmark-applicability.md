# Explicit benchmark applicability for optional strategy interfaces

Status: implemented

## Problem

The full CI benchmark job failed after cross-taker registration: it required all seven SDK entry names even though this strategy has no slow-side cache, Advice or TargetIntent path. It also lacked benchmarks for its implemented hooks.

## Decision

Measure the real CrossTaker event entry for miss, fire, Exec Broken and Risk HedgeBroken, using the existing registry fixture constants. Reset state outside each timed sample. Declare unsupported optional interfaces in benches/not-applicable.toml with nonempty reasons and expose those declarations in the gate report.

The gate only accepts slow_update, advice and target_set exclusions. Required trading/Exec/Risk hooks, empty reasons, unknown keys and entries that already have benchmarks remain errors. Existing latency limits are unchanged; CI timing remains report-only.

## Alternatives considered

No-op timings would misrepresent unsupported functionality. Ignoring every strategy's coverage during a scoped crate run would hide genuine missing coverage. Requiring new strategy functionality solely for benchmark completeness would change trading behavior.

## Consequences

Reviewers must remove an exclusion when implementing its interface. The tooling contract documents the narrow exception for optional SDK interfaces. This does not change strategy execution or risk semantics. Reference-machine latency acceptance remains outstanding.

## Testing

TOOL-185 covers accepted and invalid declarations and still rejects undeclared missing entries. Cross-taker's benchmark binary smoke exercises all four measured hooks.
