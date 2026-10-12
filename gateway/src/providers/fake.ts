// Fake provider for deterministic tests and local development.
//
// Scenario is chosen by model.name, e.g. provider "fake", name
// "plain" | "tool" | "error" | "stall" | "flood". No network, no credentials.

import type { InferenceProvider, InferenceRequest, RawEvent } from "../types";

export type FakeScenario = "plain" | "tool" | "error" | "stall" | "flood";

const SCENARIOS: FakeScenario[] = ["plain", "tool", "error", "stall", "flood"];

const TEXT_CHUNKS = [
  "Understood. ",
  "I read the note you asked about, ",
  "and here is the bounded summary you requested.",
];

export function parseScenario(name: string): FakeScenario {
  return (SCENARIOS as string[]).includes(name) ? (name as FakeScenario) : "plain";
}

export class FakeProvider implements InferenceProvider {
  readonly id = "fake";

  async *stream(request: InferenceRequest, signal: AbortSignal): AsyncIterable<RawEvent> {
    const scenario = parseScenario(request.model.name);
    const generationId = request.generationId;

    yield { type: "started", payload: { provider: this.id, model: request.model.name } };

    switch (scenario) {
      case "stall":
        // Emits one delta, then waits forever. The core's deadline aborts.
        yield { type: "content.delta", payload: { text: "starting…" } };
        await hang(signal);
        return;

      case "flood":
        // Unbounded deltas as fast as possible; the core's output limit must
        // stop this (never the provider).
        for (let i = 0; ; i++) {
          throwIfAborted(signal);
          yield { type: "content.delta", payload: { text: "x".repeat(64) } };
          if (i % 256 === 255) await Promise.resolve();
        }

      case "error":
        yield {
          type: "error",
          payload: { code: "provider_error", message: "fake provider failure", provider: this.id },
        };
        return;

      case "tool":
        yield { type: "content.delta", payload: { text: "I will propose reading the file." } };
        yield {
          type: "tool_use.proposal",
          payload: {
            proposalId: `prp_fake_${generationId.slice(-8)}`,
            name: request.tools[0]?.name ?? "workspace.read_text",
            arguments: { path: "notes.txt" },
          },
        };
        break;

      case "plain":
      default:
        for (const text of TEXT_CHUNKS) {
          throwIfAborted(signal);
          yield { type: "content.delta", payload: { text } };
        }
        break;
    }

    yield {
      type: "usage",
      payload: {
        inputTokens: 128,
        outputTokens: Math.ceil(64 + TEXT_CHUNKS.join("").length / 4),
      },
    };
    yield { type: "completed", payload: { stopReason: "end_turn" } };
  }
}

function throwIfAborted(signal: AbortSignal) {
  if (signal.aborted) {
    const err = new Error("aborted");
    err.name = "AbortError";
    throw err;
  }
}

async function hang(signal: AbortSignal): Promise<never> {
  return new Promise((_resolve, reject) => {
    if (signal.aborted) {
      reject(abortError());
      return;
    }
    signal.addEventListener(
      "abort",
      () => {
        reject(abortError());
      },
      { once: true },
    );
  });
}

function abortError(): Error {
  const err = new Error("The operation was aborted");
  err.name = "AbortError";
  return err;
}
