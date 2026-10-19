// Clicky panel: the brain of the UI.
//   Ctrl+Alt+Space / Ask  -> screenshot -> LLM -> speak + point
//   Auto-demo             -> screenshot -> LLM writes a demo script -> run it
//   Run script            -> run the JSON demo script in the editor
//   🎙️ Mic button         -> Web Speech API speech-to-text -> ask
//
// All cross-window drawing is done by emitting events the overlay listens for.

const { invoke } = window.__TAURI__.core;
const { emit, listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;
// In Tauri 2, LogicalSize lives in the `dpi` namespace (older builds: `window`).
const LogicalSize =
  (window.__TAURI__.dpi && window.__TAURI__.dpi.LogicalSize) ||
  (window.__TAURI__.window && window.__TAURI__.window.LogicalSize);

// ---------- state ----------
let cfg = null;
let history = [];
let stopFlag = false;
let busy = false;
let collapsed = false;
let recognizing = false;

// ---------- element refs ----------
const $ = (id) => document.getElementById(id);
const statusEl = $("status");
const responseEl = $("response");
const askInput = $("ask-input");
const demoJson = $("demo-json");
const btnMic = $("btn-mic");
const fileInput = $("file-input");

const PRESETS = {
  openai: { base: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  groq: { base: "https://api.groq.com/openai/v1", model: "llama-3.2-90b-vision-preview" },
  openrouter: { base: "https://openrouter.ai/api/v1", model: "openai/gpt-4o-mini" },
  ollama: { base: "http://localhost:11434/v1", model: "llava" },
};

const SAMPLE_DEMO = {
  title: "Demo of my project",
  steps: [
    { say: "Hey judges! Let me show you what we built.", point: { x: 0.5, y: 0.08, label: "Our project" }, hold: 1200 },
    { say: "This is the main area where everything happens.", find: "the main content area", hold: 1000 },
    { say: "And here's the button that kicks off the magic.", find: "the primary action button", hold: 1000 },
    { say: "Thanks so much for watching our demo!", hold: 600 },
  ],
};

// ---------- helpers ----------
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function setStatus(text) {
  statusEl.textContent = text;
}

function showResponse(text) {
  responseEl.textContent = text;
  responseEl.classList.add("show");
}

function setBusy(on) {
  busy = on;
  $("btn-ask").disabled = on;
  $("btn-autodemo").disabled = on;
  $("btn-rundemo").disabled = on;
  btnMic.disabled = on;
}

async function speak(text) {
  if (!text || !text.trim()) return;
  try {
    await invoke("speak", {
      text,
      rate: cfg ? cfg.tts_rate : 1,
      voice: cfg ? cfg.tts_voice : "",
    });
  } catch (e) {
    console.error("speak failed:", e);
  }
}

// Pull a JSON object/array out of a model reply, tolerating ```json fences.
function parseJsonLoose(text) {
  if (!text) return null;
  let t = text.trim().replace(/^```(json)?/i, "").replace(/```$/, "").trim();
  try {
    return JSON.parse(t);
  } catch (_) {}
  const start = t.search(/[{[]/);
  const end = Math.max(t.lastIndexOf("}"), t.lastIndexOf("]"));
  if (start !== -1 && end > start) {
    try {
      return JSON.parse(t.slice(start, end + 1));
    } catch (_) {}
  }
  return null;
}

// Split a reply into ordered "say" and "point" segments using [POINT:x,y:label].
// Uses the shared parser from point_parser.js (loaded before this file in panel.html).
function parseSegments(text) {
  if (globalThis.ClickyPointParser) {
    return globalThis.ClickyPointParser.parseSegments(text).map((s) =>
      s.type === "point" ? { type: "point", x: s.x, y: s.y, label: s.label } : s
    );
  }
  // Inline fallback (should not run if panel.html is up to date).
  const re = /\[POINT:\s*([0-9]*\.?[0-9]+)\s*,\s*([0-9]*\.?[0-9]+)\s*:\s*([^\]]*)\]/g;
  const segs = [];
  let last = 0;
  let m;
  while ((m = re.exec(text))) {
    const before = text.slice(last, m.index).trim();
    if (before) segs.push({ type: "say", text: before });
    segs.push({ type: "point", x: parseFloat(m[1]), y: parseFloat(m[2]), label: m[3].trim() });
    last = re.lastIndex;
  }
  const tail = text.slice(last).trim();
  if (tail) segs.push({ type: "say", text: tail });
  return segs;
}

// Speak the reply, flying the pointer to each [POINT] in order.
async function narrate(text) {
  const segs = parseSegments(text);
  // The clean (spoken) text, with tags stripped, for the response box.
  showResponse(text.replace(/\[POINT:[^\]]*\]/g, "").trim());
  for (const s of segs) {
    if (stopFlag) break;
    if (s.type === "point") {
      emit("point", { x: s.x, y: s.y, label: s.label });
      await sleep(450);
    } else {
      emit("caption", { text: s.text });
      await speak(s.text);
    }
  }
  emit("caption", { text: "" });
  setTimeout(() => emit("clear-pointer"), 1600);
}

// ---------- Web Speech API (speech-to-text) ----------
let recognition = null;

function initSpeechRecognition() {
  const SpeechRecognition = window.SpeechRecognition || window.webkitSpeechRecognition;
  if (!SpeechRecognition) {
    console.warn("Web Speech API not available in this WebView.");
    btnMic.title = "Speech recognition not available in this WebView";
    btnMic.style.opacity = "0.4";
    btnMic.style.cursor = "not-allowed";
    return;
  }

  recognition = new SpeechRecognition();
  recognition.continuous = false;
  recognition.interimResults = true;
  recognition.lang = "en-US";

  recognition.onstart = () => {
    recognizing = true;
    btnMic.textContent = "🔴";
    btnMic.style.boxShadow = "0 0 12px rgba(255, 80, 80, 0.5)";
    setStatus("Listening…");
  };

  recognition.onresult = (event) => {
    let interim = "";
    let final_ = "";
    for (let i = 0; i < event.results.length; i++) {
      if (event.results[i].isFinal) {
        final_ += event.results[i][0].transcript;
      } else {
        interim += event.results[i][0].transcript;
      }
    }
    askInput.value = final_ || interim;
    if (final_) {
      setStatus("Got it!");
    }
  };

  recognition.onerror = (event) => {
    console.error("Speech recognition error:", event.error);
    recognizing = false;
    btnMic.textContent = "🎙️";
    btnMic.style.boxShadow = "";
    if (event.error === "not-allowed") {
      setStatus("Microphone access denied.");
    } else {
      setStatus("Speech error: " + event.error);
    }
  };

  recognition.onend = () => {
    recognizing = false;
    btnMic.textContent = "🎙️";
    btnMic.style.boxShadow = "";
    // If we got text, automatically trigger ask
    if (askInput.value.trim()) {
      setStatus("Processing…");
      ask();
    } else {
      setStatus("Ready — Ctrl+Alt+Space to talk");
    }
  };
}

function toggleMic() {
  if (!recognition) return;
  if (recognizing) {
    recognition.stop();
  } else {
    askInput.value = "";
    recognition.start();
  }
}

// ---------- main flows ----------
async function captureScreen() {
  return invoke("capture_screen"); // { data_url, width, height }
}

async function ask() {
  const q = askInput.value.trim();
  if (!q || busy) return;
  setBusy(true);
  setStatus("Looking…");
  try {
    let image = null;
    if (cfg && cfg.vision) {
      const shot = await captureScreen();
      image = shot.data_url;
    }
    setStatus("Thinking…");
    const reply = await invoke("ask_llm", { prompt: q, imageDataUrl: image, history });
    history.push({ role: "user", content: q });
    history.push({ role: "assistant", content: reply.replace(/\[POINT:[^\]]*\]/g, "").trim() });
    if (history.length > 12) history = history.slice(-12);
    setStatus("Speaking…");
    await narrate(reply);
    setStatus("Ready.");
  } catch (e) {
    setStatus("Error.");
    showResponse(String(e));
  } finally {
    setBusy(false);
    askInput.value = "";
  }
}

// Ask the model where an element is; returns {x,y,label} in 0..1 coords.
async function locate(description) {
  const shot = await captureScreen();
  const prompt =
    `In this screenshot, find: "${description}". Reply with ONLY JSON ` +
    `{"x":<0..1>,"y":<0..1>} for the normalized center of that element ` +
    `(0,0 = top-left, 1,1 = bottom-right). No other text.`;
  const reply = await invoke("ask_llm", { prompt, imageDataUrl: shot.data_url, history: [] });
  const j = parseJsonLoose(reply);
  if (j && typeof j.x === "number" && typeof j.y === "number") {
    return { x: j.x, y: j.y, label: description };
  }
  return null;
}

async function runDemo(demo) {
  if (!demo || !Array.isArray(demo.steps)) {
    setStatus("Invalid demo script.");
    return;
  }
  stopFlag = false;
  setBusy(true);
  setStatus(`Demo: ${demo.title || "running"}…`);
  try {
    for (const step of demo.steps) {
      if (stopFlag) break;
      let pt = null;
      if (step.point && typeof step.point.x === "number") {
        pt = { x: step.point.x, y: step.point.y, label: step.point.label || "" };
      } else if (step.find && cfg && cfg.vision) {
        pt = await locate(step.find);
      }
      if (pt) emit("point", { x: pt.x, y: pt.y, label: pt.label || step.find || "" });
      if (step.say) {
        emit("caption", { text: step.say });
        await speak(step.say);
      }
      await sleep(step.hold || 600);
    }
  } catch (e) {
    showResponse(String(e));
  } finally {
    emit("caption", { text: "" });
    emit("clear-pointer");
    setStatus(stopFlag ? "Demo stopped." : "Demo complete. 🎉");
    stopFlag = false;
    setBusy(false);
  }
}

async function autoDemo() {
  if (busy) return;
  setBusy(true);
  setStatus("Studying your screen…");
  try {
    const shot = await captureScreen();
    const prompt =
      "You are an upbeat presenter demoing the app in this screenshot at a " +
      "hackathon. Write a short spoken walkthrough (5-7 steps) for the judges. " +
      'Reply with ONLY JSON, no markdown: {"title":"...","steps":[{"say":"one or ' +
      'two spoken sentences","find":"short description of the element to point ' +
      'at"}]}. Use "find" to point at real things you can see; omit "find" for ' +
      "intro/outro lines.";
    const reply = await invoke("ask_llm", { prompt, imageDataUrl: shot.data_url, history: [] });
    const demo = parseJsonLoose(reply);
    if (!demo || !demo.steps) {
      setStatus("Could not generate a demo.");
      showResponse(reply);
      setBusy(false);
      return;
    }
    demoJson.value = JSON.stringify(demo, null, 2);
    setBusy(false); // runDemo manages its own busy state
    await runDemo(demo);
  } catch (e) {
    setStatus("Error.");
    showResponse(String(e));
    setBusy(false);
  }
}

function stopEverything() {
  stopFlag = true;
  invoke("stop_speaking").catch(() => {});
  emit("caption", { text: "" });
  emit("clear-pointer");
  setStatus("Stopped.");
}

// ---------- file load ----------
function loadDemoFile() {
  fileInput.click();
}

function handleFileLoad(event) {
  const file = event.target.files[0];
  if (!file) return;
  const reader = new FileReader();
  reader.onload = (e) => {
    try {
      const text = e.target.result;
      // Validate it's valid JSON
      JSON.parse(text);
      demoJson.value = text;
      setStatus(`Loaded: ${file.name}`);
      // Open the demo editor so the user can see the loaded script
      $("demo-editor").open = true;
    } catch (err) {
      setStatus("Invalid JSON file.");
    }
  };
  reader.readAsText(file);
  // Reset so the same file can be loaded again
  fileInput.value = "";
}

// ---------- settings & window sizing ----------
function updateWindowSize() {
  if (collapsed) {
    try { getCurrentWindow().setSize(new LogicalSize(380, 46)); } catch (_) {}
    return;
  }
  let h = 300;
  if ($("demo-editor").open) {
    h += 180; // height of the demo script editor
  }
  if (!$("settings").classList.contains("hidden")) {
    h += 380; // height of the settings drawer
  }
  try { getCurrentWindow().setSize(new LogicalSize(380, h)); } catch (_) {}
}

function fillSettings() {
  $("cfg-provider").value = cfg.provider;
  $("cfg-base").value = cfg.base_url;
  $("cfg-key").value = cfg.api_key;
  $("cfg-model").value = cfg.model;
  $("cfg-vision").checked = !!cfg.vision;
  $("cfg-voice").value = cfg.tts_voice;
  $("cfg-rate").value = cfg.tts_rate;
  $("cfg-system").value = cfg.system_prompt;
}

async function saveSettings() {
  cfg = {
    provider: $("cfg-provider").value,
    base_url: $("cfg-base").value.trim(),
    api_key: $("cfg-key").value.trim(),
    model: $("cfg-model").value.trim(),
    vision: $("cfg-vision").checked,
    tts_voice: $("cfg-voice").value.trim(),
    tts_rate: parseInt($("cfg-rate").value, 10) || 0,
    system_prompt: $("cfg-system").value.trim(),
  };
  try {
    await invoke("save_config", { config: cfg });
    setStatus("Settings saved.");
  } catch (e) {
    setStatus("Save failed: " + e);
  }
  $("settings").classList.add("hidden");
  updateWindowSize();
}

function toggleCollapse() {
  collapsed = !collapsed;
  $("main").style.display = collapsed ? "none" : "";
  $("demo-editor").style.display = collapsed ? "none" : "";
  if (collapsed) $("settings").classList.add("hidden");
  updateWindowSize();
}

// ---------- wire up ----------
async function init() {
  try {
    cfg = await invoke("load_config");
  } catch (e) {
    cfg = null;
  }
  if (!cfg) {
    cfg = {
      provider: "openai",
      base_url: "https://api.openai.com/v1",
      api_key: "",
      model: "gpt-4o-mini",
      vision: true,
      tts_voice: "",
      tts_rate: 1,
      system_prompt: "You are Clicky, a friendly on-screen buddy.",
    };
  }
  fillSettings();
  demoJson.value = JSON.stringify(SAMPLE_DEMO, null, 2);

  if (!cfg.api_key && cfg.provider !== "openai") {
    // local/keyless is fine
  } else if (!cfg.api_key) {
    setStatus("Add an API key in ⚙ settings to get started.");
  }

  // Buttons
  $("btn-ask").addEventListener("click", ask);
  askInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") ask();
  });
  $("btn-autodemo").addEventListener("click", autoDemo);
  $("btn-rundemo").addEventListener("click", () => runDemo(parseJsonLoose(demoJson.value)));
  $("btn-stop").addEventListener("click", stopEverything);

  // Microphone
  btnMic.addEventListener("click", toggleMic);
  initSpeechRecognition();

  // File loading
  $("btn-load-demo").addEventListener("click", loadDemoFile);
  fileInput.addEventListener("change", handleFileLoad);

  // Settings
  $("btn-settings").addEventListener("click", () => {
    $("settings").classList.toggle("hidden");
    updateWindowSize();
  });
  $("btn-close-settings").addEventListener("click", () => {
    $("settings").classList.add("hidden");
    updateWindowSize();
  });
  $("btn-save").addEventListener("click", saveSettings);
  $("btn-min").addEventListener("click", toggleCollapse);

  // Demo editor resize
  $("demo-editor").addEventListener("toggle", updateWindowSize);

  document.querySelectorAll(".presets button").forEach((b) => {
    b.addEventListener("click", () => {
      const p = PRESETS[b.dataset.preset];
      if (!p) return;
      $("cfg-base").value = p.base;
      $("cfg-model").value = p.model;
      $("cfg-provider").value = "openai";
    });
  });

  // Global hotkey -> focus the input so the user can start talking/typing.
  listen("hotkey", () => {
    if (collapsed) toggleCollapse();
    getCurrentWindow().show().catch(() => {});
    getCurrentWindow().setFocus().catch(() => {});
    askInput.focus();
  });

  setStatus("Ready — Ctrl+Alt+Space to talk");
}

window.addEventListener("DOMContentLoaded", init);
