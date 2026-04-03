# Performance Budgets

## Goal
`0.9.0` treats performance as a release gate, not a vague aspiration.
The server should have explicit budgets for tick latency, command latency, CPU, memory, connection count, and the future SQLite-backed event log writes.

## Reference environments
- CI reference environment: GitHub Actions `ubuntu-latest` runner. Use this for repeatable code-level regressions and budget gating.
- Hosted reference environment: Ubuntu 24.04 on a Linode `4 GB / 2 vCPU` instance. Use this for deploy smoke, synthetic probes, and live-host sanity checks.

## Current budget targets
- Simulation tick latency:
  - `p95 <= 4 ms`
  - `p99 <= 8 ms`
  - worst observed tick during routine smoke or soak `<= 16 ms`
- Command latency from accepted ingress packet to emitted server response:
  - `p95 <= 50 ms`
  - `p99 <= 100 ms`
- CPU on the hosted reference environment:
  - steady-state idle stack `<= 25%` of one core
  - active local playtest with one live match `<= 70%` total CPU
- Memory:
  - backend RSS after warm startup `<= 350 MiB`
  - whole compose stack on the hosted reference environment `<= 2.0 GiB`
- Connection capacity:
  - `100` idle websocket sessions without health degradation
  - `10` simultaneous active matches without health degradation
- SQLite event logging:
  - `p95 <= 10 ms`
  - `p99 <= 25 ms`
  - zero dropped writes during a normal match

## How to test these budgets
- Criterion benchmarks cover micro hot paths in `game_sim` and `game_net`.
- `./scripts/quality.ps1 soak` now runs the fixed-reference `game_api` soak and performance-budget gate suite.
- The current reference gate exercises:
  - `100` idle clients
  - `10` simultaneous matches
  - command latency percentiles
  - simulation tick percentiles
  - SQLite combat-log append and query percentiles
  - backend RSS on Linux CI or Linux reference hosts
- Hosted smoke probes verify the public path after every deploy, and Linode setup now installs recurring hosted smoke and liveprobe timers.
- Prometheus metrics and `/adminz?format=json` confirm uptime, tick timing, ingress rejection rate, websocket health, and combat-log timing on the real host.

## Release honesty
- The budget numbers above are now the enforced `0.9.6` targets for the fixed reference environment.
- Whole-stack CPU on the hosted reference environment still needs to be checked through Prometheus, Docker stats, and the hosted diagnostics bundle rather than the CI-only gate.
