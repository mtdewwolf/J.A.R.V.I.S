# Phase 0 engineering package

Status: proposed for review, 2026-10-11. Implementation approval is pending.

The first milestone is a real assistant that converses through a model provider,
executes one explicitly approved tool, remembers a confirmed fact across sessions,
and resumes an interrupted task after process restart. The first interface is a
terminal client. Voice and cinematic clients follow after this foundation works.

## Required deliverables

| Deliverable | Artifact |
| --- | --- |
| Capability parity and limits | [capabilities.md](capabilities.md) |
| Modular architecture and boundaries | [architecture.md](architecture.md) |
| Technology decisions and alternatives | [technology.md](technology.md), [ADRs](../adr/README.md) |
| Security and permissions | [security.md](security.md) |
| Dependency-ordered roadmap | [roadmap.md](roadmap.md) |
| Repository structure | [repository.md](repository.md) |
| First five reviewable PR specifications | [pull-requests.md](pull-requests.md) |
| Initial end-to-end acceptance contracts | [acceptance.md](acceptance.md), [Gherkin](../../tests/acceptance/initial-slice.feature) |
| Typed interface contracts | [protocol.md](protocol.md), [schemas](../../contracts/README.md) |
| Audit and research evidence | [environment.md](environment.md), [sources.md](sources.md) |

## Proposed approval scope

Approve the five bounded implementation increments described in
[pull-requests.md](pull-requests.md): Rust core, Bun model transport, PostgreSQL,
authenticated local CLI, `workspace.read_text`, confirmed semantic memory, and
durable task recovery. Implement both a hosted transport and a local transport
against one provider-neutral contract; qualify actual model choices with a live
test, configuration, credentials where needed, and explicit data-egress settings.

Default constraints: one local owner, one active task, maximum four model turns
and 30 seconds per provider attempt, one pending tool at a time, 64 KiB tool output,
60-second one-use grants. These are starting design limits to verify, not measured
performance. Approval to build does not grant the resulting assistant tool access.

No paid resources, production deployments, arbitrary shell tool, device control,
or public server are included. Future capabilities have separate release gates.

Your master directive says: “Then begin implementation of the approved initial
vertical slice.” This package makes that slice concrete before seeking approval.

## Completion status

As of 2026-10-12 the package is complete as documentation: every deliverable
above exists and cross-checks under `scripts/validate-phase0.mjs`, and the
initial commit lands this state on `main`. Feature scenarios remain contracts
without step implementations; they have not run against a service. No capability
in [capabilities.md](capabilities.md) is claimed complete. Five implementation
PR specifications are prepared in [pull-requests.md](pull-requests.md); hosting
them requires a remote and an implementation approval, both of which are
pending.
