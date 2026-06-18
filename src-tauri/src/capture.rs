// Screen capture via the `xcap` crate (v0.9+).
//
// Returns a PNG data URL plus the pixel dimensions of the (possibly
// downscaled) image. The LLM is told the image is `width` x `height`, and the
// overlay maps the coordinates it returns back onto the real screen, so this
// dimension pair is the single source of truth for the coordinate space.

use base64::Engine;
use image::{ImageBuffer, Rgba};
use serde::Serialize;
use xcap::Monitor;

// Vision models don't need a 4K screenshot; cap the long edge to keep token
// cost and latency sane. Pointing accuracy at this size is still pixel-fine.
const MAX_WIDTH: u32 = 1280;

#[derive(Serialize)]
pub struct Screenshot {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub async fn capture_screen() -> Result<Screenshot, String> {
    // xcap is blocking; run it off the async executor.
    tauri::async_runtime::spawn_blocking(|| capture_blocking())
        .await
        .map_err(|e| e.to_string())?
}

fn capture_blocking() -> Result<Screenshot, String> {
    let monitors = Monitor::all().map_err(|e| e.to_string())?;
    // MVP: capture the first monitor. (Multi-monitor selection is a TODO.)
    let monitor = monitors
        .into_iter()
        .next()
        .ok_or_else(|| "no monitor found".to_string())?;

    let image = monitor.capture_image().map_err(|e| e.to_string())?;
    let src_w = image.width();
    let src_h = image.height();

    let final_image = maybe_downscale(&image, src_w, src_h, MAX_WIDTH);
    let width = final_image.width();
    let height = final_image.height();

    // Encode as PNG using the `image` crate's built-in encoder.
    let mut png_buf: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_buf);
    final_image
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_buf);
    Ok(Screenshot {
        data_url: format!("data:image/png;base64,{}", b64),
        width,
        height,
    })
}

/// Nearest-neighbour downscale of an RgbaImage. Returns the source unchanged if
/// it is already within `max_w`.
fn maybe_downscale(
    src: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    w: u32,
    h: u32,
    max_w: u32,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    if w <= max_w {
        return src.clone();
    }
    let scale = max_w as f32 / w as f32;
    let dst_w = max_w;
    let dst_h = ((h as f32) * scale).round().max(1.0) as u32;

    image::imageops::resize(src, dst_w, dst_h, image::imageops::FilterType::Triangle)
}
