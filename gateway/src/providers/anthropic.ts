// Claude Messages streaming adapter (ADR-0004 candidate).
//
// The adapter speaks the Anthropic SSE wire format and normalizes it into the
// provider-neutral event stream. Parsing is a pure function so the mapping is
// tested against captured SSE lines without any network or credentials.
//
// Live qualification (recorded model, latencies, token usage) is a separate
// gate per docs/phase-0/technology.md; no claim about this provider is made
// until that runs.

import type { InferenceProvider, InferenceRequest, RawEvent } from "../types";

const ANTHROPIC_VERSION = "2023-06-01";
const ENDPOINT = "https://api.anthropic.com/v1/messages";

/** Convert one SSE `data:` payload into zero or one raw event. */
export function anthropicEventFromPayload(payload: unknown): RawEvent | null {
  if (typeof payload !== "object" || payload === null) return null;
  const frame = payload as Record<string, unknown>;
  const type = frame.type;

  if (type === "message_start") {
    return { type: "started", payload: { provider: "anthropic" } };
  }
  if (type === "content_block_delta") {
    const delta = frame.delta as Record<string, unknown> | undefined;
    if (delta?.type === "text_delta" && typeof delta.text === "string") {
      return { type: "content.delta", payload: { text: delta.text } };
    }
    return null;
  }
  if (type === "content_block_start") {
    const block = frame.content_block as Record<string, unknown> | undefined;
    if (block?.type === "tool_use") {
      // Tool use arrives incrementally; the proposal is emitted by the stream
      // loop once arguments are complete (see AnthropicProvider).
      return null;
    }
    return null;
  }
  if (type === "message_delta") {
    const usage = (frame.usage ?? frame.message?.usage) as Record<string, unknown> | undefined;
    if (usage) {
      return {
        type: "usage",
        payload: {
          inputTokens: usage.input_tokens ?? 0,
          outputTokens: usage.output_tokens ?? 0,
        },
      };
    }
    return null;
  }
  if (type === "message_stop") {
    return { type: "completed", payload: { stopReason: "end_turn" } };
  }
  if (type === "error") {
    const error = frame.error as Record<string, unknown> | undefined;
    return {
      type: "error",
      payload: { code: String(error?.type ?? "anthropic_error"), message: String(error?.message ?? "unknown") },
    };
  }
  if (type === "ping") return null;
  return null;
}

/** Collect a complete tool_use block from Anthropic's input_json_delta frames. */
export class AnthropicProvider implements InferenceProvider {
  readonly id = "anthropic";

  constructor(
    private readonly apiKey: string,
    private readonly baseUrl = ENDPOINT,
  ) {}

  async *stream(request: InferenceRequest, signal: AbortSignal): AsyncIterable<RawEvent> {
    const body = {
      model: request.model.name,
      max_tokens: request.maxOutputTokens,
      stream: true,
      system: request.system.map((s) => s.content).join("\n\n"),
      messages: request.messages.map((m) => ({
        role: m.role === "tool" ? "user" : m.role,
        content: m.content,
      })),
      tools: request.tools.map((t) => ({
        name: t.name,
        description: (t.schema as { description?: string }).description ?? "",
        input_schema: t.schema,
      })),
    };

    const response = await fetch(this.baseUrl, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-api-key": this.apiKey,
        "anthropic-version": ANTHROPIC_VERSION,
      },
      body: JSON.stringify(body),
      signal,
    });

    if (!response.ok || !response.body) {
      yield {
        type: "error",
        payload: { code: "http_error", message: `anthropic returned ${response.status}` },
      };
      return;
    }

    // Tool use arguments accumulate as input_json_delta frames until the block
    // closes; the core must only ever see a complete, schema-valid proposal.
    let toolName: string | null = null;
    let argumentDeltas: string[] = [];

    for await (const frame of sseFrames(response.body)) {
      if (signal.aborted) return;
      const payload = frame;
      const type = (payload as Record<string, unknown>).type;

      if (type === "content_block_start") {
        const block = (payload as Record<string, unknown>).content_block as Record<string, unknown>;
        if (block?.type === "tool_use") {
          toolName = String(block.name ?? "");
          argumentDeltas = [];
        }
        continue;
      }
      if (type === "content_block_delta") {
        const delta = (payload as Record<string, unknown>).delta as Record<string, unknown>;
        if (delta?.type === "input_json_delta" && typeof delta.partial_json === "string") {
          argumentDeltas.push(delta.partial_json);
          continue;
        }
        const event = anthropicEventFromPayload(payload);
        if (event) yield event;
        continue;
      }
      if (type === "content_block_stop" && toolName) {
        let parsed: unknown = null;
        try {
          parsed = JSON.parse(argumentDeltas.join("")) as unknown;
        } catch {
          parsed = null;
        }
        yield {
          type: "tool_use.proposal",
          payload: { name: toolName, arguments: parsed ?? {} },
        };
        toolName = null;
        argumentDeltas = [];
        continue;
      }

      const event = anthropicEventFromPayload(payload);
      if (event) yield event;
    }
  }
}

/** Parse an SSE response body into JSON payloads. */
export async function* sseFrames(body: ReadableStream<Uint8Array>): AsyncGenerator<Record<string, unknown>> {
  const decoder = new TextDecoder();
  let buffer = "";
  for await (const chunk of body) {
    buffer += decoder.decode(chunk, { stream: true });
    let boundary = buffer.indexOf("\n\n");
    while (boundary !== -1) {
      const frame = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      for (const line of frame.split("\n")) {
        if (!line.startsWith("data:")) continue;
        const data = line.slice(5).trim();
        if (!data || data === "[DONE]") continue;
        try {
          yield JSON.parse(data) as Record<string, unknown>;
        } catch {
          // A partial or malformed frame is not fatal to the stream.
        }
      }
      boundary = buffer.indexOf("\n\n");
    }
  }
}
