// Model transport gateway — PR-1 skeleton.
//
// PR-3 implements the provider-neutral streaming contract from
// docs/phase-0/protocol.md (POST /v1/inference/stream). Until then the
// gateway deliberately exposes only a health probe: no inference route, no
// database route, no workspace mount — matching docs/phase-0/security.md and
// acceptance contract AC-24.

export function handleRequest(req: Request): Response {
  const url = new URL(req.url);
  if (url.pathname === "/healthz") {
    return new Response("ok\n", { headers: { "content-type": "text/plain" } });
  }
  return new Response("jarvis gateway: inference contract arrives in PR-3\n", {
    status: 501,
    headers: { "content-type": "text/plain" },
  });
}

if (import.meta.main) {
  const port = Number(process.env.JARVIS_GATEWAY_PORT ?? 8787);
  Bun.serve({
    port,
    hostname: "127.0.0.1",
    fetch: handleRequest,
  });
  console.log(`jarvis gateway skeleton listening on 127.0.0.1:${port}`);
}
