# Architecture decision records

Status: all entries recorded 2026-10-12 as **Proposed**, matching the phase-0
package. None is an installed or measured decision; the implementing PR records
actual evidence and can supersede an entry with a new ADR rather than editing
history.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](0001-rust-trusted-core.md) | Trusted core in Rust with Tokio and Axum | Proposed |
| [0002](0002-bun-model-gateway.md) | Integration transport in TypeScript on Bun, isolated process | Proposed |
| [0003](0003-postgresql-durable-state.md) | PostgreSQL as durable state, no message broker | Proposed |
| [0004](0004-hosted-transport-candidate.md) | Claude Messages API as hosted transport candidate | Proposed |
| [0005](0005-local-transport-candidate.md) | Ollama as local transport candidate | Proposed |
| [0006](0006-cli-first-client.md) | Authenticated Rust CLI as first client | Proposed |
| [0007](0007-exact-key-memory-retrieval.md) | Owner-scoped encrypted key/value retrieval first | Proposed |

## Conventions

- One decision per file, numbered, immutable once accepted; supersede instead of
  rewriting.
- Every ADR states context, decision, alternatives, consequences and the
  evidence required before the decision is considered verified.
- Decisions in [technology.md](../phase-0/technology.md) link here so the matrix
  and the records cannot drift apart silently.
