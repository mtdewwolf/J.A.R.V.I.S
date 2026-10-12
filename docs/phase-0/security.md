# Security and permissions model

## Threat boundary

Protect owner data, credentials, task integrity, devices and external recipients.
Treat models, documents, retrieved memories, webpages and gateway output as
untrusted content. Also consider unauthenticated local network requests, stale
workers, cross-session confusion, crashes and malicious filenames. Initial trust
includes the installed core binary, owner OS account, admin and kernel. A malicious
same-user process can often read local credentials; defending against a compromised
owner account requires a stronger OS boundary and is outside Phase 1.

Loopback is a network restriction, not identity. Use an owner-generated bearer
credential stored with restrictive permissions, never URLs; reject unknown Host
and Origin values and unauthenticated commands. CLI does not use ambient cookies.
Model gateway has a distinct service credential and only an inference endpoint.
Browser clients later need explicit device pairing and origin/session protection.

## Grant lifecycle

1. Core resolves a model proposal against a registered tool/version and canonical
   schema-valid arguments. Unknown tools, extra fields, duplicate JSON keys and
   invalid resource identifiers fail before approval.
2. Authenticated client displays tool, resource, exact args, risk, outbound-data
   implications, expiry and one-use constraint. Natural-language “yes” from a
   model or document cannot approve anything.
3. Owner approval creates a DB grant bound to owner, session, task, generation,
   proposal ID, tool/version, RFC 8785 canonical argument SHA-256 digest, resource
   scope, policy version, expiry and `max_uses=1`. Use tested canonicalization and
   cross-language vectors, not ordinary `JSON.stringify` as a signing format.
   [RFC 8785](https://www.rfc-editor.org/info/rfc8785/)
4. Immediately before dispatch, core checks every binding, current cancellation,
   expiry and revocation. Consume grant and persist execution intent atomically.
   Concurrent requests cannot consume it twice. Failed validation cannot partially
   consume a grant. Policy upgrades invalidate pending incompatible grants.
5. Log requested/denied/authorized/dispatched/outcome metadata with IDs and reason
   codes. No secret args or plaintext bodies in audit metadata; sensitive hashes
   use keyed audit digests where exposed. Grant state is not model-editable.

Expiry/revocation govern starting new operations. Already-started actions cannot
always be revoked retroactively; cancel attempts and actual outcomes are distinct.
For the initial bounded read, cancellation stops consuming output and prevents
stale result publication, continuation or memory writes.

## Risk tiers

| Tier | Example | Required control |
| --- | --- | --- |
| R0 | Read local task state, explicitly confirmed memory | Authenticated owner; scoped query and audit; provider disclosure separate |
| R1 | Read an exported text file | Exact proposal grant; no ambient whole-workspace permission |
| R2 | Reversible sandbox edits, test execution | Bounded reviewed capability grant, quotas, sandbox, reversible artifacts |
| R3 | Send message, push code, external account mutation | Exact recipient/diff and action approval; scoped credential; reconciliation |
| R4 | Destructive, financial, physical safety or production action | Strong authentication, explicit confirmation and purpose-specific controls |

Only R0/R1 are included initially. Each future adapter declares risk and limits in
trusted registry metadata; the model cannot downgrade risk. Scoped grants may be
configured later for repeated low-risk workflows, with duration/use bounds and
revocation. Approval to run a tool does not approve disclosure to a new provider.

## Initial tool boundary

`workspace.read_text` reads one relative UTF-8 file from a dedicated owner-prepared
export directory, at most 64 KiB in 2 seconds. The approval covers the named path's
contents at dispatch, not an immutable snapshot; present this clearly. Root is an
allowlisted configuration ID, never a model-provided absolute path. Credentials,
`.env`, `.git`, key directories and hidden files are excluded; only deliberately
exported nonsensitive text belongs in the root. Gateway has no mount of this root.

On Linux use descriptor-relative race-resistant opening (`openat2` with beneath,
no-symlink and no-magic-link resolution, or an equivalently checked implementation).
Reject absolute paths, traversal, symlinks, unexpected mount transitions, hardlinks
(`nlink > 1`), special files and non-UTF-8 bytes. Inspect the opened descriptor rather
than validating a path and reopening it. Fail closed if the required primitive is
unavailable. Test path replacement during open, byte limits and cancellation.

Core container mounts only the exported root read-only, has no host shell tool,
no Docker socket and no general home mount. Gateway container is nonroot with
read-only image, no DB credential/network route and no workspace mount. Provider
credentials belong only to gateway transport; control outbound endpoints/egress
in deployment configuration. Future external-write credentials live in isolated
adapters; policy checks alone do not stop a compromised worker that owns such keys.

## Memory and erasure

Confirm the exact structured key/value, epistemic state, source IDs and scope through
an authenticated command. Changing candidate content invalidates confirmation.
Inference stays labeled; conflict creates supersession or asks the owner, never
silently upgrades belief. Enforce user/scope filtering before decrypting.

Encrypt private payloads using a vetted authenticated-encryption implementation,
random nonces and versioned key identifiers; enforce restrictive key files and
record rotation procedures. M1 key storage is outside DB/backups, without promising
protection against a compromised owner OS account. Restart without a key fails
closed. Deletion scrubs memory, derived context, referenced source messages and
payloads; retained audit metadata has no private content. Default private-payload
retention is 30 days unless explicitly pinned; candidate retention is 24 hours.
Audit metadata retention is 90 days. Development backups are off by default.

Before enabling backups declare maximum 30-day expiry and test a restore that
applies an independently retained erasure ledger before accepting traffic. Online
deletion acknowledgement distinguishes erased active data from expiring backup
copies. Never claim erased content cannot be reconstituted from an old wrapped
key/payload copy. Future indexes/artifacts must join this same deletion inventory.

## Failure cases and controls

| Threat | Enforcement and evidence |
| --- | --- |
| Prompt injection or fabricated approval | Proposal-only model output, authenticated grants; AC-07 |
| Replay / args substitution / concurrent consumption | Exact bindings, uniqueness, atomic consume+intent; AC-04–06 |
| File traversal / TOCTOU / disclosure | Descriptor-relative open and exported root; AC-08–10, AC-28 |
| API impersonation / cross-owner leakage | Auth, Host/Origin and scope checks; AC-03, AC-26 |
| Runaway planning / model stall | Turn/token/time limits, abort and bounded buffers; AC-21–22 |
| Crash or stale execution | Checkpoints, fences and fresh recovery approval; AC-17–20, AC-23, AC-25 |
| Private content in audit/backups | Encrypted reference storage, erasure ledger/retention; AC-14–15, AC-27 |
| Gateway compromise | No DB/workspace access or execution endpoint; AC-24 |
| Unsafe fallback | Same privacy policy, configured provider only; AC-16, AC-29 |

Security logging failure or unavailable durable state prevents dispatch. Keep audit
metadata tamper-resistant within DB privileges, but do not call it tamper-proof
against database administrators. Distributed signed audit anchoring is deferred.
