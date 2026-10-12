# First five implementation PR specifications

Status: proposed, 2026-10-12. Each PR is one reviewable increment; none grants
the resulting assistant tool access — that requires the phased approval gates in
[security.md](security.md). Slice scope refers to
[acceptance.md](acceptance.md) contract IDs.

## PR-1 — Repository skeleton, pinned toolchains, consistency gate

- **Goal:** a clean checkout that builds and validates. Creates the layout in
  [repository.md](repository.md), pins Rust and Bun toolchains, adds CI running
  `cargo fmt --check`, `cargo clippy`, `cargo test`, `bun install` and
  `scripts/validate-phase0.mjs`. Closes the environment prerequisite recorded in
  [environment.md](environment.md) by *recording* the pins; CI supplies the
  toolchains.
- **Status:** landed 2026-10-12. Toolchains were provisioned locally and the
  fmt/clippy/test/locked-build sequence was recorded before committing; see
  the Post-PR-1 provisioning addendum in [environment.md](environment.md).
- **Out of scope:** application code, migrations, models.
- **Exit criteria:** clean-clone build recorded; validator green in CI.
- **Advances:** AC-28 (evidence pipeline only, via audit-ready CI artifacts).
- **Risks:** none functional; keeps CI fast by deferring DB and live-provider
  jobs to PR-2 and PR-4.

## PR-2 — Durable core: event store, task machine, checkpoints, fencing

- **Goal:** `jarvis-core` with PostgreSQL migrations, the task state machine
  (`queued`, `running`, `waiting_approval`, `waiting_provider`, `waiting_review`,
  `succeeded`, `failed`, `cancelled`), idempotent command ingestion, generation
  checkpoints, lease takeover with monotonic fencing, and crash-injection tests.
  No model calls yet; the gateway interface is called through an in-process fake.
- **Status:** landed 2026-10-12. 13 tests pass against real PostgreSQL:
  command dedup (AC-05), CAS/stale-write rejection, checkpoint round-trip,
  lease fencing, and the four recovery contracts — AC-17 checkpoint resume,
  AC-18 dispatched-without-outcome → `waiting_review`, AC-19
  authorized-not-dispatched → fresh approval, AC-23 outcome pauses for
  review — each with pre- and post-intent crash simulation.
- **Out of scope:** HTTP surface, tools, memory semantics.
- **Exit criteria:** crash-injection suite green: kill -9 across dispatch,
  commit and checkpoint boundaries; restart recovery never duplicates events,
  results or dispatches.
- **Verifies:** AC-05, AC-17, AC-18, AC-19, AC-23. **Advances:** AC-20, AC-28.
- **Risks:** fencing correctness is the PR's real content; treat as
  security-relevant and review with the [ADR-0003](../adr/0003-postgresql-durable-state.md)
  consequences (migrations, rollback drills) in mind.

## PR-3 — Bun model gateway with provider-neutral streaming contract

- **Goal:** `gateway/` implementing the contract in
  [protocol.md](protocol.md): one fake provider adapter plus the Claude Messages
  and Ollama adapters from [ADR-0004](../adr/0004-hosted-transport-candidate.md)
  and [ADR-0005](../adr/0005-local-transport-candidate.md); deadline enforcement,
  bounded buffers, abort, usage reporting; container with no DB credential, no
  workspace mount, no execution route.
- **Status:** landed 2026-10-12. 27 gateway tests pass: schema conformance by
  construction (every emitted event is validated against
  `contracts/schemas`), deadline abort against a stalling provider, output-cap
  cutoff of a flooding provider, delta ceiling, client-disconnect abort,
  refusal of non-conformant provider events, Anthropic/Ollama wire-format
  mapping from captured frames, and end-to-end SSE through the server.
  AC-24's isolation half is enforced structurally: zero dependencies (no DB
  client exists), no `DATABASE_URL`/`postgres://` reference in source, only
  `/healthz` and `POST /v1/inference/stream` routes. `gateway/Dockerfile`
  adds the nonroot, no-credential, no-mount container posture. Live provider
  qualification (AC-29) remains a separate recorded gate.
- **Out of scope:** provider selection policy, fallback, live qualification
  gates (a live smoke run is recorded separately per
  [technology.md](technology.md)).
- **Exit criteria:** schema-conformance tests against
  [`contracts/schemas/`](../../contracts/README.md) pass for all adapters; abort
  and flood tests pass; container credential probe finds no DB route (AC-24).
