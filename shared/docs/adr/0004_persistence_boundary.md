# ADR 0004: Single-Host Persistence Boundary For 0.9

## Context
The current hosted stack is intentionally single-host oriented: match ownership is local to one process and player records are local to the host.
Trying to fake multi-node ownership before the persistence model changes would create operational complexity the code does not yet support.

## Decision
Keep `0.9` on a single authoritative backend host with local persistence.
Use that boundary to harden deployment, smoke probes, observability, and protocol stability before introducing distributed ownership.

## Consequences
- Deploy and rollback stay simple enough for a single operator.
- Match and log persistence should be designed with a future migration path, but not forced into premature distribution.
- Hosted validation should focus on one good production-style backend rather than weak multi-node theater.
