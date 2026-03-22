# Architecture Governance

## Goal
Clean code does not stay clean by accident.
The repo needs explicit crate-boundary rules, ADRs for high-risk choices, and a human review checklist that catches coupling or protocol drift before it lands.

## Crate boundary rules
- `game_domain` is pure domain logic. It must not depend on transport, persistence, or HTTP concerns.
- `game_content` owns authored-content parsing and validation. It may feed domain or simulation types, but runtime crates must not duplicate content validation logic.
- `game_sim` is the pure combat engine. It must not depend on HTTP, websocket, WebRTC, or file-system deployment concerns.
- `game_lobby` and `game_match` own phase and flow rules. They must stay transport-agnostic.
- `game_net` owns the wire format, packet validation, and codec boundaries. It must not contain application orchestration.
- `game_api` is the service boundary. It may compose the lower crates, but lower crates must not depend on `game_api`.
- `dedicated_server` is the operational entrypoint. It owns env parsing, process setup, and deployment-facing wiring, not gameplay rules.

## File and module discipline
- Prefer one concept per file and split mixed-purpose files before they become hotspots.
- Keep tests close to their module when that improves readability, but avoid giant mixed prod-and-test files.
- When a module owns both encode and decode paths, split them when fuzzing, mutation testing, or review clarity benefits.

## Human PR review checklist
- Does this change alter the public protocol or signaling surface?
- Does this cross a crate boundary that should stay one-way?
- Does this add new state or side effects without observability?
- Does this add rules or parsing without direct tests?
- Does this change impact deploy, smoke probes, or operator workflows?
- Does this enlarge a hotspot file instead of splitting it?
- Does this introduce logging, persistence, or background work without a failure-mode story?

## ADR expectations
- Write an ADR before changing protocol shape, persistence format, admin/operator surfaces, or gameplay event logging.
- Prefer short ADRs that explain context, decision, consequences, and rejected alternatives.
- Update the relevant ADR when the decision changes instead of letting the code drift silently.
