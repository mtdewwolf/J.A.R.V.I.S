import { expect, test } from "bun:test";
import { handleRequest } from "./server";

test("health probe returns ok", async () => {
  const res = handleRequest(new Request("http://127.0.0.1/healthz"));
  expect(res.status).toBe(200);
  expect(await res.text()).toContain("ok");
});

test("no inference route exists before PR-3 (AC-24)", async () => {
  const res = handleRequest(new Request("http://127.0.0.1/v1/inference/stream"));
  expect(res.status).toBe(501);
});
