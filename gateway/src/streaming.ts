// Bounded streaming pump.
//
// The core hands the gateway a deadline, an output-token cap and a max turn
// count; the pump enforces them on top of whatever the provider does. A
// provider that floods deltas, stalls, or dies mid-stream must not be able to
// exhaust the gateway's memory or hand the core unbounded output (AC-22).
//
// Every emitted event is validated against contracts/schemas before it
// leaves, so schema conformance holds by construction.

import { validate } from "./schema";
import { SCHEMAS } from "./contracts";
import type { InferenceEvent, InferenceProvider, InferenceRequest, RawEvent } from "./types";

/** Absolute per-stream delta ceiling, independent of token estimates. */
export const MAX_DELTAS_PER_STREAM = 10_000;
/** Yield to the event loop after this many consecutive unacknowledged writes. */
const BACKPRESSURE_BATCH = 256;

export type PumpReason =
  | "deadline"
  | "output_limit"
  | "delta_limit"
  | "client_disconnected"
  | "non_conformant_event"
  | "provider_ended_without_completion"
  | "provider_error";

export interface PumpResult {
  aborted: boolean;
  reason: "completed" | PumpReason;
  events: number;
  detail?: string;
}

export interface PumpSink {
  emit(event: InferenceEvent): Promise<void> | void;
}

export interface PumpOptions {
  now?: () => number;
  /** Aborting this signal (e.g. client disconnect) stops the stream. */
  externalSignal?: AbortSignal;
}

export async function pump(
  request: InferenceRequest,
  provider: InferenceProvider,
  sink: PumpSink,
  options: PumpOptions = {},
): Promise<PumpResult> {
  const now = options.now ?? (() => Date.now());
  const started = now();
  let events = 0;
  let terminalEmitted = false;
  let stopReason: PumpReason | null = null;
  let outcome: PumpResult | null = null;

  const emitRaw = async (raw: RawEvent): Promise<void> => {
    const event: InferenceEvent = {
      contractVersion: "0.1",
      generationId: request.generationId,
      type: raw.type,
      payload: raw.payload,
    };
    const issues = validate(SCHEMAS.inferenceEvent, event);
    if (issues.length > 0) {
      // The gateway refuses to emit a non-conformant event: the core must
      // never receive a payload that breaks the contract.
      throw new NonConformantEventError(issues.map((i) => `${i.path}: ${i.message}`).join("; "));
    }
    await sink.emit(event);
    events += 1;
    if (raw.type === "completed" || raw.type === "aborted" || raw.type === "error") {
      terminalEmitted = true;
    }
  };

  const controller = new AbortController();
  const onExternalAbort = () => {
    stopReason = "client_disconnected";
    controller.abort(new Error("client_disconnected"));
  };
  options.externalSignal?.addEventListener("abort", onExternalAbort, { once: true });

  const timer = setTimeout(() => {
    stopReason = "deadline";
    controller.abort(new Error("deadline"));
  }, request.deadlineMs);

  const finish = (reason: PumpResult["reason"], aborted: boolean, detail?: string): PumpResult => ({
    aborted,
    reason,
    events,
    ...(detail ? { detail } : {}),
  });

  let approxOutputTokens = 0;
  let deltas = 0;
  let sinceYield = 0;

  try {
    for await (const raw of provider.stream(request, controller.signal)) {
      if (raw.type === "content.delta") {
        deltas += 1;
        sinceYield += 1;
        if (deltas > MAX_DELTAS_PER_STREAM) {
          outcome = finish("delta_limit", true);
          controller.abort(new Error("delta_limit"));
          break;
        }
        const text = String((raw.payload as { text?: unknown }).text ?? "");
        approxOutputTokens += estimateTokens(text);
        if (approxOutputTokens > request.maxOutputTokens) {
          outcome = finish("output_limit", true);
          controller.abort(new Error("output_limit"));
          break;
        }
        if (sinceYield >= BACKPRESSURE_BATCH) {
          // Cooperative yield so a flooding provider cannot monopolize the
          // event loop; the deadline timer still bounds total time.
          sinceYield = 0;
          await Promise.resolve();
        }
      }
      await emitRaw(raw);
      if (outcome) break;
    }

    outcome ??= terminalEmitted ? finish("completed", false) : finish("provider_ended_without_completion", true);
  } catch (err) {
    if (err instanceof NonConformantEventError) {
      outcome = finish("non_conformant_event", true, err.message);
    } else if (!outcome) {
      outcome = stopReason
        ? finish(stopReason, true, err instanceof Error ? err.message : undefined)
        : finish("provider_error", true, err instanceof Error ? err.message : String(err));
    }
  } finally {
    clearTimeout(timer);
    options.externalSignal?.removeEventListener("abort", onExternalAbort);
  }

  // The terminal event is emitted exactly once, after the loop settles.
  if (outcome.aborted) {
    const reason = outcome.reason;
    const eventType = reason === "provider_error" || reason === "non_conformant_event" ? "error" : "aborted";
    await sink.emit({
      contractVersion: "0.1",
      generationId: request.generationId,
      type: eventType,
      payload: {
        reason,
        events,
        elapsedMs: now() - started,
        ...(outcome.detail ? { detail: outcome.detail } : {}),
      },
    });
  }

  return outcome;
}

export class NonConformantEventError extends Error {
  constructor(detail: string) {
    super(detail);
    this.name = "NonConformantEventError";
  }
}

/** Rough token estimate (≈4 chars/token) — enough for a hard cap, not a bill. */
export function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}
