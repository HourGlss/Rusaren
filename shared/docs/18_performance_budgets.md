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
- Soak and load scenarios should exercise repeated match loops and lobby churn under `game_api`.
- Hosted smoke probes should verify the public path after every deploy and on a recurring timer.
- Prometheus metrics should be used to confirm uptime, tick timing, ingress rejection rate, and websocket health on the real host.

## Release honesty
- The budget numbers above are now the intended `0.9` targets.
- The full automated gate is still incomplete until the repeatable load harness measures CPU, memory, connection capacity, and SQLite log latency directly from the reference environment.
