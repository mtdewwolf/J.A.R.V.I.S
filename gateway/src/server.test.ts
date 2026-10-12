import { expect, test } from "bun:test";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { makeFetchHandler } from "./server";
import { buildProviders } from "./providers";
import type { InferenceEvent } from "./types";

function bootGateway() {
  const providers = buildProviders({});
  const server = Bun.serve({ port: 0, hostname: "127.0.0.1", fetch: makeFetchHandler(providers) });
  return { base: `http://127.0.0.1:${server.port}`, stop: () => server.stop(true) };
}

const validRequest = {
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

async function readSse(response: Response): Promise<InferenceEvent[]> {
  const text = await response.text();
  return text
    .split("\n\n")
    .filter((frame) => frame.startsWith("data: "))
    .map((frame) => JSON.parse(frame.slice(6)) as InferenceEvent);
}

test("streams a conformant event sequence for a valid request", async () => {
  const { base, stop } = bootGateway();
  try {
    const response = await fetch(`${base}/v1/inference/stream`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(validRequest),
    });
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toContain("text/event-stream");
    const events = await readSse(response);
    expect(events.map((e) => e.type)).toEqual([
      "started",
      "content.delta",
      "content.delta",
      "content.delta",
      "usage",
      "completed",
    ]);
    for (const event of events) {
      expect(event.contractVersion).toBe("0.1");
      expect(event.generationId).toBe(validRequest.generationId);
    }
  } finally {
    stop();
  }
});

test("violating the request budget is refused before any provider call", async () => {
  const { base, stop } = bootGateway();
  try {
    const response = await fetch(`${base}/v1/inference/stream`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ...validRequest, maxTurns: 9 }),
    });
    expect(response.status).toBe(400);
    const body = (await response.json()) as { error: { code: string; issues: unknown[] } };
    expect(body.error.code).toBe("invalid_request");
    expect(body.error.issues.length).toBeGreaterThan(0);
  } finally {
    stop();
  }
});

test("unknown providers and malformed bodies fail with structured errors", async () => {
  const { base, stop } = bootGateway();
  try {
    const unknown = await fetch(`${base}/v1/inference/stream`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ ...validRequest, model: { provider: "not-registered", name: "x" } }),
    });
    expect(unknown.status).toBe(400);
    expect(((await unknown.json()) as { error: { code: string } }).error.code).toBe("unknown_provider");

    const malformed = await fetch(`${base}/v1/inference/stream`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: "this is not json",
    });
    expect(malformed.status).toBe(400);
    expect(((await malformed.json()) as { error: { code: string } }).error.code).toBe("invalid_json");
  } finally {
    stop();
  }
});

test("no execution route exists; only health and inference are served", async () => {
  const { base, stop } = bootGateway();
  try {
    const health = await fetch(`${base}/healthz`);
    expect(health.status).toBe(200);
    expect(await health.text()).toContain("ok");

    for (const path of ["/db", "/exec", "/tools/workspace.read_text", "/v1/inference/stream"]) {
      const response = await fetch(`${base}${path}`); // GET: wrong shape or absent route
      expect(response.status).toBe(501);
    }
  } finally {
    stop();
  }
});

test("AC-24 isolation: no database client, no workspace access, no credentials", () => {
  const srcDir = join(import.meta.dir, ".");
  const files: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === "node_modules" || entry.name.startsWith(".")) continue;
      const path = join(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (!entry.name.includes(".test.")) files.push(path);
    }
  };
  walk(srcDir);

  const banned = [/DATABASE_URL/i, /postgres:\/\//i, /\bpg\b.*Client/i, /Bun\.file\(/i, /createClient/i];
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    for (const pattern of banned) {
      expect(text.match(pattern)?.[0] ?? null, `${file} matches ${pattern}`).toBeNull();
    }
  }

  const pkg = JSON.parse(readFileSync(join(import.meta.dir, "..", "package.json"), "utf8")) as {
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
  };
  expect(Object.keys(pkg.dependencies ?? {}).length, "no runtime dependencies: no DB driver is possible").toBe(0);
  expect(Object.keys(pkg.devDependencies ?? {}).length).toBe(0);
});
