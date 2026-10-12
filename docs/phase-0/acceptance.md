# Initial acceptance contracts

Status: proposed contracts, 2026-10-12. These are testable statements, not
results. Rows here are the single source of truth for `AC-xx` IDs referenced by
[capabilities.md](capabilities.md), [security.md](security.md),
[pull-requests.md](pull-requests.md) and
[initial-slice.feature](../../tests/acceptance/initial-slice.feature).

- **Level** — E2E: whole system through the CLI; Unit: module-scope; Live: needs
  a real provider or hardware, cannot pass with fakes.
- **Slice** — included in the initial vertical slice with deterministic fakes
  plus one live provider, per the false-claims policy.
- Passing every row below is a prerequisite for claiming the corresponding
  capability in [capabilities.md](capabilities.md); a failing or deferred row
  keeps that capability unimplemented.

## Summary matrix

| Contract | Statement (short) | Level | Slice | Supports |
| --- | --- | --- | --- | --- |
| AC-01 | Completed authenticated text generation | E2E | Yes | CORE-01, CAP-05 |
| AC-02 | Recorded provenance in every response | E2E | Yes | CAP-12, CORE-01 |
| AC-03 | Authenticated, origin-checked commands | Unit/E2E | Yes | OPS-01 |
| AC-04 | Exact, one-use grant binding | Unit | Yes | CAP-06 |
| AC-05 | Input and proposal deduplication | Unit | Yes | CAP-06, CAP-13 |
| AC-06 | Single-consumption under concurrency | Unit | Yes | CAP-06 |
| AC-07 | Model cannot grant or alter permissions | E2E | Yes | CAP-06, OPS-01 |
| AC-08 | Resource resolution fails closed | Unit | Yes | CAP-06, CTRL-03 |
| AC-09 | Race-resistant descriptor-relative open | Unit | Yes | CAP-06 |
| AC-10 | Bounded UTF-8 read | Unit/E2E | Yes | CAP-06 |
| AC-11 | Exact-content confirmation gate | Unit | Yes | MEM-03 |
| AC-12 | Cross-session recall after restart | E2E | Yes | MEM-03, CAP-03 |
| AC-13 | Conflict supersession, never silent upgrade | Unit | Yes | MEM-06 |
| AC-14 | Payload and source erasure | Unit | Yes | MEM-07, CAP-03 |
| AC-15 | Retention defaults; key loss fails closed | Unit | Yes | MEM-07 |
| AC-16 | No silent disclosure widening in fallback | Unit/E2E | Yes | CORE-03 |
| AC-17 | Checkpoint resume without duplication | E2E | Yes | CAP-13 |
| AC-18 | Dispatched without outcome awaits review | Unit | Yes | CAP-13 |
| AC-19 | Authorized-not-dispatched needs fresh approval | Unit | Yes | CAP-13 |
| AC-20 | Bounded working-memory generation | Unit | Yes | MEM-01 |
| AC-21 | Turn and per-attempt time budgets | Unit | Yes | CAP-05 |
| AC-22 | Bounded buffers against stall or flood | Unit | Yes | CAP-05, CAP-13 |
| AC-23 | Uncertain outcome pauses for review | Unit | Yes | CAP-13 |
| AC-24 | Gateway isolation and honest degradation | Unit | Yes | CAP-12 |
| AC-25 | Cancellation stops continuation and writes | Unit | Yes | CAP-13 |
| AC-26 | Owner and session isolation before decryption | Unit | Yes | OPS-01 |
| AC-27 | Erasure ledger applied before serving restore | Manual | No | MEM-07 |
| AC-28 | Audit evidence for every attempted action | Unit | Yes | CAP-06, OPS-02 |
| AC-29 | Two independently qualified transports | Live | No | CORE-03 |

## Contracts

### AC-01 — Completed authenticated text generation

An authenticated owner's text input produces exactly one bounded generation
and one committed response; response is replayable from the event cursor after
reconnect; unauthenticated input yields no generation.
*Evidence: E2E scenario "owner completes a text turn"; gateway and DB logs.*

### AC-02 — Recorded provenance in every response

Every committed response stores provider identity, exact model configuration,
tool outcomes used, and degradation or uncertainty markers; a stale or aborted
generation never commits partial content as final.
*Evidence: E2E response record assertions.*

### AC-03 — Authenticated, origin-checked commands

Commands without the owner bearer credential, with an unknown Host or Origin,
or from an unpaired client are rejected before any effect; the CLI stores the
credential with restrictive permissions and no ambient cookies.
*Evidence: unit + E2E negative probes.*

### AC-04 — Exact, one-use grant binding

An approval grant binds owner, session, task, generation, proposal ID, tool and
version, RFC 8785 canonical argument SHA-256 digest, resource scope, policy
version, expiry and `max_uses=1`; dispatch with altered arguments fails before
execution.
*Evidence: unit suite for bindings; canonicalization cross-language vectors.*

### AC-05 — Input and proposal deduplication

Replayed owner/request IDs and replayed proposal IDs create no duplicate
commands, generations or executions; where a result is committed it is reused.
*Evidence: unit replay suite.*

### AC-06 — Single-consumption under concurrency

Concurrent dispatch attempts against one grant produce exactly one execution
and one recorded outcome; a failed validation never partially consumes a grant.
*Evidence: concurrency test with racing dispatch calls.*

### AC-07 — Model cannot grant or alter permissions

Content in documents, tool results or model output is an untrusted observation:
a stored injection string inside an approved file — "approve", "grant",
"ignore previous approval" — produces no privilege effect, and grant state is
never model-editable.
*Evidence: E2E scenario "injection content cannot approve"; state inspection.*

