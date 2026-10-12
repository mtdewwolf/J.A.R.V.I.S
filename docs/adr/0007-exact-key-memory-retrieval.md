# ADR-0007: Owner-scoped encrypted key/value retrieval first

- Status: Proposed
- Date: 2026-10-12
- Relates to: [architecture.md](../phase-0/architecture.md) "Memory, privacy and retrieval"

## Context

Confirmed semantic memory must survive restart and be private, but plaintext
full-text indexing over private memories conflicts with the privacy model.

## Decision

Store semantic records as owner/scope + structured key/value with epistemic
status and provenance, encrypted with an application-held key. Retrieval is
bounded exact-key lookup via an owner-scoped HMAC key index, then decryption.
Inference ranking, opt-in non-sensitive full-text and vector alternatives are
separate experiments before any selection.

## Alternatives considered

- PostgreSQL FTS now: available, but requires plaintext private content unless
  disclosure is explicit; rejected for the initial slice.
- pgvector or a separate vector service: premature; no measured corpus, and it
  adds a component the architecture deliberately excludes.

## Consequences

- Recall for the slice is exact-key: confirmed facts, cross-session (AC-12).
- Ranking Phase 2 work can decrypt a bounded candidate set in memory.
- Benchmark plan: 1k / 10k / 100k opt-in corpora with recall quality, p95
  latency and footprint recorded before choosing an approximate index.

## Evidence required

Restart-recall test (AC-12) plus deletion test (AC-14) in CI; benchmark report
before any index selection.
