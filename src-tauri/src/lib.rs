// Clicky for Windows - library entry point.
//
// Two windows are declared in tauri.conf.json:
//   * "panel"   - the small, draggable, always-on-top buddy card (interactive)
//   * "overlay" - a full-screen, transparent, click-through layer that draws
//                 the flying pointer and the subtitle captions during a demo.
//
// All the heavy lifting lives in the backend modules below and is exposed to
// the web UI as Tauri commands. Cross-window messaging is done with events.

mod capture;
mod config;
mod llm;
mod point;
mod tts;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Global push-to-talk / wake hotkey: Ctrl+Alt+Space anywhere on the
        // system focuses the buddy and tells the panel to start a turn.
        .plugin({
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{ShortcutState};

                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(move |app, _shortcut, event| {
                        if event.state == ShortcutState::Pressed {
                            let _ = app.emit("hotkey", ());
                        }
                    })
                    .build()
            }
            #[cfg(not(desktop))]
            {
                tauri_plugin_global_shortcut::Builder::new().build()
            }
        })
        .manage(tts::TtsState::default())
        .invoke_handler(tauri::generate_handler![
            capture::capture_screen,
            llm::ask_llm,
            tts::speak,
            tts::stop_speaking,
            config::load_config,
            config::save_config,
        ])
        .setup(|app| {
            // Register hotkey dynamically and gracefully handle error if already in use
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{
                    Code, GlobalShortcutExt, Modifiers, Shortcut,
                };
                let hotkey =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
                if let Err(e) = app.global_shortcut().register(hotkey) {
                    eprintln!("WARNING: Failed to register global hotkey (Ctrl+Alt+Space): {}. The app will still run, but the global hotkey won't be active.", e);
                }
            }

            // --- Configure the overlay window ---------------------------------
            // Make it cover the primary monitor and pass every click straight
            // through to whatever is underneath (it is purely visual).
            if let Some(overlay) = app.get_webview_window("overlay") {
                if let Ok(Some(monitor)) = overlay.primary_monitor() {
                    let size = *monitor.size();
                    let pos = *monitor.position();
                    let _ =
                        overlay.set_position(tauri::PhysicalPosition::new(pos.x, pos.y));
                    let _ =
                        overlay.set_size(tauri::PhysicalSize::new(size.width, size.height - 1));
                }
                let _ = overlay.set_ignore_cursor_events(true);
                let _ = overlay.set_always_on_top(true);
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the Clicky application");
}
