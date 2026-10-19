/**
 * Pure POINT-tag parser shared by the panel and the offline eval harness.
 *
 * Tags look like: [POINT:0.42,0.17:Submit button]
 * Coordinates are normalized 0..1 fractions of the screenshot.
 */
(function (root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
  } else {
    root.ClickyPointParser = factory();
  }
})(typeof globalThis !== "undefined" ? globalThis : this, function () {
  const RE = /\[POINT:\s*(-?[0-9]*\.?[0-9]+)\s*,\s*(-?[0-9]*\.?[0-9]+)\s*:\s*([^\]]*)\]/g;

  function parseSegments(text) {
    const segs = [];
    let last = 0;
    let m;
    const re = new RegExp(RE.source, "g");
    while ((m = re.exec(text)) !== null) {
      if (m.index > last) {
        const say = text.slice(last, m.index).trim();
        if (say) segs.push({ type: "say", text: say });
      }
      const x = parseFloat(m[1]);
      const y = parseFloat(m[2]);
      segs.push({
        type: "point",
        x,
        y,
        label: (m[3] || "").trim(),
        valid: Number.isFinite(x) && Number.isFinite(y) && x >= 0 && x <= 1 && y >= 0 && y <= 1,
      });
      last = m.index + m[0].length;
    }
    if (last < text.length) {
      const say = text.slice(last).trim();
      if (say) segs.push({ type: "say", text: say });
    }
    return segs;
  }

  function stripPoints(text) {
    return text.replace(new RegExp(RE.source, "g"), "").replace(/\s+/g, " ").trim();
  }

  function validateDemo(demo) {
    const errors = [];
    if (!demo || typeof demo !== "object") return ["demo must be an object"];
    if (!Array.isArray(demo.steps) || demo.steps.length === 0) {
      errors.push("demo.steps must be a non-empty array");
      return errors;
    }
    demo.steps.forEach((step, i) => {
      if (!step || typeof step !== "object") {
        errors.push(`steps[${i}] must be an object`);
        return;
      }
      if (typeof step.say !== "string" || !step.say.trim()) {
        errors.push(`steps[${i}].say must be a non-empty string`);
      }
      if (step.point) {
        const { x, y } = step.point;
        if (typeof x !== "number" || typeof y !== "number" || x < 0 || x > 1 || y < 0 || y > 1) {
          errors.push(`steps[${i}].point.x/y must be numbers in [0,1]`);
        }
      }
      if (step.hold != null && (typeof step.hold !== "number" || step.hold < 0)) {
        errors.push(`steps[${i}].hold must be a non-negative number (ms)`);
      }
    });
    return errors;
  }

  return { parseSegments, stripPoints, validateDemo, RE };
});
