// Typed mirrors of contracts/schemas/*.json.
//
// The schemas are the source of truth (contracts/README.md); these types exist
// for ergonomics. Any divergence is caught by the schema-conformance tests,
// because every inbound request and outbound event is validated against the
// schemas at runtime.

export interface InferenceRequest {
  contractVersion: "0.1";
  generationId: string;
  deadlineMs: number;
  maxTurns: number;
  maxOutputTokens: number;
  model: { provider: string; name: string };
  system: { role: "system"; content: string }[];
  messages: { role: "user" | "assistant" | "tool"; content: string; untrusted: true }[];
  tools: { name: string; version: number; schema: Record<string, unknown> }[];
}

export type InferenceEventType =
  | "started"
  | "content.delta"
  | "tool_use.proposal"
  | "usage"
  | "completed"
  | "aborted"
  | "error";

export interface InferenceEvent {
  contractVersion: "0.1";
  generationId: string;
  type: InferenceEventType;
  payload: Record<string, unknown>;
}

/** A provider-emitted event before the contract envelope is attached. */
export interface RawEvent {
  type: InferenceEventType;
  payload: Record<string, unknown>;
}

export interface InferenceProvider {
  readonly id: string;
  /**
   * Stream raw events for a request. `signal` aborts on the request deadline
   * (or client disconnect) and providers MUST stop promptly.
   */
  stream(request: InferenceRequest, signal: AbortSignal): AsyncIterable<RawEvent>;
}
