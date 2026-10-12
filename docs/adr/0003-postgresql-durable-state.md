# ADR-0003: PostgreSQL as durable state, no message broker

- Status: Proposed
- Date: 2026-10-12
- Relates to: [architecture.md](../phase-0/architecture.md) "Durable task state"

## Context

Tasks, grants, confirmations, events and encrypted payloads must transition
atomically. Recovery correctness depends on committing state transition, event
and execution intent in one transaction.

## Decision

PostgreSQL (SQLx as the Rust client candidate) holds all durable state. Events
are append-only; clients poll by cursor with notifications as hints. No message
broker, no separate vector database, no event-stream service in the initial
slice.

## Alternatives considered

- SQLite: simplest local deployment, weaker concurrency and operations story
  than the owner's stated PostgreSQL baseline.
- Event-stream service: adds infrastructure and a second owner of ordering
  without a demonstrated need.

## Consequences

- Migrations are reviewed as schema changes (repository.md layout rules).
- Row-level locking informs fencing and single-consumption logic; the
  application still enforces its own invariants (PostgreSQL supports locks, it
  does not enforce these particular ones).
- Leases plus monotonic fencing prevent stale-worker writes; an outbox is added
  only if a real worker dispatch boundary appears.

## Evidence required

Migration, rollback and restart drills; crash-injection runs of AC-17, AC-18
and AC-23 in CI with a disposable PostgreSQL service.
