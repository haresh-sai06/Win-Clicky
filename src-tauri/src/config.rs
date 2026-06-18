// Persistent, user-editable settings.
//
// Stored as JSON at: %APPDATA%\clicky\config.json  (via dirs::config_dir()).
// The LLM is fully swappable: anything that speaks the OpenAI Chat Completions
// protocol works by just changing `base_url` + `model` (OpenAI, Groq,
// OpenRouter, LM Studio, Ollama, ...). Anthropic's native API is also supported
// by setting `provider` to "anthropic".

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    /// "openai" (any OpenAI-compatible endpoint) or "anthropic".
    pub provider: String,
    /// Base URL of the API, e.g. "https://api.openai.com/v1".
    pub base_url: String,
    /// API key / bearer token. Leave empty for keyless local servers (Ollama).
    pub api_key: String,
    /// Model id, e.g. "gpt-4o-mini", "claude-3-5-sonnet-latest", "llama3.2".
    pub model: String,
    /// Whether to send a screenshot to the model (requires a vision model).
    pub vision: bool,
    /// Windows SAPI voice name, e.g. "Microsoft Zira Desktop". Empty = default.
    pub tts_voice: String,
    /// Speech rate, -10 (slow) .. 10 (fast).
    pub tts_rate: i32,
    /// Extra persona/instructions appended to the system prompt.
    pub system_prompt: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            provider: "openai".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: String::new(),
            model: "gpt-4o-mini".into(),
            vision: true,
            tts_voice: String::new(),
            tts_rate: 1,
            system_prompt: "You are Clicky, a friendly, concise on-screen teaching buddy."
                .into(),
        }
    }
}

fn config_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("clicky");
    let _ = std::fs::create_dir_all(&dir);
    dir.push("config.json");
    dir
}

#[tauri::command]
pub fn load_config() -> AppConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

#[tauri::command]
pub fn save_config(config: AppConfig) -> Result<(), String> {
    let path = config_path();
    let text = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(())
}
