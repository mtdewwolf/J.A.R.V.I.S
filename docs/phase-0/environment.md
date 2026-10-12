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
