# Repository and environment audit

Observed on 2026-10-11 in `/home/developer/.t3/projects/j-a-r-v-i-s`.

| Item | Observation |
| --- | --- |
| Existing files | `README.md`, `assets/icon.svg`; both staged before this work |
| Git | `main`, no commits, no remotes; no existing application to preserve |
| OS | Linux x86_64, kernel 5.14, glibc 2.34 |
| Node.js | v22.23.3 available |
| Python | 3.9.25 available |
| GitHub CLI | 2.102.0 available; authentication not tested |
| Rust / Cargo | Not on PATH |
| Bun | Not on PATH |
| Docker / Podman | Not on PATH |
| PostgreSQL client | `psql` not on PATH; server availability not established |
| Model accounts / local inference | No credentials inspected; availability unverified |
| Camera / microphone / GPU / Android | Hardware and access unverified |

The implementation cannot be described as buildable or integration-tested in
this environment until these prerequisites are provisioned and exercised.
PR 1 must pin supported toolchains, lock dependencies and container images, and
record an actual clean build. The architecture package does not install them.

## Post-PR-1 provisioning (2026-10-12)

PR 1 provisioned and pinned the two required toolchains and recorded the
verification this audit could not perform on 2026-10-11:

| Item | Observation |
| --- | --- |
| Rust / Cargo | `rustc`/`cargo` 1.99.0 installed via rustup (minimal profile, clippy, rustfmt); pinned in `rust-toolchain.toml`; auto-install verified when entering the repository |
| Verified locally | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (1 pass), and `cargo build --workspace --locked` all pass on the pinned toolchain |
| Bun | 1.4.3 installed and pinned (`gateway/package.json` engines, CI `oven-sh/setup-bun@v2`); `bun install`, `bun test` (2 pass) and a boot smoke test of the loopback gateway all pass |
| Cargo.lock | Committed; dependency set limited to the ADR-0001/0002 baseline (Tokio, Axum, Serde, tracing) |
| PostgreSQL client | Still absent; PR-2 prerequisite remains open |
| Docker / Podman | Still absent; PR-4 sandbox prerequisite remains open |
| Model accounts | Still unverified; live qualification gate remains open |

The gateway skeleton deliberately exposes no inference route, so no provider
credentials were needed or used for this verification.

## Research tooling

The provider-directory CLI was unavailable initially. A checksum-verified Stripe
CLI release was downloaded to `/tmp/jarvis-directory-n4wyx4s8/stripe`; its Directory
plugin v0.3.5 installed successfully. Searching through the official CLI failed:

```text
Invalid user agent. This endpoint can only be used by the Stripe CLI tool.
```

No directory results are used as provider evidence. Primary technology documents
were consulted instead; no subscription, service, credentials, or payment was
provisioned. The plugin may retain its CLI-managed installation in the user profile;
no repository dependency on Stripe was added.

An independent delegated review examined the proposed security/recovery boundaries.
Its findings informed exact approval binding, authenticated loopback access,
race-resistant file opens, lease fencing, cancellation outcomes, and erasure policy.
This was a design review, not runtime verification.
