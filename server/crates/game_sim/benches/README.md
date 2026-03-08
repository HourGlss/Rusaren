# Criterion Benchmarks

`game_sim` is the first crate that should grow meaningful Criterion benchmarks because it will own the fixed-tick server loop and the most performance-sensitive state transitions.

There is intentionally no fake benchmark target here yet. Add a Criterion benchmark when one of these exists:
- a stable tick function or scheduler entrypoint
- line-of-sight or fog-of-war calculations
- snapshot or delta packing code
- hot-path modifier or effect resolution

When that happens, add `criterion` as a `dev-dependency` in this crate and a `[[bench]]` target with `harness = false`.
