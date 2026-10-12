# Interface contracts

Single source of truth for structured payloads shared by `jarvis-core`,
`gateway/` and clients, as defined in
[phase-0/protocol.md](../docs/phase-0/protocol.md).

## Files

| Schema | Payload | Used by |
| --- | --- | --- |
| [event-envelope.schema.json](schemas/event-envelope.schema.json) | Event envelope for client protocol and replay | Core, CLI |
| [client-command.schema.json](schemas/client-command.schema.json) | Authenticated commands (`input.send`, `approval.grant`, …) | Core, CLI |
| [inference-request.schema.json](schemas/inference-request.schema.json) | Provider-neutral inference request | Core, gateway |
| [inference-event.schema.json](schemas/inference-event.schema.json) | Gateway streaming events | Gateway, core |
| [tool-proposal.schema.json](schemas/tool-proposal.schema.json) | Model tool proposal awaiting approval | Gateway, core |
| [permission-grant.schema.json](schemas/permission-grant.schema.json) | One-use owner approval grant | Core |
| [memory-record.schema.json](schemas/memory-record.schema.json) | Semantic memory record | Core |

## Conventions

- JSON Schema draft 2020-12, `additionalProperties: false` everywhere: unknown
  fields are validation failures, not silently ignored.
- Canonical form for digests is RFC 8785 (JCS); the argument digest in a grant
  is SHA-256 over the canonical JSON, cross-language vectors required before
  use (AC-04). Do not use `JSON.stringify` as the signing format.
- Contract version `0.1`; additive changes only within a major version.
- Tests validate core and gateway output against these files; a schema change
  without a protocol change is rejected in review.

## Known-answer requirement

`tests/contract-vectors/` (added with PR-4) holds cross-language canonicalization
vectors: same input, same digest, from Rust and TypeScript, before the grant
binding is trusted.
