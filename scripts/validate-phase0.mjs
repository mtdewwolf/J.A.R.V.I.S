#!/usr/bin/env node
/**
 * validate-phase0.mjs — documentation and contract consistency check for the
 * J.A.R.V.I.S phase-0 engineering package.
 *
 * This checks that the package is internally consistent: referenced artifacts
 * exist, cross-links resolve, contract IDs match, schemas are valid JSON.
 * It does NOT test a running assistant — nothing is implemented yet.
 *
 * Usage: node scripts/validate-phase0.mjs
 * Exit code 0 = all checks passed, 1 = failures found.
 */
import { readFileSync, existsSync, readdirSync } from "node:fs";
import { join, dirname, resolve, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const errors = [];
const warnings = [];
const rel = (p) => relative(ROOT, p) || ".";

function check(name, fn) {
  try {
    fn();
  } catch (err) {
    errors.push(`${name}: ${err.message}`);
  }
}

const read = (relPath) => {
  const p = join(ROOT, relPath);
  if (!existsSync(p)) throw new Error(`missing file ${relPath}`);
  return readFileSync(p, "utf8");
};

/* 1. Required artifacts ------------------------------------------------ */
const REQUIRED = [
  "README.md",
  "docs/phase-0/README.md",
  "docs/phase-0/capabilities.md",
  "docs/phase-0/architecture.md",
  "docs/phase-0/technology.md",
  "docs/phase-0/security.md",
  "docs/phase-0/environment.md",
  "docs/phase-0/roadmap.md",
  "docs/phase-0/repository.md",
  "docs/phase-0/pull-requests.md",
  "docs/phase-0/acceptance.md",
  "docs/phase-0/protocol.md",
  "docs/phase-0/sources.md",
  "docs/adr/README.md",
  "contracts/README.md",
  "tests/acceptance/initial-slice.feature",
  "scripts/validate-phase0.mjs",
];
check("required artifacts", () => {
  const missing = REQUIRED.filter((f) => !existsSync(join(ROOT, f)));
  if (missing.length) throw new Error(`${missing.join(", ")}`);
});

/* 2. Markdown relative links resolve ------------------------------------ */
function listMarkdown(dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === ".git") continue;
    const p = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...listMarkdown(p));
    else if (entry.name.endsWith(".md")) out.push(p);
  }
  return out;
}
check("markdown links", () => {
  const mdFiles = listMarkdown(ROOT);
  const linkRe = /\]\(([^)\s]+)\)/g;
  const broken = [];
  for (const file of mdFiles) {
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(linkRe)) {
      const target = m[1];
      if (/^(https?:|mailto:|#)/.test(target)) continue;
      const [pathPart] = target.split("#");
      if (!pathPart) continue;
      if (!existsSync(resolve(dirname(file), pathPart))) {
        broken.push(`${rel(file)} -> ${target}`);
      }
    }
  }
  if (broken.length) throw new Error(`${broken.length} broken link(s):\n  ${broken.join("\n  ")}`);
});

