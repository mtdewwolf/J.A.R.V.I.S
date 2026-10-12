# Dependency-ordered roadmap

Statuses are proposed. Each phase lists what must be true first; phases overlap
only where an earlier phase has shipped its exit criteria. Phase numbers match
the "earliest phase" column in [capabilities.md](capabilities.md).

## Phase 0 — Approved engineering package (this package)

- **Goal:** a reviewable, internally consistent design package plus repository
  skeleton and the deterministic `scripts/validate-phase0.mjs` check.
- **Depends on:** nothing.
- **Exit criteria:** validator passes on a clean checkout; all deliverables in
  [README.md](README.md) exist and cross-link; initial commit lands on `main`.
- **Explicitly not included:** implementation, toolchain installation, model
  qualification, remote or hosted PR hosting.

## Phase 1 — Terminal vertical slice

- **Goal:** authenticated CLI talking to one configured provider through the
  Bun gateway, one approval-gated `workspace.read_text`, confirmed semantic
  memory surviving restart, and durable recovery of interrupted tasks.
- **Depends on:** Phase 0 approval. Payload contracts in
  [protocol.md](protocol.md) and [pull-requests.md](pull-requests.md).
- **Exit criteria:** the [initial-slice feature](../../tests/acceptance/initial-slice.feature)
  scenarios pass end to end against PostgreSQL with deterministic fake
  transports; one live provider passes the evaluation protocol in
  [technology.md](technology.md); recovery contracts AC-17–AC-23 pass; Rust
  and Bun toolchains pinned with a recorded clean build (closing the PR-1
  prerequisite noted in [environment.md](environment.md)).
- **Explicitly not included:** voice, vision, device control, agents, second
  live provider qualification (AC-29), backups (AC-27), any non-loopback client.

## Phase 2 — Voice and full memory

- **Goal:** local ASR/VAD/synthesis, barge-in, episodic, procedural and project
  memory, provenance ranking and conflict graphs, declared backup restore with
  erasure ledger (AC-27).
- **Depends on:** Phase 1; measured ASR/TTS benchmarks on declared reference
  hardware (CAP-01/02 targets are goals until measured).
- **Explicitly not included:** speaker identification or camera capture
  (Phase 5), cross-device sessions (Phase 6).

## Phase 3 — Bounded planning and specialist agents

- **Goal:** task decomposition, replanning within budgets, and bounded
  specialist agents inheriting the intersection of parent scope (CAP-07).
- **Depends on:** Phase 1 turn/duration budget data (AC-21–22) and procedural
  memory from Phase 2.
- **Explicitly not included:** open-ended loops, agents approving their own
  proposals, shared secrets between agents.

## Phase 4 — Computer and service control

- **Goal:** R2/R3 adapters (bounded terminal/filesystem, isolated browser
  profile, git and containers, then remote servers) behind scoped grants,
  sandboxing and reconciliation.
- **Depends on:** Phase 1 grant lifecycle; container runtime provisioned
  (currently absent per [environment.md](environment.md)).
- **Explicitly not included:** R4 actions (destructive, financial, physical
  automation) and unsupervised deployment.

## Phase 5 — Multimodal capture

- **Goal:** images, screenshots, documents and consented camera/microphone
  capture with timestamped context and media-retention leases (CAP-04,
  VOICE-04/05).
- **Depends on:** Phase 1 (and Phase 2 for voice alignment); explicit opt-in
  and per-capture consent; measured alignment error.
- **Explicitly not included:** speaker-identification embeddings without
  consent; any ambient capture.

## Phase 6 — Cross-device clients

- **Goal:** Android, Linux desktop and web clients consuming the same versioned
  protocol, with device pairing, revocation and remote TLS (CAP-10, VOICE-06).
- **Depends on:** Phase 1 protocol; non-loopback exposure requires pairing and
  TLS that precede any remote service.
- **Explicitly not included:** clients with capabilities the core lacks;
  browser clients trusting cookies or ambient credentials.

## Phase 7 — Proactive behavior

- **Goal:** named sources, schedules, conditions and bounded alerts with
  persistent deduplication, quiet hours and revocable rules (CAP-11, PRO-01/02).
- **Depends on:** Phase 1 durable state and Phase 2 episodic memory.
- **Explicitly not included:** unseen-source browsing, unrevocable automation,
  silently failing closed-open rule states.

## Phase 8 — Engineering automation, physical integration, spatial UI

- **Goal:** sandboxed engineering assistance with real checks (CAP-08), approved
  home/sensor integrations with safety interlocks (CAP-09), and spatial or
  cinematic interfaces built only on validated backend state (CAP-14 spatial
  tier), with GPU and accessibility benchmarks first.
- **Depends on:** Phase 4 control adapters, Phase 3 agents, Phase 6 clients.
- **Explicitly not included:** autonomous production mutation, guaranteed-safe
  physical intervention, decorative dashboards presented as state.
