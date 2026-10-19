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
const blankInEn = enKeys.filter((key) => String(en[key]).trim() === "");

const placeholders = (value) =>
  new Set([...String(value).matchAll(/\{(\w+)\}/g)].map((match) => match[1]));
const placeholderDrift = enKeys.filter(
  (key) => key in zh && !eqSets(placeholders(en[key]), placeholders(zh[key])),
);
function eqSets(a, b) {
  if (a.size !== b.size) return false;
  for (const item of a) if (!b.has(item)) return false;
  return true;
}

const failed =
  missingInZh.length + unexpectedInZh.length + blankInZh.length + blankInEn.length +
  placeholderDrift.length > 0;
if (failed) {
  console.error("Catalog parity check failed:");
  if (missingInZh.length) console.error(`  missing in zh-CN: ${missingInZh.join(", ")}`);
  if (unexpectedInZh.length) console.error(`  unexpected in zh-CN: ${unexpectedInZh.join(", ")}`);
  if (blankInZh.length) console.error(`  blank in zh-CN: ${blankInZh.join(", ")}`);
  if (blankInEn.length) console.error(`  blank in en: ${blankInEn.join(", ")}`);
  if (placeholderDrift.length)
    console.error(
      `  placeholder drift in zh-CN: ${placeholderDrift
        .map((key) => `${key} (en {${[...placeholders(en[key])].join(",")}} vs zh {${[...placeholders(zh[key])].join(",")}})`)
        .join("; ")}`,
    );
  process.exit(1);
}
console.log(`Catalog parity OK: ${enKeys.length} keys, placeholders matching in both catalogs`);
