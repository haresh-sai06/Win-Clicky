#!/usr/bin/env node
/** Offline POINT-tag parser eval + demo-script validation. */
import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const code = readFileSync(join(root, "src/point_parser.js"), "utf8");
const module = { exports: {} };
const { parseSegments, validateDemo } = new Function("module", "exports", code + "\n; return module.exports;")(module, module.exports);

const fixtures = JSON.parse(readFileSync(join(root, "eval/fixtures/golden_replies.json"), "utf8"));
let tp = 0, fp = 0, fn = 0, invalidOk = 0, invalidTotal = 0;

function near(a, b, eps = 1e-6) {
  return Math.abs(a - b) <= eps;
}

for (const row of fixtures) {
  const segs = parseSegments(row.reply);
  const points = segs.filter((s) => s.type === "point");
  const valid = points.filter((s) => s.valid);
  const invalid = points.filter((s) => !s.valid);
  if (row.expect_invalid) {
    invalidTotal += 1;
    if (invalid.length >= row.expect_invalid) invalidOk += 1;
  }
  const gold = row.gold_points || [];
  // greedy match by order
  let gi = 0;
  for (const p of valid) {
    if (gi < gold.length && near(p.x, gold[gi].x) && near(p.y, gold[gi].y) && p.label === gold[gi].label) {
      tp += 1; gi += 1;
    } else {
      fp += 1;
    }
  }
  fn += gold.length - gi;
}

const precision = tp / (tp + fp) || 0;
const recall = tp / (tp + fn) || 0;
const f1 = precision + recall ? (2 * precision * recall) / (precision + recall) : 0;

const demos = ["sample-demo.json", "fixed-coordinates-demo.json"].map((name) => {
  const demo = JSON.parse(readFileSync(join(root, "demo-scripts", name), "utf8"));
  const errors = validateDemo(demo);
  return { name, ok: errors.length === 0, errors, steps: demo.steps.length };
});

const summary = {
  point_tag_extraction: {
    tp, fp, fn,
    precision: +precision.toFixed(4),
    recall: +recall.toFixed(4),
    f1: +f1.toFixed(4),
    n_fixtures: fixtures.length,
  },
  invalid_coordinate_detection: {
    detected: invalidOk,
    cases: invalidTotal,
    accuracy: invalidTotal ? +(invalidOk / invalidTotal).toFixed(4) : null,
  },
  demo_script_validation: demos,
};

const outDir = join(root, "eval/results");
mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, "summary.json"), JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary, null, 2));
