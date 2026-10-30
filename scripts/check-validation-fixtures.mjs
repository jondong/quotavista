#!/usr/bin/env node
// Validation-fixture gate: proves the sanitized capability corpus under
// docs/validation/fixtures is well-formed, honestly labelled, and free of
// secret-like content, and that the two capability reports still describe it.
// Run it with `node scripts/check-validation-fixtures.mjs`. It is
// dependency-free and exits non-zero when any check fails.
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const fixturesDir = join(repoRoot, "docs", "validation", "fixtures");
const reports = [
  join(repoRoot, "docs", "validation", "provider-capability-openrouter.md"),
  join(repoRoot, "docs", "validation", "provider-capability-kimi.md"),
];
const maxBytes = 64 * 1024;
const redacted = "<redacted-at-capture>";
const readOnPattern = /^\d{4}-\d{2}-\d{2}$/;
const ulidPattern = /^[0-9A-Z]{26}$/;

const requiredMetaKeys = [
  "authorization",
  "captured_from_live_account",
  "fixture",
  "notes",
  "origin",
  "provider",
  "read_on",
  "report",
  "sanitized",
  "source_url",
  "surface",
  "validated",
];

// Key-prefix rules demand a plausible-length body so that prose merely naming a
// prefix, or a masked label such as the provider's own truncated example, does
// not trip the gate.
const secretPatterns = [
  {
    name: "authorization-header-credential",
    pattern:
      /["']?(?:proxy-)?authorization["']?\s*[:=]\s*["']?(?:bearer|basic)\s+[A-Za-z0-9._~+/=-]{16,}/i,
  },
  { name: "openrouter-key", pattern: /sk-or-v1-[A-Za-z0-9_-]{20,}/i },
  { name: "anthropic-key", pattern: /sk-ant-[A-Za-z0-9_-]{20,}/i },
  { name: "openai-project-key", pattern: /sk-proj-[A-Za-z0-9_-]{20,}/i },
  { name: "generic-sk-run", pattern: /\bsk-[A-Za-z0-9_-]{20,}/ },
  {
    name: "cookie-value",
    pattern: /["']?(?:set-)?cookie["']?\s*[:=]\s*["']?[A-Za-z0-9._~+/=%-]{16,}/i,
  },
  { name: "pem-private-key", pattern: /-----BEGIN [A-Z ]*PRIVATE KEY-----/ },
  {
    name: "json-web-token",
    pattern: /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}/,
  },
  {
    name: "secret-named-field",
    pattern:
      /["'](?:password|passwd|api_key|apikey|secret|access_token|refresh_token|client_secret|private_key|session_token)["']\s*:/i,
  },
];

const failures = [];
function fail(check, message) {
  failures.push(`${check}: ${message}`);
}

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir).sort()) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else out.push(full);
  }
  return out;
}

// Fields whose presence in the Kimi userinfo shape would be a leak by
// construction; the fixture must omit them rather than redact them.
const forbiddenUserinfoFields = [
  "avatar",
  "bio",
  "globalId",
  "username",
  "createdTime",
  "lastLoginTime",
];

function collectValuesForKey(value, name, found) {
  if (Array.isArray(value)) {
    for (const item of value) collectValuesForKey(item, name, found);
  } else if (value && typeof value === "object") {
    for (const [key, item] of Object.entries(value)) {
      if (key === name) found.push(item);
      collectValuesForKey(item, name, found);
    }
  }
  return found;
}

if (!existsSync(fixturesDir)) {
  fail("layout", `missing corpus directory ${relative(repoRoot, fixturesDir)}`);
}
for (const report of reports) {
  if (!existsSync(report)) {
    fail("layout", `missing capability report ${relative(repoRoot, report)}`);
  }
}

const files = existsSync(fixturesDir) ? walk(fixturesDir) : [];
const contents = new Map();
const parsed = new Map();

// 1. Encoding, size, and JSON syntax.
for (const file of files) {
  const name = relative(repoRoot, file);
  const bytes = readFileSync(file);
  if (bytes.length > maxBytes) {
    fail("encoding", `${name} is ${bytes.length} bytes, over the ${maxBytes}-byte cap`);
  }
  let text;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    fail("encoding", `${name} is not valid UTF-8`);
    continue;
  }
  contents.set(file, text);
  if (file.endsWith(".json")) {
    try {
      parsed.set(file, JSON.parse(text));
    } catch (error) {
      fail("json", `${name} does not parse: ${error.message}`);
    }
  }
}

