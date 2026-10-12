# Technology decision matrix

Selections are proposals, not installed dependencies or measured winners. Pin
versions and verify API/runtime compatibility during the implementing PR. Do not
encode a current model leaderboard or unmeasured latency claim in configuration.

| Area | Proposed choice | Alternatives considered | Decision and next evidence |
| --- | --- | --- | --- |
| Trusted core | Rust, Tokio, Axum, Serde | All-Bun runtime, Python framework | Meets user's Rust baseline; type explicit state/policy boundaries. Axum supplies HTTP building blocks, not authorization. [Axum](https://docs.rs/axum/latest/axum/) |
| Integration transport | TypeScript on Bun, minimal HTTP streaming adapters | Node.js, AI SDK, provider SDKs | Matches baseline; keep transport behind schemas. Qualify abort, timeouts and backpressure on Bun. [Bun server](https://bun.sh/docs/runtime/http/server) |
| Agent runtime | Small persisted Rust state machine | eve, graph frameworks, workflow service | User's Rust-core boundary takes precedence over skill defaults. Avoid a second owner of durable session/permission state. eve is a credible TypeScript alternative; its current quickstart requires Node 24+. Revisit if custom runtime costs exceed measured value. [eve](https://eve.dev/docs/getting-started) |
| Durable state | PostgreSQL, SQLx candidate | SQLite, event-stream service | PostgreSQL is baseline; transactional state/event/intents, no message broker. Verify migrations, rollback and restart behavior. [PostgreSQL locks](https://www.postgresql.org/docs/current/explicit-locking.html) |
| Hosted model transport | Claude Messages candidate behind gateway | Other independently qualified hosted APIs | Documented streaming/tool-use contract; never use server-side execution to bypass core. Provider account/model choice remains configurable and pending live eval. [Streaming](https://platform.claude.com/docs/en/build-with-claude/streaming), [tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/overview) |
| Local model transport | Ollama candidate | llama.cpp server, other local engines | Chat supports streaming and declared tools; individual models still require qualification. No local inference hardware assumed. [Ollama chat](https://docs.ollama.com/api/chat) |
| Initial memory retrieval | Structured owner-scoped encrypted key/value; HMAC key lookup | PostgreSQL FTS, pgvector, separate vector service | Exact confirmed facts first. Benchmark 1k/10k/100k opt-in corpus retrieval quality, p95 latency and footprint before selecting approximate index. PostgreSQL FTS is available but conflicts with private plaintext indexing unless disclosure is explicit. [FTS](https://www.postgresql.org/docs/current/textsearch-intro.html) |
| ASR / VAD | Evaluate whisper.cpp and a qualified streaming hosted adapter | OS speech APIs, other streaming engines | whisper.cpp documents CPU and multiple accelerators plus VAD; chunked transcription is not automatically low-latency full duplex. Benchmark target hardware, WER and endpointing. [whisper.cpp](https://github.com/ggml-org/whisper.cpp) |
| Synthesis | Evaluate local Piper and a qualified streaming hosted adapter | OS TTS | Local privacy candidate; verify voice-model licensing and streaming behavior. Current Piper repository is GPL; distribution needs a deliberate licensing decision. No engine is committed yet. [Piper](https://github.com/OHF-Voice/piper1-gpl) |
| Voice transport | Local audio first; WebRTC candidate for cross-device sessions | Raw WebSockets; LiveKit session layer | Benchmark echo, jitter and stop-to-silence. LiveKit documents turn/interruption events; any such layer must call central policy. Do not add it for the text milestone. [LiveKit turns](https://docs.livekit.io/agents/logic/turns/) |
| Initial UI | Authenticated Rust CLI | Web client first | Smallest complete backend verification path; no decorative capability dashboard |
| Desktop | Evaluate GPUI versus Tauri and web | Other native GPU frameworks | GPUI is a Rust UI candidate; Tauri can share web UI with a Rust host. Defer selection until Linux rendering, accessibility, IME, packaging and 3D benchmarks exist. [GPUI](https://gpui.rs/), [Tauri](https://tauri.app/) |
| Android | Kotlin / Jetpack Compose | Web wrapper, shared native framework | User baseline; evaluate wake/camera/microphone behavior on target OS. Android foreground-service rules constrain capture/background operation. [Android rules](https://developer.android.com/develop/background-work/services/fgs/service-types) |
| Deployment | Local containers with pinned images; minimal Compose setup | Hosted platform, Kubernetes | No distributed infrastructure in MVP. Verify rootless/nonroot isolation and durable volume restore before exposure |
| Telemetry | Structured metadata traces and stage latency histograms | Raw prompt logs, full conversation tracing | Never store credentials/private bodies in diagnostic logs; compare evidence with recorded limits |

## Evaluation protocol

Model evaluation: fixed tasks covering plain text, schema-valid tools, instruction
injection, factual recall, outage and budget exhaustion. Record selected provider,
exact model/configuration, tool precision, task success rate, first-token and total
latency p50/p95, actual token usage and cost where available. Use a fake transport
only for deterministic failure injection; a live hosted or local test must pass
before claiming real conversation. Second-provider live qualification is required
before claiming operational fallback.

Voice: measure capture/VAD/ASR/model/TTS/playback separately, including cold starts,
WAN jitter, echo, accents and interruptions. Target p95 first audible answer under
1.5 seconds after end-of-turn on a declared reference configuration; target p95
playback stop under 250 ms after detected interruption. These are goals, not results.

Desktop: implement the same event-driven conversation/timeline screen in two
throwaway spikes; measure frame time, memory, keyboard/screen-reader accessibility,
Linux X11/Wayland support, and packaging. Select from actual evidence, not cinematic
appearance. Retrieval ranking and local inference get similar bounded experiments.
