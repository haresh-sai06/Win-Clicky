// Overlay window: draws the flying pointer and subtitle captions.
// It owns no logic of its own -- the panel drives it entirely via events.
//
// Coordinates arrive normalized (0..1) so they're independent of resolution
// and Windows display scaling: we just multiply by this window's size.

const { listen } = window.__TAURI__.event;

const pointer = document.getElementById("pointer");
const ptrLabel = document.getElementById("ptr-label");
const caption = document.getElementById("caption");

function placePointer(fx, fy, label) {
  const x = Math.max(0, Math.min(1, fx)) * window.innerWidth;
  const y = Math.max(0, Math.min(1, fy)) * window.innerHeight;
  pointer.style.left = `${x}px`;
  pointer.style.top = `${y}px`;
  if (label) {
    ptrLabel.textContent = label;
    ptrLabel.style.display = "block";
    // Flip the label to the left side if we're near the right edge.
    ptrLabel.style.left = fx > 0.8 ? "auto" : "26px";
    ptrLabel.style.right = fx > 0.8 ? "26px" : "auto";
  } else {
    ptrLabel.style.display = "none";
  }
  pointer.classList.remove("hidden");
}

listen("point", (e) => {
  const p = e.payload || {};
  placePointer(p.x, p.y, p.label);
});

listen("clear-pointer", () => {
  pointer.classList.add("hidden");
});

listen("caption", (e) => {
  const text = (e.payload && e.payload.text) || "";
  if (text) {
    caption.textContent = text;
    caption.classList.remove("hidden");
  } else {
    caption.classList.add("hidden");
  }
});
