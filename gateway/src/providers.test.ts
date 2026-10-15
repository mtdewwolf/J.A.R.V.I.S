import { expect, test } from "bun:test";
import { FakeProvider, parseScenario } from "./providers/fake";
import { anthropicEventFromPayload } from "./providers/anthropic";
import { ollamaEventFromLine } from "./providers/ollama";
import type { InferenceRequest } from "./types";

const request: InferenceRequest = {
  contractVersion: "0.1",
  generationId: "g_0123456789abcdef",
  deadlineMs: 5000,
  maxTurns: 4,
  maxOutputTokens: 4096,
  model: { provider: "fake", name: "plain" },
  system: [],
  messages: [{ role: "user", content: "hi", untrusted: true }],
  tools: [{ name: "workspace.read_text", version: 1, schema: { type: "object" } }],
};

async function collect(iterable: AsyncIterable<unknown>): Promise<unknown[]> {
  const out: unknown[] = [];
  for await (const item of iterable) out.push(item);
  return out;
}

test("fake plain scenario completes with usage", async () => {
  const events = (await collect(new FakeProvider().stream(request, new AbortController().signal))) as {
    type: string;
    payload: Record<string, unknown>;
  }[];
  expect(events.map((e) => e.type)).toEqual(["started", "content.delta", "content.delta", "content.delta", "usage", "completed"]);
  expect(events.at(-1)?.payload.stopReason).toBe("end_turn");
});

test("fake tool scenario emits one complete proposal", async () => {
  const toolRequest = { ...request, model: { provider: "fake", name: "tool" } };
  const events = (await collect(new FakeProvider().stream(toolRequest, new AbortController().signal))) as {
    type: string;
    payload: Record<string, unknown>;
  }[];
  const proposal = events.find((e) => e.type === "tool_use.proposal");
  expect(proposal).toBeDefined();
  expect(proposal?.payload).toMatchObject({ name: "workspace.read_text", arguments: { path: "notes.txt" } });
});

test("fake error scenario surfaces a provider error", async () => {
  const errRequest = { ...request, model: { provider: "fake", name: "error" } };
  const events = (await collect(new FakeProvider().stream(errRequest, new AbortController().signal))) as { type: string }[];
  expect(events.map((e) => e.type)).toEqual(["started", "error"]);
});

test("fake stall stops promptly when the deadline aborts the signal", async () => {
  const stallRequest = { ...request, model: { provider: "fake", name: "stall" } };
  const controller = new AbortController();
  const started = Date.now();
  const streaming = new FakeProvider().stream(stallRequest, controller.signal);
  // Drain in the background and abort shortly after start. Providers signal
  // aborts by rejecting; the pump (streaming.ts) is what converts that into a
  // conformant aborted event, so the raw generator rejection is expected here.
  const drained = collect(streaming);
  setTimeout(() => controller.abort(new Error("deadline")), 50);
  await expect(drained).rejects.toThrow("The operation was aborted");
  expect(Date.now() - started).toBeLessThan(2000);
});

test("unknown fake scenarios fall back to plain", () => {
  expect(parseScenario("plain")).toBe("plain");
  expect(parseScenario("something-else")).toBe("plain");
});

test("anthropic SSE frames map to neutral events", () => {
  expect(anthropicEventFromPayload({ type: "message_start" })).toEqual({ type: "started", payload: { provider: "anthropic" } });
  expect(anthropicEventFromPayload({ type: "content_block_delta", delta: { type: "text_delta", text: "hi" } })).toEqual({
    type: "content.delta",
    payload: { text: "hi" },
  });
  // input_json_delta frames accumulate in the provider and are not events.
  expect(anthropicEventFromPayload({ type: "content_block_delta", delta: { type: "input_json_delta", partial_json: "{" } })).toBeNull();
  expect(anthropicEventFromPayload({ type: "message_delta", usage: { output_tokens: 12 } })).toEqual({
    type: "usage",
    payload: { inputTokens: 0, outputTokens: 12 },
  });
  expect(anthropicEventFromPayload({ type: "message_stop" })).toEqual({ type: "completed", payload: { stopReason: "end_turn" } });
  expect(anthropicEventFromPayload({ type: "error", error: { type: "overloaded", message: "busy" } })).toMatchObject({
    type: "error",
    payload: { code: "overloaded", message: "busy" },
  });
  expect(anthropicEventFromPayload({ type: "ping" })).toBeNull();
});

test("ollama NDJSON lines map to neutral events", () => {
  expect(ollamaEventFromLine(JSON.stringify({ message: { content: "hi" } }))).toEqual({ type: "content.delta", payload: { text: "hi" } });
  expect(
    ollamaEventFromLine(JSON.stringify({ message: { tool_calls: [{ function: { name: "workspace.read_text", arguments: { path: "a" } } }] } })),
  ).toEqual({
    type: "tool_use.proposal",
    payload: { name: "workspace.read_text", arguments: { path: "a" } },
  });
  expect(ollamaEventFromLine(JSON.stringify({ done: true, done_reason: "stop", prompt_eval_count: 10, eval_count: 20 }))).toEqual({
    type: "completed",
    payload: { stopReason: "stop", usage: { inputTokens: 10, outputTokens: 20 } },
  });
  expect(ollamaEventFromLine("not json")).toBeNull();
  expect(ollamaEventFromLine(JSON.stringify({ error: "model missing" }))).toMatchObject({ type: "error" });
});

test("Ollama final content frame retains completion, and malformed arguments fail closed", async () => {
  const { ollamaEventsFromLine }=await import("./providers/ollama");
  expect(ollamaEventsFromLine(JSON.stringify({message:{content:"final text"},done:true,done_reason:"stop"})).map(e=>e.type)).toEqual(["content.delta","completed"]);
  expect(()=>ollamaEventsFromLine('{"message":{"tool_calls":[]},"done":true,"done":false}')).toThrow();
});
test("Anthropic SSE accepts CRLF frame boundaries", async () => {
  const { sseFrames }=await import("./providers/anthropic");
  const response=new Response('data: {"type":"ping"}\r\n\r\ndata: {"type":"message_stop"}\r\n\r\n');
  const frames=[];for await(const frame of sseFrames(response.body!))frames.push(frame.type);
  expect(frames).toEqual(["ping","message_stop"]);
});
