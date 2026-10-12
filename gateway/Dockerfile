# Gateway container: untrusted proposal transport only (AC-24).
#
# Build from the repository root:
#   docker build -f gateway/Dockerfile -t jarvis-gateway .
#
# Security posture (docs/phase-0/security.md):
#   * nonroot user; the image is compatible with readOnlyRootFilesystem and
#     no-new-privileges at runtime (see deploy notes);
#   * NO database credential, NO workspace mount, NO execution route — the
#     dependency tree has no database client at all;
#   * loopback by default; exposure happens only behind the authenticated core.
#
# Build context is the repository root so contracts/schemas (imported at
# runtime for validation) and the workspace layout stay identical to dev.

FROM oven/bun:1.4.3-alpine

WORKDIR /usr/src/gateway
COPY gateway/package.json ./gateway/package.json
COPY gateway/src ./gateway/src
COPY contracts ./contracts

USER bun
ENV NODE_ENV=production
ENV JARVIS_GATEWAY_PORT=8787
EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD bun -e "const r = await fetch('http://127.0.0.1:8787/healthz'); process.exit(r.ok ? 0 : 1)"

CMD ["bun", "run", "gateway/src/server.ts"]
