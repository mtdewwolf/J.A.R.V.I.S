# ADR-0006: Authenticated Rust CLI as first client

- Status: Proposed
- Date: 2026-10-12
- Relates to: [technology.md](../phase-0/technology.md), [security.md](../phase-0/security.md)

## Context

The first milestone needs the smallest complete client that still exercises
every backend boundary: authentication, approval presentation, replay and
recovery. Cinematic interfaces are explicitly later.

## Decision

The initial client is a Rust CLI that authenticates with an owner-generated
bearer credential stored with restrictive file permissions. Approval screens
show exact arguments, scope, risk, expiry and the one-use constraint.

## Alternatives considered

- Web client first: faster to make attractive, but adds origin, cookie and
  pairing surface before the backend deserves exposure.
- Desktop app first: prematurely commits a UI framework before benchmarks
  (GPUI versus Tauri remain deferred).

## Consequences

- CLI rejects unknown Host and Origin values like any other client; loopback is
  a network restriction, not identity.
- The protocol must be versioned so later clients (Phase 6) consume the same
  contracts.
- Approval UX is plain-text but complete: exact canonical argument digest, tool
  and version, resource, and provider disclosure.

## Evidence required

End-to-end slice passing with the CLI, including an injection string displayed
  as data and never as an instruction.
