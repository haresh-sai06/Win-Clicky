// Text-to-speech using the FREE, built-in Windows voice (System.Speech / SAPI).
//
// No API key, no network. We drive it through a hidden PowerShell process so we
// don't need a native crate. `speak` blocks until the line finishes (great for
// sequencing a demo: "say this, then point"), and `stop_speaking` kills it.
//
// Text and voice name are base64-encoded into the script to avoid every
// possible quoting/escaping headache, then the whole script is passed via
// PowerShell's -EncodedCommand (UTF-16LE base64).

use base64::Engine;
use std::sync::Mutex;
use tauri::State;

/// Holds the PID of the currently-speaking PowerShell process, if any.
#[derive(Default)]
pub struct TtsState(pub Mutex<Option<u32>>);

#[tauri::command]
pub async fn speak(
    text: String,
    rate: i32,
    voice: String,
    state: State<'_, TtsState>,
) -> Result<(), String> {
    // Interrupt anything currently being spoken.
    kill_current(&state);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let encoded = build_encoded_command(&text, rate, &voice);
        let mut child = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;

        {
            *state.0.lock().unwrap() = Some(child.id());
        }

        let wait = tauri::async_runtime::spawn_blocking(move || child.wait())
            .await
            .map_err(|e| e.to_string())?;

        {
            *state.0.lock().unwrap() = None;
        }
        wait.map(|_| ()).map_err(|e| e.to_string())
    }

    #[cfg(not(windows))]
    {
        // Non-Windows dev fallback: just log so the app still compiles/runs.
        let _ = (rate, voice);
        println!("[tts] {}", text);
        Ok(())
    }
}

#[tauri::command]
pub fn stop_speaking(state: State<'_, TtsState>) -> Result<(), String> {
    kill_current(&state);
    Ok(())
}

fn kill_current(state: &State<'_, TtsState>) {
    let pid = { state.0.lock().unwrap().take() };
    if let Some(pid) = pid {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            use std::process::Command;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
        }
    }
}

#[cfg(windows)]
fn build_encoded_command(text: &str, rate: i32, voice: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD;
    let text_b64 = b64.encode(text.as_bytes());
    let voice_b64 = b64.encode(voice.as_bytes());
    let rate = rate.clamp(-10, 10);

    let script = format!(
        "$ti=[Convert]::FromBase64String('{text_b64}');\
         $t=[System.Text.Encoding]::UTF8.GetString($ti);\
         $vi=[Convert]::FromBase64String('{voice_b64}');\
         $v=[System.Text.Encoding]::UTF8.GetString($vi);\
         Add-Type -AssemblyName System.Speech;\
         $s=New-Object System.Speech.Synthesis.SpeechSynthesizer;\
         $s.Rate={rate};\
         if($v.Length -gt 0){{try{{$s.SelectVoice($v)}}catch{{}}}}\
         $s.Speak($t);"
    );

    // PowerShell -EncodedCommand expects UTF-16LE, base64-encoded.
    let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    b64.encode(utf16)
}
