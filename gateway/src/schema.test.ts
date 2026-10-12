import { expect, test } from "bun:test";
import { validate } from "./schema";
import { SCHEMAS } from "./contracts";

const validEvent = {
  contractVersion: "0.1",
  generationId: "g_0123456789abcdef",
  type: "content.delta",
  payload: { text: "hello" },
};

test("accepts a conformant inference event", () => {
  expect(validate(SCHEMAS.inferenceEvent, validEvent)).toEqual([]);
});

test("rejects every contract event type", () => {
  for (const type of ["started", "content.delta", "tool_use.proposal", "usage", "completed", "aborted", "error"]) {
    expect(validate(SCHEMAS.inferenceEvent, { ...validEvent, type })).toEqual([]);
  }
});

test("rejects unknown event types and extra fields", () => {
  expect(validate(SCHEMAS.inferenceEvent, { ...validEvent, type: "response.whispered" }).length).toBeGreaterThan(0);
  expect(validate(SCHEMAS.inferenceEvent, { ...validEvent, surprise: true }).length).toBeGreaterThan(0);
});

test("rejects unknown fields and out-of-range values in requests", () => {
  const base = {
    contractVersion: "0.1",
    generationId: "g_0123456789abcdef",
    deadlineMs: 1000,
    maxTurns: 4,
    maxOutputTokens: 1024,
    model: { provider: "fake", name: "plain" },
    system: [],
    messages: [{ role: "user", content: "hi", untrusted: true }],
    tools: [],
  };
  expect(validate(SCHEMAS.inferenceRequest, base)).toEqual([]);
  expect(validate(SCHEMAS.inferenceRequest, { ...base, maxTurns: 5 }).length).toBeGreaterThan(0);
  expect(validate(SCHEMAS.inferenceRequest, { ...base, deadlineMs: 60000 }).length).toBeGreaterThan(0);
  expect(validate(SCHEMAS.inferenceRequest, { ...base, nope: 1 }).length).toBeGreaterThan(0);
  expect(
    validate(SCHEMAS.inferenceRequest, {
      ...base,
      messages: [{ role: "user", content: "hi", untrusted: false }],
    }).length,
  ).toBeGreaterThan(0);
});

test("patterns and consts are enforced", () => {
  expect(validate(SCHEMAS.permissionGrant, { contractVersion: "0.2" }).length).toBeGreaterThan(0);
  expect(validate(SCHEMAS.permissionGrant, { argumentsDigest: "nothash" }).length).toBeGreaterThan(0);
  expect(
    validate(SCHEMAS.permissionGrant, {
      contractVersion: "0.1",
      grantId: "grt_0123456789abcdef",
      ownerId: "owner",
      sessionId: "s_0123456789abcdef",
      taskId: "tsk_0123456789abcdef",
      generationId: "g_0123456789abcdef",
      proposalId: "prp_0123456789abcdef",
      tool: "workspace.read_text",
      toolVersion: 1,
      argumentsDigest: "sha256:" + "a".repeat(64),
      digestCanonicalization: "RFC8785",
      resourceScope: "notes.txt",
      policyVersion: 1,
      expiresAt: "2026-10-12T00:00:00Z",
      maxUses: 1,
    }),
  ).toEqual([]);
});

test("unsupported schema keywords fail loudly rather than silently pass", () => {
  const exotic = { type: "object", properties: { a: { type: "string", format: "email" } }, unevaluatedProperties: false };
  const issues = validate(exotic, { a: "not-an-email" });
  expect(issues.some((i) => i.message.includes("unsupported schema keyword"))).toBe(true);
});