const isMeta = (file) => file.endsWith(".meta.json");
// A fixture's provenance sibling drops the fixture's own extension, so
// `activity_ok.json` pairs with `activity_ok.meta.json` and the Markdown page
// description pairs the same way.
const metaNameFor = (file) => `${file.replace(/\.(?:json|md)$/, "")}.meta.json`;
const fixtures = files.filter((file) => !isMeta(file) && file !== join(fixturesDir, "README.md"));
const metas = files.filter(isMeta);

// 2. Fixture and provenance pair up in both directions.
for (const file of fixtures) {
  const expected = metaNameFor(file);
  if (!metas.includes(expected)) {
    fail("pairing", `${relative(repoRoot, file)} has no sibling ${relative(repoRoot, expected)}`);
  }
}
for (const meta of metas) {
  const stem = meta.slice(0, -".meta.json".length);
  if (!existsSync(`${stem}.json`) && !existsSync(`${stem}.md`)) {
    fail("pairing", `${relative(repoRoot, meta)} has no sibling fixture`);
  }
}

// 3. Provenance is complete and honest.
for (const meta of metas) {
  const name = relative(repoRoot, meta);
  const value = parsed.get(meta);
  if (value === undefined) continue;
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    fail("provenance", `${name} is not a JSON object`);
    continue;
  }
  for (const key of requiredMetaKeys) {
    if (!(key in value)) fail("provenance", `${name} is missing the required key ${key}`);
  }
  if (value.sanitized !== true) {
    fail("provenance", `${name} must declare sanitized true`);
  }
  if (value.validated !== false) {
    fail("provenance", `${name} must declare validated false; nothing in this corpus has been compared with a live response`);
  }
  if (value.captured_from_live_account !== false) {
    fail("provenance", `${name} must declare captured_from_live_account false; a live capture is not allowed in this corpus`);
  }
  if (!["kimi", "openrouter"].includes(value.provider)) {
    fail("provenance", `${name} names an unknown provider ${JSON.stringify(value.provider)}`);
  }
  if (typeof value.read_on !== "string" || !readOnPattern.test(value.read_on)) {
    fail("provenance", `${name} read_on must be a YYYY-MM-DD date`);
  }
  if (typeof value.source_url !== "string" || !value.source_url.startsWith("https://")) {
    fail("provenance", `${name} source_url must be an https URL`);
  }
  for (const key of ["authorization", "notes", "origin", "surface"]) {
    if (typeof value[key] !== "string" || value[key].trim() === "") {
      fail("provenance", `${name} ${key} must be a non-empty string`);
    }
  }
  const stem = meta.slice(0, -".meta.json".length);
  const sibling = existsSync(`${stem}.json`) ? `${stem}.json` : `${stem}.md`;
  if (value.fixture !== relative(dirname(sibling), sibling)) {
    fail("provenance", `${name} fixture must name its sibling ${relative(dirname(sibling), sibling)}`);
  }
  const reportPath = join(repoRoot, String(value.report ?? ""));
  if (!reports.includes(reportPath)) {
    fail("provenance", `${name} report must be one of the two capability reports, got ${JSON.stringify(value.report)}`);
  } else if (!existsSync(reportPath)) {
    fail("provenance", `${name} report ${value.report} does not exist`);
  }
  if (String(value.provider) !== relative(fixturesDir, dirname(meta))) {
    fail("provenance", `${name} sits under ${relative(fixturesDir, dirname(meta))}${sep} but declares provider ${value.provider}`);
  }
}

// 4. Bodies stay wire-shaped: no comment key smuggled into the payload.
for (const file of fixtures) {
  if (!file.endsWith(".json")) continue;
  const value = parsed.get(file);
  if (typeof value !== "object" || value === null || Array.isArray(value)) continue;
  for (const key of Object.keys(value)) {
    if (key.startsWith("_")) {
      fail("wire-shape", `${relative(repoRoot, file)} carries the comment key ${key}; provenance belongs in the sibling meta file`);
    }
  }
}

