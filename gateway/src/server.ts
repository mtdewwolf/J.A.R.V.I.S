// Model transport gateway server.
//
// Trust boundary (docs/phase-0/security.md): the gateway transports model
// input/output. It cannot execute tools, write memory, access the database or
// mount the workspace. There is no database client in the dependency tree,
// no workspace path is ever read, and the only routes are a health probe and
// the inference stream — no execution route exists (AC-24).
//
// The listener binds loopback only; authentication happens in the core before
// a request reaches here.

import { strictParse } from "./strict-json";
import { timingSafeEqual } from "node:crypto";
import { readFileSync, lstatSync } from "node:fs";
import { validate } from "./schema";
import { SCHEMAS } from "./contracts";
import { buildProviders } from "./providers";
import { pump } from "./streaming";
import type { InferenceEvent, InferenceProvider, InferenceRequest } from "./types";

export interface GatewayOptions {
  hostname?: string;
  port?: number;
  env?: Record<string, string | undefined>;
}

/** Non-stream routes: a health probe and an explicit "not implemented". */
export function handleRequest(req: Request): Response {
  const url = new URL(req.url);
  if (url.pathname === "/healthz") {
    return new Response("ok\n", { headers: { "content-type": "text/plain" } });
  }
  return new Response("jarvis gateway: no such route; inference contract is POST /v1/inference/stream\n", {
    status: 501,
    headers: { "content-type": "text/plain" },
  });
}

/** POST /v1/inference/stream: validate, select provider, stream SSE back. */
export async function handleInferenceStream(
  req: Request,
  providers: Map<string, InferenceProvider>,
): Promise<Response> {
  let body: unknown;
  try {
    body = strictParse(await req.text());
  } catch {
    return jsonError(400, "invalid_json", "request body is not valid JSON");
  }

  const issues = validate(SCHEMAS.inferenceRequest, body);
  if (issues.length > 0) {
    return jsonError(400, "invalid_request", "request failed contract validation", issues);
  }
  const request = body as InferenceRequest;

  const provider = providers.get(request.model.provider);
  if (!provider) {
    return jsonError(400, "unknown_provider", `no provider registered for '${request.model.provider}'`);
  }

  const encoder = new TextEncoder();
  const disconnect = new AbortController();
  req.signal.addEventListener("abort", () => disconnect.abort(), { once: true });
  if (req.signal.aborted) disconnect.abort();
  const clientSignal = disconnect.signal;
  let streamController: ReadableStreamDefaultController<Uint8Array> | null = null;

  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      streamController = controller;
    },
    cancel() {
      disconnect.abort();
    },
  });

  const sink = {
    emit(event: InferenceEvent) {
      try {
        streamController?.enqueue(encoder.encode(`data: ${JSON.stringify(event)}\n\n`));
      } catch {
        // Stream already closed (client disconnected); nothing to do.
      }
    },
  };

  // Run the pump without blocking the response: it closes the stream when done.
  void pump(request, provider, sink, { externalSignal: clientSignal })
    .catch(() => undefined)
    .finally(() => {
      try {
        streamController?.close();
      } catch {
        // Already closed.
      }
    });

  return new Response(stream, {
    headers: {
      "content-type": "text/event-stream; charset=utf-8",
      "cache-control": "no-cache, no-transform",
      connection: "keep-alive",
      "x-accel-buffering": "no",
    },
  });
}

function jsonError(
  status: number,
  code: string,
  message: string,
  issues?: { path: string; message: string }[],
): Response {
  return new Response(
    JSON.stringify({ error: { code, message, ...(issues ? { issues } : {}) } }),
    { status, headers: { "content-type": "application/json" } },
  );
}

/** Build the fetch handler for a gateway with the given providers. */
export function makeFetchHandler(providers: Map<string, InferenceProvider>, token?: string) {
  return async function fetchHandler(req: Request): Promise<Response> {
    const url = new URL(req.url);
    if (req.method === "POST" && url.pathname === "/v1/inference/stream") {
      if (token) {
        const a = Buffer.from(req.headers.get("authorization") ?? "");
        const b = Buffer.from(`Bearer ${token}`);
        if (a.length !== b.length || !timingSafeEqual(a,b)) return jsonError(401,"unauthorized","gateway authentication required");
      }
      if (req.headers.has("origin")) return jsonError(403,"forbidden","browser requests are not allowed");
      return handleInferenceStream(req, providers);
    }
    return handleRequest(req);
  };
}

if (import.meta.main) {
  const port = Number(process.env.JARVIS_GATEWAY_PORT ?? 8787);
  const config = process.env.JARVIS_CONFIG_DIR ?? `${process.env.HOME}/.config/jarvis`;
  const tokenPath = `${config}/gateway-token`;
  const meta = lstatSync(tokenPath);
  if (!meta.isFile() || (meta.mode & 0o077) !== 0 || meta.uid !== process.getuid?.()) throw new Error("gateway token must be owned by this user and mode 0600");
  const token = readFileSync(tokenPath,"utf8").trim();
  if (token.length < 32) throw new Error("gateway token is too short");
  const providers = buildProviders(process.env);
  Bun.serve({
    port,
    hostname: "127.0.0.1",
    fetch: makeFetchHandler(providers, token),
    maxRequestBodySize: 256 * 1024,
  });
  const registered = [...providers.keys()].join(", ");
  console.log(
    `jarvis gateway listening on 127.0.0.1:${port} (providers: ${registered || "none configured"})`,
  );
}