/* 3. Acceptance contract IDs -------------------------------------------- */
const AC_TOTAL = 29;
const acceptanceText = read("docs/phase-0/acceptance.md");
const definedACs = new Set(
  [...acceptanceText.matchAll(/^### (AC-\d{2}) —/gm)].map((m) => m[1]),
);
const expectedACs = Array.from({ length: AC_TOTAL }, (_, i) => `AC-${String(i + 1).padStart(2, "0")}`);
check("acceptance contract definitions", () => {
  const missing = expectedACs.filter((id) => !definedACs.has(id));
  if (missing.length) throw new Error(`contracts not defined: ${missing.join(", ")}`);
  const extra = [...definedACs].filter((id) => !expectedACs.includes(id));
  if (extra.length) throw new Error(`unknown contracts: ${extra.join(", ")}`);
});
const referencedACs = (text) => new Set([...text.matchAll(/\bAC-\d{2}\b/g)].map((m) => m[0]));
check("AC references resolve", () => {
  const bad = [];
  for (const f of ["docs/phase-0/capabilities.md", "docs/phase-0/security.md", "docs/phase-0/pull-requests.md", "docs/phase-0/roadmap.md"]) {
    for (const id of referencedACs(read(f))) {
      if (!definedACs.has(id)) bad.push(`${f} -> ${id}`);
    }
  }
  if (bad.length) throw new Error(`unknown references:\n  ${bad.join("\n  ")}`);
});
check("no orphan contracts", () => {
  const referenced = new Set();
  for (const f of ["docs/phase-0/capabilities.md", "docs/phase-0/security.md", "docs/phase-0/pull-requests.md", "tests/acceptance/initial-slice.feature"]) {
    for (const id of referencedACs(read(f))) referenced.add(id);
  }
  const orphans = expectedACs.filter((id) => !referenced.has(id));
  if (orphans.length) throw new Error(`contracts referenced by nothing: ${orphans.join(", ")}`);
});

/* 4. Capability IDs ------------------------------------------------------ */
check("capability references resolve", () => {
  const caps = new Set(
    [...read("docs/phase-0/capabilities.md").matchAll(/^\|\s*((?:CAP|CORE|MEM|VOICE|AGENT|CTRL|ENG|PRO|UI|OPS)-\d+)\s*\|/gm)].map((m) => m[1]),
  );
  if (!caps.has("CAP-01")) throw new Error("capability matrix unreadable");
  const bad = [];
  for (const f of ["docs/phase-0/acceptance.md"]) {
    for (const id of [...read(f).matchAll(/\b((?:CAP|CORE|MEM|VOICE|AGENT|CTRL|ENG|PRO|UI|OPS)-\d+)\b/g)].map((m) => m[1])) {
      if (!caps.has(id)) bad.push(`${f} -> ${id}`);
    }
  }
  if (bad.length) throw new Error(`unknown capability references:\n  ${bad.join("\n  ")}`);
});

/* 5. Gherkin feature tags ------------------------------------------------ */
check("gherkin feature", () => {
  const feature = read("tests/acceptance/initial-slice.feature");
  // Each scenario is preceded by exactly one @AC-xx tag line.
  const re = /@(AC-\d{2})\s*\n\s*Scenario:\s+(.+)/g;
  const matches = [...feature.matchAll(re)];
  if (matches.length < 20) throw new Error(`expected at least 20 scenarios, found ${matches.length}`);
  const bad = [];
  const sliceIds = new Set();
  matches.forEach((m, i) => {
    const [id, title] = [m[1], m[2].trim()];
    sliceIds.add(id);
    if (!definedACs.has(id)) bad.push(`"${title}" tags unknown ${id}`);
    const body = feature.slice(m.index + m[0].length, i + 1 < matches.length ? matches[i + 1].index : undefined);
    if (!/\bWhen\b/.test(body)) bad.push(`"${title}" has no When`);
    if (!/\bThen\b/.test(body)) bad.push(`"${title}" has no Then`);
  });
  if (bad.length) throw new Error(`feature file:\n  ${bad.join("\n  ")}`);
  const uncovered = expectedACs.filter((id) => !sliceIds.has(id));
  if (uncovered.length) {
    warnings.push(`contracts without a slice scenario (must be deferred rows): ${uncovered.join(", ")}`);
  }
});
function bodyeatureHolder(s) { return s; }

/* 6. PR coverage matrix --------------------------------------------------- */
check("PR coverage matrix", () => {
  const prs = read("docs/phase-0/pull-requests.md");
  const missing = expectedACs.filter((id) => !prs.includes(`| ${id} |`));
  if (missing.length) throw new Error(`coverage matrix rows missing for: ${missing.join(", ")}`);
  for (let n = 1; n <= 5; n++) {
    if (!new RegExp(`## PR-${n} `).test(prs)) throw new Error(`## PR-${n} specification missing`);
  }
});

/* 7. Contract schemas ------------------------------------------------------ */
check("contract schemas", () => {
  const dir = join(ROOT, "contracts/schemas");
  const files = readdirSync(dir).filter((f) => f.endsWith(".json"));
  if (files.length < 6) throw new Error(`expected >= 6 schemas, found ${files.length}`);
  const bad = [];
  for (const f of files) {
    let doc;
    try {
      doc = JSON.parse(readFileSync(join(dir, f), "utf8"));
    } catch (e) {
      bad.push(`${f}: invalid JSON (${e.message})`);
      continue;
    }
    for (const key of ["$schema", "$id", "title", "type"]) {
      if (!doc[key]) bad.push(`${f}: missing ${key}`);
    }
    if (doc.additionalProperties !== false) bad.push(`${f}: must set additionalProperties: false`);
  }
  if (bad.length) throw new Error(`schema problems:\n  ${bad.join("\n  ")}`);
});

/* 8. Roadmap phases -------------------------------------------------------- */
check("roadmap phases", () => {
  const roadmap = read("docs/phase-0/roadmap.md");
  const phases = new Set([...roadmap.matchAll(/^## Phase (\d)/gm)].map((m) => m[1]));
  const expected = ["0", "1", "2", "3", "4", "5", "6", "7", "8"];
  const missing = expected.filter((p) => !phases.has(p));
  if (missing.length) throw new Error(`roadmap missing phases: ${missing.join(", ")}`);
});

/* 9. ADR index ------------------------------------------------------------- */
check("ADR index", () => {
  const index = read("docs/adr/README.md");
  const adrs = readdirSync(join(ROOT, "docs/adr")).filter((f) => /^\d{4}-.*\.md$/.test(f));
  if (adrs.length < 7) throw new Error(`expected >= 7 ADRs, found ${adrs.length}`);
  const bad = [];
  for (const f of adrs) {
    if (!index.includes(`](${f})`)) bad.push(`${f} not listed in docs/adr/README.md`);
    if (!/^- Status: Proposed/m.test(read(`docs/adr/${f}`))) bad.push(`${f} missing Status`);
  }
  if (bad.length) throw new Error(bad.join("; "));
});

/* 10. Balanced code fences -------------------------------------------------- */
check("code fences", () => {
  const bad = [];
  for (const file of listMarkdown(ROOT)) {
    const fences = readFileSync(file, "utf8").match(/^```/gm) || [];
    if (fences.length % 2 !== 0) bad.push(rel(file));
  }
  if (bad.length) throw new Error(`unbalanced code fences in ${bad.join(", ")}`);
});

/* Report --------------------------------------------------------------------- */
for (const w of warnings) console.warn(`warning: ${w}`);
if (errors.length) {
  console.error(`FAIL: ${errors.length} check(s) failed\n`);
  for (const e of errors) console.error(`- ${e}\n`);
  process.exit(1);
}
console.log(
  `PASS: phase-0 package is internally consistent\n` +
    `  artifacts:   ${REQUIRED.length} required files present\n` +
    `  contracts:   ${expectedACs.length} acceptance contracts defined and referenced\n` +
    `  features:    initial-slice scenarios tagged and tagged IDs defined\n` +
    `  schemas:     contracts/schemas valid JSON with closed objects\n` +
    `  note:        documentation check only; no assistant runtime is implemented`,
);
