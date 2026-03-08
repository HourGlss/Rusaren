# Selective Verus Strategy

Verus should not be used across this entire repo. It is best treated as a scalpel for a few small, high-value modules with hard invariants.

## Where Verus fits

Verus is a verifier for Rust code, but its own docs say it currently supports only a subset of Rust and does not intend to support all Rust features and libraries. It is also under active development.

That makes it a poor fit for:
- end-to-end async networking stacks
- Tokio or QUIC integration layers
- broad application code with lots of framework glue
- code that needs to depend directly on large external crates without tight wrappers

It is a good fit for:
- pure parsing and validation logic
- packet/frame layout invariants
- sequence-number and replay-window math
- rate-limit and quota accounting
- fixed-size buffer manipulations
- small deterministic state machines with security-sensitive preconditions

## Recommended boundary

Use ordinary Rust for transport adapters and runtime integration:
- socket or QUIC libraries
- async tasks and runtime glue
- serialization framework integration
- database, HTTP, and service plumbing

Wrap those edges with small, mostly-pure modules that own the critical invariants:
- `game_net::sequence_window`
- `game_net::frame_codec`
- `game_net::input_validation`
- `game_net::rate_limit`
- `game_content::validation` for content graph invariants shared with untrusted inputs

Those inner modules are the right candidates for Verus.

## How to integrate it safely

Start with separate crates or modules that have:
- few external dependencies
- no async runtime coupling
- explicit preconditions and postconditions
- deterministic inputs and outputs

For code that must call into external crates or unverified wrappers, Verus provides `#[verifier::external]`, `#[verifier::external_body]`, and `assume_specification`. Use those only at narrow boundaries and keep the trusted surface small.

## First candidates in this repo

`game_net`
- sequence number monotonicity
- ack/range tracking
- packet size bounds
- delta decode preconditions
- input-command admissibility checks

`game_content`
- skill prerequisite graph validity
- no cycles
- tier progression invariants
- map-content validation invariants

`game_sim`
- only for small arithmetic or state-machine kernels, not the whole tick loop

V1 scope decision:
- Verus is limited to small `game_net` invariant modules in v1.
- Do not extend Verus into broad content validation or simulation logic yet.

## What not to do first

Do not start by trying to verify:
- the entire network stack
- Tokio tasks
- QUIC integration
- all of `game_sim`
- generic ECS plumbing

That will turn Verus into schedule risk instead of risk reduction.
