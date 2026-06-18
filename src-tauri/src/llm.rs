// Swappable LLM client.
//
// `provider = "openai"`  -> OpenAI Chat Completions protocol. Works with
//                           OpenAI, Groq, OpenRouter, Together, LM Studio,
//                           Ollama (http://localhost:11434/v1), etc.
// `provider = "anthropic"` -> Anthropic Messages API.
//
// The model may embed pointing tags like `[POINT:x,y:label]` in its reply; the
// frontend parses those out and drives the overlay. We only define the format
// here in the system prompt -- the actual coordinate space (image width/height)
// is provided by the caller in the user message.

use crate::config::{self, AppConfig};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatMsg {
    pub role: String, // "user" | "assistant"
    pub content: String,
}

const BASE_INSTRUCTIONS: &str = "\
You live on the user's screen and can point at things. When you want to draw \
the user's attention to a specific spot, embed a tag exactly like \
[POINT:X,Y:short label] inline in your reply, where X and Y are NORMALIZED \
fractions of the shared screenshot from 0.0 to 1.0: X=0.0 is the left edge, \
X=1.0 the right edge, Y=0.0 the top, Y=1.0 the bottom (so the center is \
[POINT:0.5,0.5:...]). You may use several POINT tags. Speak naturally and \
concisely - everything that is NOT a POINT tag will be read aloud, so never \
read coordinates out loud. If no screenshot is provided, just answer normally \
without POINT tags.";

#[tauri::command]
pub async fn ask_llm(
    prompt: String,
    image_data_url: Option<String>,
    history: Option<Vec<ChatMsg>>,
) -> Result<String, String> {
    let cfg = config::load_config();
    let history = history.unwrap_or_default();

    match cfg.provider.as_str() {
        "anthropic" => ask_anthropic(&cfg, &prompt, &image_data_url, &history).await,
        _ => ask_openai(&cfg, &prompt, &image_data_url, &history).await,
    }
}

fn system_prompt(cfg: &AppConfig) -> String {
    format!("{}\n\n{}", cfg.system_prompt, BASE_INSTRUCTIONS)
}

async fn ask_openai(
    cfg: &AppConfig,
    prompt: &str,
    image: &Option<String>,
    history: &[ChatMsg],
) -> Result<String, String> {
    let mut messages: Vec<Value> = Vec::new();
    messages.push(json!({ "role": "system", "content": system_prompt(cfg) }));
    for m in history {
        messages.push(json!({ "role": m.role, "content": m.content }));
    }

    // Current turn: text + optional image.
    let user_content = match image {
        Some(url) => json!([
            { "type": "text", "text": prompt },
            { "type": "image_url", "image_url": { "url": url } }
        ]),
        None => json!(prompt),
    };
    messages.push(json!({ "role": "user", "content": user_content }));

    let body = json!({
        "model": cfg.model,
        "messages": messages,
        "max_tokens": 4096,   // Reasoning models burn tokens on thinking; give them room
        "temperature": 0.5
    });

    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client
        .post(&url)
        .header("HTTP-Referer", "https://github.com/clicky-windows")
        .header("X-Title", "Clicky for Windows")
        .json(&body);
    if !cfg.api_key.is_empty() {
        req = req.bearer_auth(&cfg.api_key);
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("LLM error {}: {}", status, text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let msg = &v["choices"][0]["message"];
    // Standard field
    if let Some(s) = msg["content"].as_str() {
        return Ok(s.to_string());
    }
    // Reasoning / thinking models (DeepSeek-R1, Nemotron-omni, etc.) put the
    // visible reply in `reasoning_content` when `content` is null.
    if let Some(s) = msg["reasoning_content"].as_str() {
        return Ok(s.to_string());
    }
    // OpenRouter wraps some models with a nested content array
    if let Some(arr) = msg["content"].as_array() {
        let joined: String = arr.iter()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("");
        if !joined.is_empty() { return Ok(joined); }
    }
    Err(format!("unexpected response shape: {}", text))
}

async fn ask_anthropic(
    cfg: &AppConfig,
    prompt: &str,
    image: &Option<String>,
    history: &[ChatMsg],
) -> Result<String, String> {
    let mut messages: Vec<Value> = Vec::new();
    for m in history {
        messages.push(json!({ "role": m.role, "content": m.content }));
    }

    let user_content = match image {
        Some(url) => {
            // Strip the "data:image/png;base64," prefix for Anthropic.
            let b64 = url.split(',').nth(1).unwrap_or(url);
            json!([
                { "type": "text", "text": prompt },
                { "type": "image", "source": {
                    "type": "base64", "media_type": "image/png", "data": b64 } }
            ])
        }
        None => json!([{ "type": "text", "text": prompt }]),
    };
    messages.push(json!({ "role": "user", "content": user_content }));

    let body = json!({
        "model": cfg.model,
        "max_tokens": 900,
        "system": system_prompt(cfg),
        "messages": messages
    });

    let url = format!("{}/messages", cfg.base_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("x-api-key", &cfg.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("LLM error {}: {}", status, text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    v["content"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("unexpected response shape: {}", text))
}
