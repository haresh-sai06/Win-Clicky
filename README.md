# Clicky for Windows

An AI buddy that lives next to your cursor. It can see your screen, talk to you with a free built-in Windows voice, and physically **point at things** by flying a glowing cursor across a transparent, click-through overlay — and it has a **Hackathon Demo Mode** that narrates and points through your project autonomously.

This is a Windows port of the idea behind [farzaa/clicky](https://github.com/farzaa/clicky) (which is macOS/Swift), rebuilt on **Tauri 2 (Rust + web UI)** with a **swappable LLM** and **no paid voice/transcription keys required**.

> Status: this is a working **MVP scaffold**. The code is complete and intended to run with `cargo tauri dev` on Windows.

---

## What it does

- **Floating buddy** — a small always-on-top card you can drag anywhere, with a premium glassmorphism design.
- **Ask about your screen** — press `Ctrl+Alt+Space` (or type, or use the 🎙️ mic button for speech-to-text), it grabs a screenshot, sends it to your chosen LLM, then **speaks the answer and points** at the relevant spots.
- **Auto-demo** — it looks at your running app and writes + performs a spoken hackathon demo on its own.
- **Run script** — feed it a JSON demo script (paste or load from file) and it narrates + points through each step, hands-free. Great for recording a demo video or presenting live.
- **Speech-to-text** — click the 🎙️ mic button to dictate your question using the Web Speech API (no API key needed).

---

## Requirements (Windows 10/11)

1. **Rust** — install from <https://rustup.rs> (gives you `cargo`).
2. **Microsoft C++ Build Tools** — the "Desktop development with C++" workload from the Visual Studio Installer (Rust's MSVC toolchain needs this).
3. **WebView2 Runtime** — preinstalled on current Windows 10/11. If missing, get the Evergreen runtime from Microsoft.
4. **Tauri CLI** — install once:
   ```powershell
   cargo install tauri-cli --version "^2.0"
   ```

No Node.js is required: the UI is plain static HTML/CSS/JS and the Tauri JS API is exposed globally (`withGlobalTauri`).

---

## Run it

From the project root (the folder with this README):

```powershell
cargo tauri dev
```

First build takes a few minutes (Rust compiles a lot the first time). Two windows appear: the **buddy panel** and an invisible full-screen **overlay**.

To produce a distributable `.exe` / installer:

```powershell
cargo tauri build
```

---

## Configure the LLM (swappable)

Click the **⚙** gear on the panel. Pick a **Quick fill** preset or enter your own:

| Provider | Base URL | Example model | Vision? |
|---|---|---|---|
| OpenAI | `https://api.openai.com/v1` | `gpt-4o-mini` | yes |
| Groq | `https://api.groq.com/openai/v1` | a vision model | varies |
| OpenRouter | `https://openrouter.ai/api/v1` | `openai/gpt-4o-mini` | yes |
| Ollama (local, no key) | `http://localhost:11434/v1` | `llava` | yes |
| Anthropic | `https://api.anthropic.com/v1` | `claude-3-5-sonnet-latest` | yes |

Anything that speaks the **OpenAI Chat Completions** format works — just set the base URL, model, and key. For **Anthropic**, switch the provider dropdown to `Anthropic`.

**Vision matters:** "Ask about your screen," `find`-based pointing, and Auto-demo all send a screenshot, so they need a **vision-capable model**. With a text-only model, turn off *Send screenshots* and use scripted demos with explicit coordinates.

Settings are saved to `%APPDATA%\clicky\config.json` (you can edit that file directly too).

---

## The free Windows voice

Speech uses the built-in **Windows SAPI / System.Speech** synthesizer through a hidden PowerShell process — no key, no network. To see the voices you have installed:

```powershell
Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).GetInstalledVoices() | %{ $_.VoiceInfo.Name }
```

Put one of those names (e.g. `Microsoft Zira Desktop`) in the **Voice** field, or leave it blank for the system default. Add more voices via Windows Settings → Time & Language → Speech. (Want premium voices later? `tts.rs` is the only file to swap — e.g. point it at ElevenLabs.)

---

## Speech-to-text (voice input)

Click the **🎙️** mic button on the panel to dictate your question using the [Web Speech API](https://developer.mozilla.org/en-US/docs/Web/API/SpeechRecognition). This works inside Tauri's WebView2 (Chromium-based) with no API key — it uses the browser's built-in speech recognition.

- The button turns 🔴 red while listening.
- Interim results appear in the text box as you speak.
- When you stop talking, it automatically submits the question.
- If speech recognition isn't available in your WebView, the button is dimmed.

---

## Hackathon Demo Mode

Two ways to drive a demo:

**Auto-demo** — click **✨ Auto-demo**. Clicky screenshots your app, asks the LLM to write a 5–7 step spoken walkthrough, fills the script editor, and performs it.

**Run script** — open *Demo script (JSON)* on the panel (a sample is preloaded), edit it or **📂 Load file** from disk, and click **▶ Run script**. Each step:

```json
{
  "say": "What the buddy says out loud",
  "find": "describe the UI element to point at (uses the LLM + a screenshot)",
  "point": { "x": 0.5, "y": 0.1, "label": "or give exact 0-1 coordinates" },
  "hold": 1200
}
```

- `say` — narrated with the Windows voice (the runner waits until it finishes).
- `find` — a description; Clicky screenshots the screen and asks the model for the element's location. Resolution-independent, but needs a vision model.
- `point` — explicit normalized coordinates (`x`,`y` from 0 to 1). Runs **offline**, perfectly repeatable. `label` shows in a bubble.
- `hold` — milliseconds to linger before the next step (default ~600).

Hit **⏹ Stop** anytime to cut speech and clear the overlay. Sample scripts live in `demo-scripts/`.

---

## How it works

```
src/                      Static web UI (no bundler)
  index.html / panel.*    The buddy card: ask flow, demo runner, settings, STT
  overlay.html / overlay.*  Transparent, click-through layer: flying pointer + subtitles
src-tauri/                Rust backend
  src/lib.rs              App setup: overlay click-through, global hotkey
  src/capture.rs          Screenshot via `xcap` -> downscaled PNG data URL
  src/llm.rs              OpenAI-compatible + Anthropic clients
  src/tts.rs              Free Windows SAPI voice via hidden PowerShell
  src/config.rs           Load/save settings JSON
  tauri.conf.json         Two windows: "panel" + "overlay"
  capabilities/           Permissions for the windows
demo-scripts/             Example demo scripts
```

The panel orchestrates everything and talks to the overlay purely through Tauri **events** (`point`, `clear-pointer`, `caption`). All coordinates are **normalized 0–1**, so pointing is correct regardless of monitor resolution or Windows display scaling. The LLM can embed `[POINT:0.52,0.13:label]` tags in any reply to point while it talks.

---

## Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `tauri` | 2.x | App framework |
| `tauri-plugin-global-shortcut` | 2.x | System-wide Ctrl+Alt+Space hotkey |
| `xcap` | 0.9.x | Screen capture |
| `image` | 0.25.x | PNG encoding & image downscaling |
| `reqwest` | 0.12.x | HTTP client for LLM APIs |
| `serde` / `serde_json` | 1.x | JSON serialization |
| `base64` | 0.22.x | Base64 encoding for screenshots |
| `dirs` | 5.x | Platform config directory |

---

## Known limits / TODO

- **Multi-monitor**: captures the first monitor only. Picking a monitor is a small addition in `capture.rs`.
- **Click-through**: the overlay is globally click-through (it's purely visual), which sidesteps the known Windows `setIgnoreCursorEvents` per-region issues.
- **No system tray yet**: the panel stays on screen; the **—** button collapses it.

## Troubleshooting

- **A crate version fails to resolve / an API changed**: run `cargo add xcap` or `cargo add image` to pull the latest and check their docs. The capture code uses `Monitor::all()`, `capture_image()`, `width()`, `height()`, and `image::imageops::resize()`.
- **Global hotkey doesn't fire**: another app may own `Ctrl+Alt+Space`. Change it in `src-tauri/src/lib.rs` (the `Shortcut::new(...)` line).
- **No voice**: confirm a voice exists with the PowerShell command above; some minimal Windows installs ship none.
- **LLM error in the response box**: usually a missing/wrong API key, base URL, or a non-vision model with screenshots enabled.
- **Mic button greyed out**: Web Speech API may not be available in your WebView2 version. Update WebView2 runtime.

Licensed under MIT (same spirit as the original Clicky).
