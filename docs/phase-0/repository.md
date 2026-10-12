# Repository structure

Proposed layout. PR-1 in [pull-requests.md](pull-requests.md) creates the
skeleton and pins; later PRs fill the modules. The layout enforces the
boundaries in [architecture.md](architecture.md): one trusted core service, one
isolated transport process, shared typed contracts, no per-subsystem
microservices.

```text
.
├── Cargo.toml                    # workspace manifest; core + CLI crates only
├── rust-toolchain.toml           # pinned toolchain (PR-1 prerequisite)
├── crates/
│   ├── jarvis-core/              # trusted core: owns authorization, dispatch,
│   │   ├── Cargo.toml            # durable transitions. Axum HTTP is transport
│   │   └── src/                  # plumbing, not authority.
│   │       ├── main.rs
│   │       ├── conversation.rs   # input context, generation, continuation
│   │       ├── task.rs           # state machine, leases, fences, retries
│   │       ├── policy.rs         # grants, revocations, dispatch checks
│   │       ├── tools/
│   │       │   ├── mod.rs        # registry: versioned schema + limits
│   │       │   └── workspace_read_text.rs  # bounded R1 read, race-resistant open
│   │       ├── memory.rs         # candidate/confirm, retrieval, supersession
│   │       ├── events.rs         # event store access, checkpoints, cursor
│   │       └── client_protocol.rs# authenticated commands, replay cursor
│   └── jarvis-cli/               # authenticated local CLI client
│       ├── Cargo.toml
│       └── src/main.rs
├── gateway/                      # Bun model gateway: untrusted proposals only
│   ├── package.json              # no DB credential, no workspace mount,
│   └── src/                      # no execution endpoint
│       ├── server.ts             # bounded streaming HTTP surface
│       └── providers/            # per-provider adapters behind one contract
├── contracts/                    # single source of typed interface contracts
│   ├── README.md
│   └── schemas/                  # JSON Schema (draft 2020-12) payload files
├── migrations/                   # SQLx migrations: events, tasks, grants, memory
├── tests/
│   └── acceptance/
│       └── initial-slice.feature # Gherkin slice contracts (AC-01–AC-26, AC-28)
├── docs/
│   ├── adr/                      # numbered architecture decision records
│   └── phase-0/                  # engineering package
├── deploy/                       # pinned images, minimal Compose, rootless notes
├── scripts/
│   └── validate-phase0.mjs       # documentation/contract consistency check
├── .gitignore
├── LICENSE
└── README.md                     # project entry point
```

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