- **Verifies:** AC-22, AC-24 (isolation half). **Advances:** AC-16, AC-29.
- **Risks:** Bun streaming/abort behavior is unverified in this environment;
  benchmarks belong in the PR, not in a later promise.

## PR-4 — Authenticated CLI, policy and grants, `workspace.read_text`

- **Goal:** end-to-end slice turn: owner-authenticated CLI over loopback with
  Host/Origin checks; proposal → `tool.proposed` → `approval.grant` → atomic
  consume + execution intent → race-resistant `workspace.read_text` (64 KiB,
  UTF-8, exported root only, `openat2` semantics) → result continuation →
  committed response with provenance. Canonicalization vectors for grant digests.
- **Out of scope:** memory writes, recovery UX, second provider.
- **Exit criteria:** the conversation/grant/tool scenarios of
  [initial-slice.feature](../../tests/acceptance/initial-slice.feature) pass with
  the fake transport, then with one live provider from PR-3; every path-class
  and race test closed.
- **Verifies:** AC-01–AC-10, AC-24 (degradation half), AC-28. **Advances:**
  AC-16 (fallback matrix with fakes).
- **Risks:** the TOCTOU open is the security core of the PR; if the required
  primitive is unavailable the feature fails closed and is reported, not
  silently weakened.

## PR-5 — Confirmed semantic memory and durable recovery UX

- **Goal:** candidate/confirm flow with exact-content confirmation
  (AC-11), encrypted owner-scoped key/value with HMAC lookup (AC-12), conflict
  supersession (AC-13), erasure of payloads and source messages (AC-14),
  retention defaults and keyless fail-closed (AC-15), memory-aware cancellation
  (AC-25) and isolation (AC-26); CLI recovery presentation: replay by cursor,
  `waiting_review` re-approval, `outcome_unknown` reporting.
- **Out of scope:** ranking, other memory tiers (Phase 2), backups (AC-27).
- **Exit criteria:** memory and recovery scenarios of
  [initial-slice.feature](../../tests/acceptance/initial-slice.feature) pass,
  including restart-recall and injected-crash resume.
- **Verifies:** AC-11–AC-15, AC-20, AC-25, AC-26 and the recovery scenarios of
  AC-17–AC-19 as E2E. **Advances:** AC-27 (ledger declared, drill Phase 2).
- **Risks:** recall is exact-key by design (ADR-0007); no approximate index
  before benchmarks.

## Contract coverage matrix

| Contract | PR | Verification |
| --- | --- | --- |
| AC-01 | PR-4 | E2E slice scenario + live provider run |
| AC-02 | PR-4 | E2E response-record assertions |
| AC-03 | PR-4 | unit + E2E negative probes |
| AC-04 | PR-4 | unit bindings + RFC 8785 cross-language vectors |
| AC-05 | PR-2 (commands) / PR-4 (proposals) | unit replay suite |
| AC-06 | PR-4 | concurrency race test |
| AC-07 | PR-4 | E2E injection scenario + grant-state inspection |
| AC-08 | PR-4 | unit path-class suite |
| AC-09 | PR-4 | unit path-swap during open; primitive-availability test |
| AC-10 | PR-4 | unit + E2E limit tests |
| AC-11 | PR-5 | unit confirmation-change suite |
| AC-12 | PR-5 | E2E restart-recall scenario |
| AC-13 | PR-5 | unit conflict suite |
| AC-14 | PR-5 | unit erasure + database inspection |
| AC-15 | PR-5 | unit retention + keyless-start test |
| AC-16 | PR-3/PR-4 | unit + E2E fallback matrix (fakes) |
| AC-17 | PR-2 / PR-5 E2E | crash-injection + E2E kill -9 |
| AC-18 | PR-2 | crash-injection unit suite |
| AC-19 | PR-2 | crash-injection unit suite |
| AC-20 | PR-5 | unit window/overflow tests |
| AC-21 | PR-3/PR-4 | unit budget tests + stalling fake |
| AC-22 | PR-3 | unit backpressure + resource-ceiling test |
| AC-23 | PR-2 | crash-injection unit suite |
| AC-24 | PR-3/PR-4 | container credential probe + E2E gateway-down |
| AC-25 | PR-5 | unit cancellation suite |
| AC-26 | PR-5 | unit isolation suite |
| AC-27 | deferred to Phase 2 | restore drill (backups off by default) |
| AC-28 | PR-1 pipeline / PR-4 events | unit audit suite + metadata leak scan |
| AC-29 | deferred to qualification gate | second live provider evaluation |
