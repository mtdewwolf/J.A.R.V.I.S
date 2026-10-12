# ADR-0001: Trusted core in Rust with Tokio and Axum

- Status: Proposed
- Date: 2026-10-12
- Relates to: [technology.md](../phase-0/technology.md), [architecture.md](../phase-0/architecture.md)

## Context

The owner baseline requires core systems in Rust. The core owns authorization,
durable task state and the single initial tool; these are the components where
language-level enforcement of explicit state and boundaries matters most.

## Decision

Implement `jarvis-core` in Rust with Tokio for async runtime and Axum for the
HTTP surface. Axum supplies HTTP building blocks only; authorization, policy and
durability are hand-written module boundaries.

## Alternatives considered

- All-Bun runtime: one language end to end, but weakens the explicit trusted
  boundary and conflicts with the stated Rust baseline.
- Python framework: fast to write, but the same trusted-boundary argument
  applies and process-level isolation of the gateway is weaker.

## Consequences

- Toolchain must be pinned and a clean build recorded (PR-1 exit criterion).
- Core owns policy and state; nothing security-relevant may move into the
  gateway or CLI for convenience.
- Async boundaries must carry deadlines and cancellation explicitly.

## Evidence required

A recorded clean build and `cargo test` run in pinned tooling; clippy-clean
lint gate in CI before merge of PR-2.
