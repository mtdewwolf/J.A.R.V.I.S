import { expect, test } from "bun:test";
import { MAX_DELTAS_PER_STREAM, NonConformantEventError, estimateTokens, pump } from "./streaming";
import { FakeProvider } from "./providers/fake";
import type { InferenceEvent, InferenceProvider, InferenceRequest, RawEvent } from "./types";

const base: InferenceRequest = {
  contractVersion: "0.1",
  generationId: "g_0123456789abcdef",
  deadlineMs: 2000,
  maxTurns: 4,
  maxOutputTokens: 4096,
  model: { provider: "fake", name: "plain" },
  system: [],
  messages: [{ role: "user", content: "hi", untrusted: true }],
  tools: [],
};

function collector() {
  const events: InferenceEvent[] = [];
  return {
    events,
    emit(event: InferenceEvent) {
      events.push(event);
    },
  };
}

class ScriptedProvider implements InferenceProvider {
  constructor(
    readonly id: string,
    private readonly script: RawEvent[],
  ) {}

  async *stream(request: InferenceRequest, _signal: AbortSignal): AsyncIterable<RawEvent> {
    void request;
    for (const raw of this.script) yield raw;
  }
}

test("completion passes through with usage, and all events conform", async () => {
  const sink = collector();
  const result = await pump(base, new FakeProvider(), sink);
  expect(result.reason).toBe("completed");
  expect(result.aborted).toBe(false);
  const types = sink.events.map((e) => e.type);
  expect(types).toEqual(["started", "content.delta", "content.delta", "content.delta", "usage", "completed"]);
  for (const event of sink.events) {
    expect(event.contractVersion).toBe("0.1");
    expect(event.generationId).toBe(base.generationId);
  }
});

test("a stalled provider is stopped at the deadline with an aborted event", async () => {
  const sink = collector();
  const request = { ...base, model: { provider: "fake", name: "stall" }, deadlineMs: 150 };
  const started = Date.now();
  const result = await pump(request, new FakeProvider(), sink);
  expect(result.reason).toBe("deadline");
  expect(result.aborted).toBe(true);
  const last = sink.events.at(-1);
  expect(last?.type).toBe("aborted");
  expect((last?.payload as { reason?: string }).reason).toBe("deadline");
  expect(Date.now() - started).toBeLessThan(3000);
});

test("a flooding provider is cut off by the output cap, not by memory growth", async () => {
  const sink = collector();
  const request = { ...base, model: { provider: "fake", name: "flood" }, maxOutputTokens: 200 };
  const result = await pump(request, new FakeProvider(), sink);
  expect(result.reason).toBe("output_limit");
  const last = sink.events.at(-1);
  expect((last?.payload as { reason?: string }).reason).toBe("output_limit");
  // Bounded: the flood yielded far more data than was emitted.
  expect(result.events).toBeLessThan(2048);
});

test("an endless provider with a huge token cap still hits the delta ceiling", async () => {
  const sink = collector();
  const request = { ...base, model: { provider: "fake", name: "flood" }, maxOutputTokens: 10_000_000, deadlineMs: 30_000 };
  const result = await pump(request, new FakeProvider(), sink);
  expect(result.reason).toBe("delta_limit");
  expect(result.events).toBeLessThanOrEqual(MAX_DELTAS_PER_STREAM + 2);
});

test("a provider that ends without completion is reported, not silently closed", async () => {
  const sink = collector();
  const provider = new ScriptedProvider("scripted", [{ type: "content.delta", payload: { text: "partial" } }]);
  const result = await pump(base, provider, sink);
  expect(result.reason).toBe("provider_ended_without_completion");
  expect(sink.events.at(-1)?.type).toBe("aborted");
});

test("a provider error is forwarded with its message", async () => {
  const sink = collector();
  const provider = new ScriptedProvider("scripted", [
    { type: "error", payload: { code: "boom", message: "upstream said no" } },
  ]);
  const result = await pump(base, provider, sink);
  expect(result.reason).toBe("completed"); // the provider's own terminal event stands
  expect(sink.events.at(-1)?.type).toBe("error");
});

test("external abort (client disconnect) stops the stream", async () => {
  const sink = collector();
  const controller = new AbortController();
  const request = { ...base, model: { provider: "fake", name: "stall" }, deadlineMs: 30_000 };
  const running = pump(request, new FakeProvider(), sink, { externalSignal: controller.signal });
  setTimeout(() => controller.abort(new Error("client gone")), 50);
  const result = await running;
  expect(result.reason).toBe("client_disconnected");
  expect(sink.events.at(-1)?.type).toBe("aborted");
});

test("a non-conformant provider event is refused rather than emitted", async () => {
  const sink = collector();
  // payload must be an object per the schema; a string is a real violation.
  const broken = new ScriptedProvider("scripted", [{ type: "content.delta", payload: "not an object" as never }]);
  const result = await pump(base, broken, sink);
  expect(result.reason).toBe("non_conformant_event");
  // The offending event itself is never emitted; only the terminal error is.
  expect(sink.events).toHaveLength(1);
  expect(sink.events[0]?.type).toBe("error");
  expect((sink.events[0]?.payload as { reason: string }).reason).toBe("non_conformant_event");
  expect(new NonConformantEventError("x")).toBeInstanceOf(Error);
});

test("token estimation is a conservative over-estimate for ASCII", () => {
  expect(estimateTokens("")).toBe(0);
  expect(estimateTokens("abcd")).toBe(1);
  expect(estimateTokens("abcde")).toBe(2);
});
