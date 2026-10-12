# Capability-parity matrix

All entries are **unimplemented**. Feasibility labels are engineering assessments,
not benchmark results: practical = achievable for bounded tasks; conditional =
depends on hardware, provider quality, platform restrictions or integrations;
unsupported = no credible general-purpose guarantee. Criteria below are release
targets; phase-1 contract IDs refer to [acceptance.md](acceptance.md).

## User-visible capabilities

| ID | Requested capability | Feasibility and limits | Earliest phase | Measurable release criterion |
| --- | --- | --- | --- | --- |
| CAP-01 | Understand and generate spoken language | Conditional; language, accents and acoustic conditions matter | 2 | Frozen audio corpus; report WER by condition and time to first audible response |
| CAP-02 | Full-duplex interruptible voice | Conditional; acoustic echo, network and playback buffers matter | 2 | 100 barge-in trials; playback stops within 250 ms p95 after detected interruption |
| CAP-03 | Persistent structured memory | Practical for scoped confirmed facts; recall is fallible | 1 minimal / 2 full | AC-11–15; restart, conflict, provenance, expiry and deletion checks |
| CAP-04 | Images, screenshots, cameras, video and documents | Conditional; perception is probabilistic and capture needs consent | 5 | Labeled corpus precision/recall, timestamp alignment, revoked-capture checks |
| CAP-05 | Multi-step reasoning and planning | Practical for bounded tasks; no guaranteed general reasoning | 1 minimal / 3 full | AC-01, AC-20–23; held-out task success rate with budgets reported |
| CAP-06 | Authorized computer and service actions | Practical per adapter; no blanket machine access | 1 minimal / 4 full | AC-03–10, AC-28; every attempted action has policy and audit evidence |
| CAP-07 | Specialist agent coordination | Practical; more agents need not improve results | 3 | Two independent jobs finish; cancelled children stop; inherited grants never widen |
| CAP-08 | Engineering, tests, debugging and deployment | Practical with repository sandbox and approval gates | 8 | Patch built/tested in clean sandbox; PR contains actual checks; deployment separately approved |
| CAP-09 | Physical devices, sensors and home automation | Conditional on supported APIs, local controls and safety interlocks | 8 | One consented sensor and reversible actuator; offline and emergency-stop tests |
| CAP-10 | Android, Linux desktop, server and web | Practical per client; OS permissions constrain background behavior | 6 | All clients consume shared versioned protocol and pass continuity/auth tests |
| CAP-11 | Proactive event response | Practical for explicit rules and bounded sources | 7 | Seeded events fire once within configured window; disabling rule stops work |
| CAP-12 | Explain decisions, actions, uncertainty and failures | Practical through recorded evidence and concise rationale | 1 | AC-02, AC-16, AC-24; result identifies provider, tool outcome and uncertainty |
| CAP-13 | Safe failure and interrupted workflow recovery | Practical with durable checkpoints; external effects may remain unknown | 1 | AC-17–23, AC-25; injected crashes cannot renew permissions or duplicate effects |
| CAP-14 | Immersive real-time command interface | Practical; accessibility and actual backend state first | 6 / 8 spatial | Real event stream reflected within 200 ms p95 locally; keyboard/reduced-motion checks |

## Subsystem completeness

