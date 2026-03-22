# ADR 0001: Freeze The 1.0 Protocol Surface

## Context
The server now has real websocket signaling, WebRTC gameplay transport, and authored snapshot payloads.
Past this point, accidental packet or signaling drift becomes a deployment and compatibility risk rather than a local refactor inconvenience.

## Decision
Treat packet headers, control packets, snapshot payloads, and signaling message shapes as versioned public contracts.
Protocol changes after the `0.9` freeze should require explicit compatibility review, fixtures, and targeted regression tests.

## Consequences
- Network changes must be deliberate and test-backed.
- Golden fixtures and compatibility suites become release-critical.
- Clean refactors are still allowed, but behavior-preserving wire compatibility is the default.
