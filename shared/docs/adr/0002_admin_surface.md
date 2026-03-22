# ADR 0002: Private Read-Only Admin Surface

## Context
The hosted backend needs operator visibility into health, sessions, tick timing, and ingress behavior without exposing internal state publicly.
The fastest safe `0.9` path is a narrow read-only surface rather than a write-capable admin API.

## Decision
Expose a password-protected `/adminz` page from the backend itself.
Protect it with HTTP Basic auth supplied by deploy-time environment variables and keep the page read-only.

## Consequences
- Operators get a simple status page without introducing a second admin service.
- The deploy stack must carry private admin credentials.
- Future admin features should continue to default to read-only and auditable behavior.
