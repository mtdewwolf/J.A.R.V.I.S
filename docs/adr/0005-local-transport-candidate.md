# ADR-0005: Ollama as local transport candidate

- Status: Proposed
- Date: 2026-10-12
- Relates to: [technology.md](../phase-0/technology.md)

## Context

A local-only path is required so private content can be processed without
provider egress. The local engine must speak the same gateway contract with
streaming and declared tools.

## Decision

Implement the local adapter against the Ollama chat API as the *candidate*.
Individual models still require qualification; no local inference hardware is
assumed in this environment.

## Alternatives considered

- llama.cpp server: credible lower-level option; revisit if Ollama's tool-call
  or streaming behavior fails qualification on target hardware.
- Other local engines: deferred until the first two transports qualify.

## Consequences

- Local models must still pass injection-resistance tests; locality is not a
  safety property.
- Fallback from local to hosted requires the same egress policy and is not
  automatic (AC-16).
- Memory and model quality claims wait on the same evaluation protocol.

## Evidence required

Live local run on declared hardware with tool-format failures recorded;
latency/throughput numbers per candidate model.
