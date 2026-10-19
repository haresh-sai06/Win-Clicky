import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import test from "node:test";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
// point_parser.js is a UMD bundle; load via require-like eval
const code = readFileSync(join(root, "src/point_parser.js"), "utf8");
const module = { exports: {} };
const fn = new Function("module", "exports", code + "\n; return module.exports;");
const { parseSegments, stripPoints, validateDemo } = fn(module, module.exports);

test("parses interleaved say + point tags", () => {
  const segs = parseSegments("Look here [POINT:0.2,0.8:Submit] then done.");
  assert.equal(segs.length, 3);
  assert.equal(segs[0].type, "say");
  assert.deepEqual(
    { type: segs[1].type, x: segs[1].x, y: segs[1].y, label: segs[1].label, valid: segs[1].valid },
    { type: "point", x: 0.2, y: 0.8, label: "Submit", valid: true }
  );
  assert.equal(segs[2].text, "then done.");
});

test("rejects out-of-range coordinates as invalid", () => {
  const segs = parseSegments("[POINT:1.5,-0.1:bad]");
  assert.equal(segs[0].valid, false);
});

test("stripPoints removes tags", () => {
  assert.equal(stripPoints("Hi [POINT:0.5,0.5:mid] there"), "Hi there");
});

test("validateDemo accepts sample script", () => {
  const demo = JSON.parse(readFileSync(join(root, "demo-scripts/sample-demo.json"), "utf8"));
  assert.deepEqual(validateDemo(demo), []);
});

test("validateDemo catches bad points", () => {
  const errs = validateDemo({ steps: [{ say: "x", point: { x: 2, y: 0.5 } }] });
  assert.ok(errs.some((e) => e.includes("point")));
});
