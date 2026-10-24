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

/// Join `base` with an API path once. Users often paste the full endpoint
/// (`.../v1/chat/completions`) into the base URL field; appending again 404s.
fn append_endpoint(base: &str, endpoint: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    let suffix = format!("/{endpoint}");
    if trimmed.ends_with(&suffix) {
        trimmed.to_string()
    } else {
        format!("{trimmed}{suffix}")
    }
}

fn openai_chat_url(base: &str) -> String {
    append_endpoint(base, "chat/completions")
}

fn anthropic_messages_url(base: &str) -> String {
    append_endpoint(base, "messages")
}

fn meaningful(s: &str) -> Option<&str> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

fn joined_text_parts(parts: &[Value]) -> Option<String> {
    let joined: String = parts
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("");
    if meaningful(&joined).is_some() {
        Some(joined)
    } else {
        None
    }
}

/// Visible assistant text from an OpenAI-compatible `message` object.
///
/// An empty string is not a reply: some reasoning models send `"content": ""`
/// (or null) and put the words in `reasoning_content`. A content array is the
/// visible reply and wins over `reasoning_content`, which is chain-of-thought.
fn openai_message_text(msg: &Value) -> Option<String> {
    if let Some(s) = msg["content"].as_str() {
        if let Some(text) = meaningful(s) {
            return Some(text.to_string());
        }
    }
    if let Some(parts) = msg["content"].as_array() {
        if let Some(text) = joined_text_parts(parts) {
            return Some(text);
        }
    }
    msg["reasoning_content"]
        .as_str()
        .and_then(meaningful)
        .map(str::to_string)
}

/// Visible assistant text from an Anthropic Messages body.
/// Thinking blocks have no `text`; skip them and keep later text blocks.
fn anthropic_message_text(body: &Value) -> Option<String> {
    if let Some(s) = body["content"].as_str() {
        return meaningful(s).map(str::to_string);
    }
    let parts = body["content"].as_array()?;
    joined_text_parts(parts)
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

    let url = openai_chat_url(&cfg.base_url);
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
    openai_message_text(&v["choices"][0]["message"])
        .ok_or_else(|| format!("unexpected response shape: {}", text))
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

    let url = anthropic_messages_url(&cfg.base_url);
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
    anthropic_message_text(&v).ok_or_else(|| format!("unexpected response shape: {}", text))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_url_appends_once() {
        assert_eq!(
            openai_chat_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            openai_chat_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            openai_chat_url(" https://api.openai.com/v1/chat/completions/ "),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn anthropic_url_appends_once() {
        assert_eq!(
            anthropic_messages_url("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            anthropic_messages_url("https://api.anthropic.com/v1/messages/"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn empty_content_uses_reasoning_text() {
        let msg = json!({
            "content": "",
            "reasoning_content": "Look at the center. [POINT:0.5,0.5:center]"
        });
        assert_eq!(
            openai_message_text(&msg).as_deref(),
            Some("Look at the center. [POINT:0.5,0.5:center]")
        );

        let null_content = json!({
            "content": null,
            "reasoning_content": "fallback reply"
        });
        assert_eq!(
            openai_message_text(&null_content).as_deref(),
            Some("fallback reply")
        );
    }

    #[test]
    fn content_array_beats_reasoning() {
        let msg = json!({
            "content": [
                {"type": "text", "text": "Visible reply "},
                {"type": "text", "text": "[POINT:0.2,0.3:button]"}
            ],
            "reasoning_content": "internal chain of thought that must not be spoken"
        });
        assert_eq!(
            openai_message_text(&msg).as_deref(),
            Some("Visible reply [POINT:0.2,0.3:button]")
        );
    }

    #[test]
    fn plain_string_content_is_unchanged() {
        let msg = json!({"content": "  hello  "});
        assert_eq!(openai_message_text(&msg).as_deref(), Some("  hello  "));
    }

    #[test]
    fn anthropic_skips_thinking_block() {
        let body = json!({
            "content": [
                {"type": "thinking", "thinking": "plan the tour"},
                {"type": "text", "text": "Here is the button. [POINT:0.8,0.9:go]"}
            ]
        });
        assert_eq!(
            anthropic_message_text(&body).as_deref(),
            Some("Here is the button. [POINT:0.8,0.9:go]")
        );
    }

    #[test]
    fn missing_text_is_none() {
        assert_eq!(openai_message_text(&json!({})), None);
        assert_eq!(anthropic_message_text(&json!({"content": []})), None);
        assert_eq!(
            anthropic_message_text(&json!({"content": [{"type": "thinking"}]})),
            None
        );
    }
}
