# Repository structure

Proposed layout. PR-1 in [pull-requests.md](pull-requests.md) creates the
skeleton and pins; later PRs fill the modules. The layout enforces the
boundaries in [architecture.md](architecture.md): one trusted core service, one
isolated transport process, shared typed contracts, no per-subsystem
microservices.

```text
.
├── Cargo.toml                    # workspace manifest; core + CLI crates only
├── Cargo.lock                    # committed; pinned dependency set
├── rust-toolchain.toml           # pinned toolchain with clippy/rustfmt
├── .github/workflows/ci.yml      # docs, Rust and gateway gates
├── .gitignore
├── LICENSE                        # MIT
├── crates/
│   ├── jarvis-core/              # trusted core: owns authorization, dispatch,
│   │   ├── Cargo.toml            # durable transitions. Axum HTTP is transport
│   │   └── src/                  # plumbing, not authority.
│   │       ├── main.rs
│   │       ├── limits.rs         # documented default constraints + unit test
│   │       ├── store.rs          # pool, migrations, ssl policy
│   │       ├── commands.rs       # idempotent ingestion (AC-05)
│   │       ├── events.rs         # content-free events, replay cursor
│   │       ├── tasks.rs          # CAS state machine, steps, execution intent
│   │       ├── leases.rs         # single-worker lease, monotonic fencing
│   │       ├── recovery.rs       # restart sweep (AC-17–AC-19, AC-23)
│   │       ├── conversation.rs   # input context, generation, continuation
│   │       ├── policy.rs         # grants, revocations, dispatch checks
│   │       ├── tools/
│   │       │   ├── mod.rs        # registry: versioned schema + limits
│   │       │   └── workspace_read_text.rs  # bounded R1 read, race-resistant open
│   │       ├── memory.rs         # candidate/confirm, retrieval, supersession
│   │       ├── event_store.rs    # event store access, checkpoints, cursor
│   │       └── client_protocol.rs# authenticated commands, replay cursor
│   └── jarvis-cli/               # authenticated local CLI client
│       ├── Cargo.toml
│       └── src/main.rs
├── gateway/                      # Bun model gateway: untrusted proposals only
│   ├── package.json              # no DB credential, no workspace mount,
│   └── src/                      # no execution endpoint
│       ├── server.ts             # health-only skeleton until PR-3
│       ├── server.test.ts
│       └── providers/            # per-provider adapters behind one contract
├── contracts/                    # single source of typed interface contracts
│   ├── README.md
│   └── schemas/                  # JSON Schema (draft 2020-12) payload files
├── migrations/                   # SQLx migrations: events, tasks, grants, memory
│   └── 0001_init.sql
├── tests/
│   └── acceptance/
│       └── initial-slice.feature # Gherkin slice contracts (AC-01–AC-26, AC-28)
├── docs/
│   ├── adr/                      # numbered architecture decision records
│   └── phase-0/                  # engineering package
├── deploy/                       # pinned images, minimal Compose, rootless notes
├── scripts/
│   └── validate-phase0.mjs       # documentation/contract consistency check
├── README.md                     # project entry point
```

Present as of PR-2: workspace manifests, toolchain pin, crate skeletons with the
durable slice (`store`, `commands`, `events`, `tasks`, `leases`, `recovery`,
`limits`), `migrations/0001_init.sql`, the gateway health skeleton, CI (with a
PostgreSQL service job), contracts, docs, feature file, validator and LICENSE.
Marked empty here for later PRs: `conversation.rs`, `policy.rs`, `tools/`,
`memory.rs`, `event_store.rs`, `client_protocol.rs`,
`gateway/src/providers/`, `deploy/`.

## Layout rules

- `crates/jarvis-core` is the only trusted component. Policy, memory writes and
  durable transitions may not be implemented in `gateway/` or clients.
- `gateway/` holds provider adapters and transport normalization only. A
  review may reject any gateway change that touches files, database drivers,
  credentials for non-inference services, or an execution route.
- `contracts/schemas/` is the shared source of truth for client protocol and
  inference payloads; core, gateway and tests validate against these files, not
  private copies of the types.
- `migrations/` is the only schema-change path for durable state; state
  transitions plus their events land in one migration-reviewed schema.
- Acceptance features reference contract IDs (`@AC-01`); the validator rejects
  files that reference unknown IDs or codes without steps.
