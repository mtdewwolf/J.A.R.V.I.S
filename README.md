<img src="assets/icon.svg" width="64" height="64" alt="">

# J.A.R.V.I.S

Created in [T3 Code](https://t3.codes).

A persistent, permission-controlled multimodal assistant. Core systems use Rust;
services and integrations use TypeScript on Bun; durable state uses PostgreSQL.

**Current state:** Phase 0 architecture proposal. No assistant runtime exists yet.
No model, voice, device-control, memory, or recovery capability is implemented.

Start with the [engineering package](docs/phase-0/README.md), which contains the
capability matrix, architecture, technology decisions, threat model, roadmap,
repository layout, five proposed implementation PRs, and initial acceptance contracts.

Validate the package using the available Node.js runtime:

```sh
node scripts/validate-phase0.mjs
```

This checks documentation and contract consistency. It does not test a running assistant.