// 5. Secret-like patterns across the corpus and both capability reports.
for (const file of [...files, ...reports]) {
  const text = contents.get(file) ?? (existsSync(file) ? readFileSync(file, "utf8") : "");
  for (const { name, pattern } of secretPatterns) {
    const match = text.match(pattern);
    if (match) {
      fail("secrets", `${relative(repoRoot, file)} matches the ${name} pattern near ${JSON.stringify(match[0].slice(0, 40))}`);
    }
  }
}

// 6. The Kimi userinfo fixture redacts by construction.
const userinfo = join(fixturesDir, "kimi", "oauth_userinfo_shape_only.json");
if (parsed.has(userinfo)) {
  const info = parsed.get(userinfo)?.data?.userInfo;
  if (typeof info !== "object" || info === null) {
    fail("pii", `${relative(repoRoot, userinfo)} has no data.userInfo object`);
  } else {
    const mustBeSentinel = ["nickname", "userLevelName", "domain", "domainName", "email"];
    for (const key of mustBeSentinel) {
      if (info[key] !== redacted) {
        fail("pii", `${relative(repoRoot, userinfo)} userInfo.${key} must be exactly ${redacted}`);
      }
    }
    for (const key of ["countryCode", "number"]) {
      if (info.phone?.[key] !== redacted) {
        fail("pii", `${relative(repoRoot, userinfo)} userInfo.phone.${key} must be exactly ${redacted}`);
      }
    }
    for (const key of forbiddenUserinfoFields) {
      if (key in info) {
        fail("pii", `${relative(repoRoot, userinfo)} must omit userInfo.${key} entirely`);
      }
    }
  }
} else if (existsSync(userinfo)) {
  fail("pii", `${relative(repoRoot, userinfo)} did not parse, so its redaction could not be checked`);
}

// 7. Invented request identifiers keep their ULID shape.
for (const file of fixtures) {
  const value = parsed.get(file);
  if (value === undefined) continue;
  for (const id of collectValuesForKey(value, "request_id", [])) {
    if (typeof id !== "string" || !ulidPattern.test(id)) {
      fail("request-id", `${relative(repoRoot, file)} request_id ${JSON.stringify(id)} is not a 26-character ULID-shaped string`);
    }
  }
}

// 8. Every corpus path linked from a report resolves.
const reportTexts = new Map();
for (const report of reports) {
  const text = existsSync(report) ? readFileSync(report, "utf8") : "";
  reportTexts.set(report, text);
  for (const match of text.matchAll(/\]\(([^)\s]*fixtures\/[^)\s]+)\)/g)) {
    const target = join(dirname(report), decodeURIComponent(match[1]));
    if (!existsSync(target)) {
      fail("report-links", `${relative(repoRoot, report)} links to ${match[1]}, which does not exist`);
    }
  }
}

// 9. Every fixture is described by at least one report.
// Reports cite fixtures by bare filename, so basenames must stay unique for a
// citation match to be unambiguous.
const byBasename = new Map();
for (const file of fixtures) {
  const base = file.slice(file.lastIndexOf(sep) + 1);
  if (byBasename.has(base)) {
    fail(
      "coverage",
      `${base} is shared by ${relative(fixturesDir, byBasename.get(base))} and ${relative(fixturesDir, file)}; a report citation would be ambiguous`,
    );
  }
  byBasename.set(base, file);
}
for (const [base, file] of byBasename) {
  const cited = [...reportTexts.values()].some((text) => text.includes(base));
  if (!cited) {
    fail("coverage", `docs/validation/fixtures/${relative(fixturesDir, file).split(sep).join("/")} is not cited by either capability report`);
  }
}

if (failures.length > 0) {
  console.error(`validation fixtures: ${failures.length} problem(s)`);
  for (const line of failures) console.error(`  - ${line}`);
  process.exit(1);
}

const jsonFixtures = fixtures.filter((file) => file.endsWith(".json")).length;
const otherFixtures = fixtures.length - jsonFixtures;
console.log(
  `validation fixtures ok: ${jsonFixtures} JSON bodies, ${otherFixtures} other fixture(s), ` +
    `${metas.length} provenance files, ${secretPatterns.length} secret patterns clean, ` +
    `${reportTexts.size} reports cross-checked`,
);