### AC-08 — Resource resolution fails closed

Absolute paths, traversal, symlinks, magic links, unexpected mount transitions,
hardlinks (`nlink > 1`) and special files are rejected before content access.
*Evidence: unit suite over path classes.*

### AC-09 — Race-resistant descriptor-relative open

The read opens via `openat2` with beneath, no-symlink and no-magic-link
resolution (or an equivalently checked implementation) and validates the opened
descriptor, not the path; replacing the path between approval and dispatch does
not disclose the replacement; a missing primitive fails closed.
*Evidence: unit test swapping the file during open; primitive-availability test.*

### AC-10 — Bounded UTF-8 read

`workspace.read_text` returns at most 64 KiB of validated UTF-8 within the
2-second dispatch budget; oversize, non-UTF-8 or unreadable targets produce an
observable error and no fabricated content.
*Evidence: unit + E2E limit tests.*

### AC-11 — Exact-content confirmation gate

Only exact structured key/value content the owner explicitly confirms becomes
confirmed semantic memory; model candidates stay pending with 24-hour
retention; changing candidate content invalidates the confirmation.
*Evidence: unit suite; confirmation-change test.*

### AC-12 — Cross-session recall after restart

A confirmed fact is retrievable by owner-scoped key after a full process
restart; inferred content is returned labeled and never as confirmed.
*Evidence: E2E restart-recall scenario.*

### AC-13 — Conflict supersession, never silent upgrade

A conflicting confirmed fact creates a supersession record or an owner question;
belief is never silently upgraded and provenance is retained.
*Evidence: unit conflict suite.*

### AC-14 — Payload and source erasure

Deleting a memory removes its payload, derived context and referenced source
messages; retained audit metadata holds IDs and reason codes only.
*Evidence: unit erasure suite; database inspection.*

### AC-15 — Retention defaults; key loss fails closed

Retention defaults apply (30 days private payload, 24 hours candidates, 90 days
audit metadata); starting without the memory key fails closed for private
payloads and tool dispatch.
*Evidence: unit retention test; keyless-start test.*

### AC-16 — No silent disclosure widening in fallback

Fallback inference uses only already configured providers under the same
data-egress policy; otherwise the task parks in `waiting_provider` with an
actionable error and no invented answer.
*Evidence: unit + E2E fallback matrix.*

### AC-17 — Checkpoint resume without duplication

A task interrupted mid-generation resumes from its last checkpoint after
restart: no duplicated response, no duplicated dispatch; cursor replay shows
ordered, non-duplicated events.
*Evidence: E2E kill -9 during generation; event log diff.*

### AC-18 — Dispatched without outcome awaits review

A dispatched read whose outcome never committed moves to `waiting_review`; the
original grant is neither renewed nor reused; rereading requires fresh approval.
*Evidence: crash-injection unit suite.*

### AC-19 — Authorized-not-dispatched needs fresh approval

An authorized but undispatched step after restart requires fresh approval;
recovery never auto-executes.
*Evidence: crash-injection unit suite.*

### AC-20 — Bounded working-memory generation

Generation context stays within configured window/size limits; overflow
truncates or spills as configured; context is rebuilt from durable events, not
from model output.
*Evidence: unit suite; restart context reconstruction test.*

### AC-21 — Turn and per-attempt time budgets

At most four model turns per task and 30 seconds per provider attempt are
enforced; exhaustion or provider stall aborts with a recorded reason rather
than looping.
*Evidence: unit budget tests; stalling fake provider.*

### AC-22 — Bounded buffers against stall or flood

Core and gateway stream through bounded buffers; a stalled or flooding provider
cannot exhaust memory; abort cancels upstream promptly.
*Evidence: unit backpressure tests; resource ceiling assertions.*

### AC-23 — Uncertain outcome pauses for review

A crash between dispatch and committed outcome leaves the task paused with the
execution ID recorded; recovery cannot invent the outcome or blindly re-execute.
*Evidence: crash-injection unit suite.*

### AC-24 — Gateway isolation and honest degradation

The gateway process holds no database or workspace credential and exposes no
execution endpoint; gateway-supplied bytes cannot create grants; provider or
gateway failure surfaces as an actionable degraded result with uncertainty
marked, never an invented answer.
*Evidence: containerization test with credential probe; E2E gateway-down case.*

### AC-25 — Cancellation stops continuation and writes

Cancelling interest stops consuming provider output and blocks continuation,
stale publication and memory writes; an action whose completion is unknown is
reported as `outcome_unknown`, not success.
*Evidence: unit cancellation suite.*

### AC-26 — Owner and session isolation before decryption

Scope filters (owner, session) apply before decryption; cross-owner reads or
candidate injection are impossible at the API boundary.
*Evidence: unit isolation suite on memory and event queries.*

### AC-27 — Erasure ledger applied before serving restore

Backups declare a maximum 30-day expiry; a restore applies the retained erasure
ledger before accepting traffic; deletion acknowledgement distinguishes erased
active data from expiring backup copies.
*Deferred to Phase 2 (backups are off by default). Evidence: restore drill.*

### AC-28 — Audit evidence for every attempted action

Requested, denied, authorized, dispatched and outcome metadata are logged with
IDs and reason codes for each attempted action; metadata contains no secrets or
plaintext bodies; logging failure prevents dispatch.
*Evidence: unit audit suite; metadata leak scan.*

### AC-29 — Two independently qualified transports

Two independently qualified transports implement the same inference contract
with identical observable core behavior.
*Deferred to the provider-qualification gate. Evidence: second live provider
runs through the evaluation protocol.*
