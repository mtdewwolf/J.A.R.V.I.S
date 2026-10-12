# Typed interface contracts

Status: proposed, 2026-10-12. All structured payloads between components are
defined once in [`contracts/schemas/`](../../contracts/README.md) and validated
by every side; prose here defines semantics and rules the schemas cannot.

Contract version: **v0.1 (proposed)**. Compatibility is additive only within a
major version: new optional fields and new event types; never repurpose an
existing field; a breaking change requires `v0.2` running in parallel with a
documented cutover.

## Client protocol (CLI ↔ core)

Loopback HTTP with bearer authentication and an SSE event stream per session.
Commands and events are JSON; every event carries the envelope below.

### Commands (client → core)

| Command | Payload | Semantics |
| --- | --- | --- |
| `session.open` | credential handshake parameters | Establishes a session; returns session ID and current replay cursor. |
| `input.send` | owner/request ID, text, target generation | Idempotent; replays reuse the committed result (AC-05). |
| `approval.grant` | proposal ID, exact argument digest | Owner-only; creates the one-use grant (AC-04). |
| `approval.revoke` | proposal or grant ID | Prevents starting new operations only. |
| `task.cancel` | task ID | Stops interest; outcomes already begun may be `outcome_unknown` (AC-25). |
| `memory.confirm` | candidate ID, exact content digest | Confirms structured content verbatim (AC-11). |
| `memory.delete` | memory ID | Erases payload, derived context and source messages (AC-14). |
| `events.since` | cursor | Ordered replay for reconnecting clients. |

### Events (core → client)

`input.accepted`, `task.state_changed`, `tool.proposed`, `tool.dispatched`,
`tool.result`, `memory.candidate`, `memory.confirmed`, `response.started`,
`response.delta`, `response.committed`, `degraded.notice`, `audit.notice`.

### Event envelope

```json
{
  "contractVersion": "0.1",
  "eventId": "evt_01J…",
  "cursor": 4182,
  "occurredAt": "2026-10-12T00:00:00Z",
  "sessionId": "s_…",
  "type": "task.state_changed",
  "payload": { }
}
```

Rules: cursors are gap-free per session; `response.delta` content is
provisional and never an authorization source; `response.committed` is the only
committal boundary (AC-01, AC-02).

### Approval presentation rule

`tool.proposed` and `approval.grant` must display: tool name and version,
exact canonical argument digest, resolved resource, risk tier, outbound-data
implications, expiry and `max_uses=1`. A natural-language affirmation from a
model or file is not an approval (AC-07).

## Model gateway protocol (core ↔ gateway)

`POST /v1/inference/stream` with a provider-neutral request:

```json
{
  "contractVersion": "0.1",
  "generationId": "g_…",
  "deadlineMs": 30000,
  "maxTurns": 4,
  "maxOutputTokens": 4096,
  "model": { "provider": "configured", "name": "configured" },
  "system": [ { "role": "system", "content": "…" } ],
  "messages": [ { "role": "user", "content": "…", "untrusted": true } ],
  "tools": [ { "name": "workspace.read_text", "version": 1, "schema": { } } ]
}
```

Server-sent events: `started`, `content.delta`, `tool_use.proposal`,
`usage`, `completed`, `aborted`, `error`. `tool_use.proposal` is a proposal
only — complete, schema-valid arguments required before the core will persist
it; the gateway cannot invoke tools.

Untrusted-content rule: all model output, document content and tool results
carry `untrusted: true`; the gateway never sees core authorization state, and
the core treats gateway bytes as observations.

Abort and budget rules: the gateway enforces the core's deadline
(`aborted` on expiry), streams with bounded buffers, and reports token usage
per completion so budgets can be checked (AC-21, AC-22).

## Memory record shape

Semantic records carry owner, scope, structured key/value, epistemic status
(`confirmed` / `inferred` / `uncertain`), source references, timestamps,
retention and supersession links. Only `memory.confirm` promotes content to
`confirmed` (AC-11); retrieval filters by owner/scope before decryption
(AC-26).

## Tool registry contract

Each registered tool declares name, semantic version, JSON Schema arguments,
resource model, limits (size, duration, concurrency) and risk tier. Unknown
tools, extra fields and duplicate JSON keys fail before approval (AC-04). Risk
tier is registry metadata and cannot be downgraded by the model.

## Storage contract

Durable payloads (messages, tool results, memory values) are stored encrypted
with versioned key identifiers; PostgreSQL holds ciphertext, references and
content-free event metadata only. The full erasure ledger in
[security.md](security.md) governs deletion and restore.
