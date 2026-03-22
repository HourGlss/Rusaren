# ADR 0003: Server-Authored Event Logging

## Context
Gameplay debugging, moderation, replay-style regression, and production forensics all need durable records of what the server believed happened.
Client-side logs are not authoritative enough for that role.

## Decision
The long-term log source of truth will be server-authored match and combat events.
The first persistence target is SQLite so local development, CI, and single-host deploys share the same behavior.

## Consequences
- Non-movement gameplay actions should be modeled as append-only server events.
- Replay and regression work can grow from the same event source instead of ad hoc snapshots.
- This decision raises the importance of write latency budgets and schema stability.