| ID | System detail | Feasibility / phase | Acceptance target |
| --- | --- | --- | --- |
| CORE-01 | Intent, conversation, routing and response generation | Practical / 1 | Completed text/tool/continuation flow; AC-01–02 |
| CORE-02 | Task decomposition, replanning and monitoring | Conditional / 3 | Failed step replans within budget; plan version and reason recorded |
| CORE-03 | Error handling and provider independence | Practical / 1 | Two transports qualify; no silent privacy widening; AC-16, AC-29 |
| MEM-01 | Working memory | Practical / 1 | Generation-scoped context and bounded history; AC-20 |
| MEM-02 | Episodic memory | Practical / 2 | Events retain origin/time; retrieve only owner-authorized episodes |
| MEM-03 | Semantic memory | Practical / 1 | Explicit exact-content confirmation, cross-session recall; AC-11–13 |
| MEM-04 | Procedural memory | Conditional / 2–3 | Versioned reviewed workflows cannot create permission grants |
| MEM-05 | Project memory | Practical / 2 | Repository/project scopes prevent cross-project disclosure |
| MEM-06 | Provenance, ranking and conflicts | Practical / 1–2 | Confirmed/inferred/uncertain distinct; supersession graph; AC-12–13 |
| MEM-07 | Retention, encryption and deletion | Practical with declared backup limits / 1–2 | AC-14–15, AC-27; payload erasure includes retained source messages |
| VOICE-01 | Wake word | Conditional / 2 | Target-device false-wake/missed-wake rates; OS-background test |
| VOICE-02 | Streaming ASR, synthesis and VAD | Conditional / 2 | Separate stage latencies, WER, echo suppression and CPU/memory profiles |
| VOICE-03 | Barge-in and cancellation | Conditional / 2 | Audio stop target plus no stale tool proposals or memory commits |
| VOICE-04 | Timestamped audio/screen/camera context | Conditional / 5 | Alignment error within 100 ms in controlled capture test |
| VOICE-05 | Speaker identity | Conditional; opt-in only / 5 | No embedding stored without consent; report false acceptance/rejection |
| VOICE-06 | Cross-device sessions | Practical / 6 | Pair/revoke devices; reconnect event cursor; one active generation |
| AGENT-01 | Definitions, delegation and parallel execution | Practical / 3 | Two bounded specialist tasks, shared artifact references, no shared secrets |
| AGENT-02 | Scope, budgets, deadlines and cancellation | Practical / 3 | Parent/child limits intersect; budget exhaustion and cascade cancellation |
| AGENT-03 | Verification, traces and approvals | Practical / 3 | Tests substantiate results; child cannot approve its own proposal |
| CTRL-01 | Linux desktop | Conditional on display server / 4 | Opt-in app-scoped action; compositor/portal denial handled |
| CTRL-02 | Browser | Practical in isolated profile / 4 | Authorized navigation and bounded extraction; hostile page cannot grant access |
| CTRL-03 | Terminal and filesystem | Practical with sandbox / 4 | Command allowlist, bounded mounts/egress, traversal and process-tree kill |
| CTRL-04 | Git and containers | Practical with repository scope / 4–8 | Branch isolation, explicit push gate, no host Docker socket in agent sandbox |
| CTRL-05 | Remote servers | Conditional on credentials/network / 4–8 | Host-key checks, limited credentials, separate deployment approval |
| CTRL-06 | Android actions | Conditional on Android APIs / 6 | Runtime consent, background restrictions, revoked-permission tests |
| CTRL-07 | Home automation and external APIs | Conditional per integration / 8 | Explicit action scope, idempotency/reconciliation and emergency stop |
| ENG-01 | Inspect, branch, implement, test, review and PR | Practical / 8 | Seeded repo problem solved in sandbox; checks and diff attached |
| ENG-02 | Deployment assistance | Conditional / 8 | Approved artifact, rollback plan, no autonomous production mutation |
| PRO-01 | Monitoring, schedules, conditions and anomalies | Practical / 7 | Named data sources, persisted deduplication and bounded catch-up |
| PRO-02 | Suggestions, alerts and preauthorized workflows | Practical / 7 | Quiet hours/rate limits; explain trigger; revoked rule fails closed |
| UI-01 | Conversation, voice and agent/objective states | Practical / 6 | Real events only; reconnect and accessibility tests |
| UI-02 | Memory, systems, device status and live events | Practical / 6 | Provenance and last-observed times displayed; unavailable shown honestly |
| UI-03 | Permission requests and task timelines | Practical / 6 | Exact arguments displayed; stale grant rejected; ordered replay |
| UI-04 | 3D/spatial visualizations | Conditional / 8 | GPU/frame-time benchmarks, reduced-motion and textual equivalents |
| OPS-01 | Scoped credentials and untrusted-content boundaries | Practical / 1 onward | No model-issued grant; account/project isolation; AC-07, AC-26 |
| OPS-02 | Observability, reproducible deployment and recovery | Practical / 1 onward | Bounded metadata traces, clean container build and failure injection |

## Fictional capabilities without a credible parity claim

Open-ended reliable general intelligence, flawless prediction, omniscient sensing,
perfect intent recognition, instantaneous arbitrary discovery, unrestricted control
of any device, and guaranteed safe autonomous physical intervention are unsupported
objectives. Real systems need bounded task evaluations, user consent, supported
interfaces and physical safety controls. Spatial simulation may visualize validated
models; it cannot make invented physics or unknown sensor readings factual.

Do not score parity by attractive UI or conversation style. Every shipped row must
link to actual test evidence, provider/hardware configuration and known limitations.
