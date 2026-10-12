# Audit and research evidence

Status: 2026-10-12. This file records what the phase-0 decisions rest on. All
sources below are public documentation consulted during design; none is a
commitment, and none was qualified as a runtime winner — that happens in the
implementing PRs per [technology.md](technology.md).

## Primary sources

| Source | Used for | Status |
| --- | --- | --- |
| [Axum docs](https://docs.rs/axum/latest/axum/) | HTTP plumbing for the trusted core (ADR-0001) | Consulted |
| [Bun server docs](https://bun.sh/docs/runtime/http/server) | Streaming HTTP surface for the gateway (ADR-0002) | Consulted |
| [eve getting started](https://eve.dev/docs/getting-started) | TypeScript agent-runtime alternative; current quickstart requires Node 24+ | Consulted, not selected |
| [PostgreSQL explicit locking](https://www.postgresql.org/docs/current/explicit-locking.html) | Fencing/single-consumption reasoning (ADR-0003) | Consulted |
| [PostgreSQL text search intro](https://www.postgresql.org/docs/current/textsearch-intro.html) | FTS alternative privacy trade-off (ADR-0007) | Consulted, deferred |
| [Claude streaming](https://platform.claude.com/docs/en/build-with-claude/streaming) | Hosted transport candidate (ADR-0004) | Consulted |
| [Claude tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview) | Server-side execution explicitly rejected as bypass | Consulted |
| [Ollama chat API](https://docs.ollama.com/api/chat) | Local transport candidate (ADR-0005) | Consulted |
| [RFC 8785 (JCS)](https://www.rfc-editor.org/info/rfc8785/) | Grant argument digest canonicalization | Consulted; vectors required in PR-4 |
| [whisper.cpp](https://github.com/ggml-org/whisper.cpp) | ASR/VAD benchmark candidate (Phase 2) | Consulted, unmeasured |
| [Piper (GPL)](https://github.com/OHF-Voice/piper1-gpl) | Local synthesis candidate; licensing decision pending | Consulted, unmeasured |
| [LiveKit turns](https://docs.livekit.io/agents/logic/turns/) | Cross-device voice layer candidate (Phase 6) | Consulted, deferred |
| [Android foreground service types](https://developer.android.com/develop/background-work/services/fgs/service-types) | Android background constraints (Phase 6) | Consulted |
| [GPUI](https://gpui.rs/), [Tauri](https://tauri.app/) | Desktop UI candidates, deferred until benchmarks | Consulted |

## Environment audit

Recorded in [environment.md](environment.md): Linux x86_64, Node v22.23.3 and
Python 3.9.25 present; GitHub CLI present (authentication untested); Rust, Bun,
Docker/Podman and PostgreSQL absent at audit time. The PR-1 exit criterion
(pinned toolchains, recorded clean build) exists precisely because this
environment cannot yet build the slice.

## Research tooling note

An initial attempt to use the Stripe CLI's Directory plugin for provider
research failed with an invalid-user-agent response; no directory results are
used as evidence anywhere in this package. No subscription, credentials,
payment, or tool was provisioned during this phase.

## Review evidence

An independent delegated design review examined the proposed security and
recovery boundaries before this package and informed exact approval binding,
authenticated loopback access, race-resistant opens, lease fencing,
cancellation outcomes and the erasure policy. It was a design review, not
runtime verification; nothing in this package claims executed evidence.

## Claim ledger

Every capability row in [capabilities.md](capabilities.md) is unimplemented at
package time. Measured values (WER, latency, success rates) appear only as
targets attached to phase gates in [roadmap.md](roadmap.md), never as results.
