# ADR-0002: Integration transport in TypeScript on Bun, isolated process

- Status: Proposed
- Date: 2026-10-12
- Relates to: [technology.md](../phase-0/technology.md), [security.md](../phase-0/security.md)

## Context

Model providers change fastest at the transport layer. That code must be easy
to update, but it operates on the most untrusted data and must never hold
authority.

## Decision

Provider integrations live in `gateway/`, TypeScript on Bun, in a separate
container: nonroot, read-only image, no database credential, no workspace
mount, no execution endpoint. The gateway normalizes provider events into the
provider-neutral streaming contract in [protocol.md](../phase-0/protocol.md).

## Alternatives considered

- Node.js: workable, but Bun matches the owner baseline and ships a simple
  streaming HTTP surface.
- AI SDK / provider SDKs: faster per provider; rejected for the initial slice
  because opaque retry and tool-call semantics blur the abort, timeout and
  backpressure behavior the core must observe.

## Consequences

- Transport behavior (abort, timeouts, backpressure) needs explicit tests on
  Bun before claiming streaming works (PR-3 scope).
- Gateway compromise is contained by isolation; AC-24 tests that containment.
- Cross-language contract drift is mitigated by validating both sides against
  `contracts/schemas/`.

## Evidence required

Bun benchmark of streaming adapter abort under load; schema-conformance tests
passing for both a fake provider and each qualified provider.
