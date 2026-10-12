# ADR-0004: Claude Messages API as hosted transport candidate

- Status: Proposed
- Date: 2026-10-12
- Relates to: [technology.md](../phase-0/technology.md)

## Context

The slice needs one hosted inference provider behind the gateway contract. The
choice must stay configuration, not architecture, so qualifying a second
provider (AC-29) does not require core changes.

## Decision

Implement the hosted adapter against the Claude Messages API streaming and
tool-use contract as the *candidate*. Provider, model and configuration live in
deployment configuration; the core never references provider-specific shapes.

## Alternatives considered

- Other independently qualified hosted APIs: viable; selection waits on the
  evaluation protocol, not on this document.
- Server-side assistant tool execution: rejected outright; it would bypass
  core-held authorization.

## Consequences

- Data-egress policy must be explicit per deployment; fallback only within the
  same policy (AC-16).
- Credentials live only in the gateway container configuration.
- No leaderboard position is encoded in configuration; qualification requires a
  live test per the evaluation protocol.

## Evidence required

Recorded live evaluation run (tool precision, success rate, first-token/total
latency p50/p95, token usage, cost) on the declared reference configuration.
