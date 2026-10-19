#!/usr/bin/env node
// Catalog parity gate: fails when the en and zh-CN catalogs drift apart or
// when a zh-CN entry is blank while its English source is not. CI runs this
// through `npm run i18n:check` in apps/desktop.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const catalogDir = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "apps",
  "desktop",
  "src",
  "i18n",
);
const en = JSON.parse(readFileSync(join(catalogDir, "en.json"), "utf8"));
const zh = JSON.parse(readFileSync(join(catalogDir, "zh-CN.json"), "utf8"));

const enKeys = Object.keys(en);
const zhKeys = Object.keys(zh);
const missingInZh = enKeys.filter((key) => !(key in zh));
const unexpectedInZh = zhKeys.filter((key) => !(key in en));
const blankInZh = zhKeys.filter(
  (key) => key in en && String(zh[key]).trim() === "" && String(en[key]).trim() !== "",
);

if (missingInZh.length + unexpectedInZh.length + blankInZh.length > 0) {
  console.error("Catalog parity check failed:");
  if (missingInZh.length) console.error(`  missing in zh-CN: ${missingInZh.join(", ")}`);
  if (unexpectedInZh.length) console.error(`  unexpected in zh-CN: ${unexpectedInZh.join(", ")}`);
  if (blankInZh.length) console.error(`  blank in zh-CN: ${blankInZh.join(", ")}`);
  process.exit(1);
}
console.log(`Catalog parity OK: ${enKeys.length} keys present in both catalogs`);
