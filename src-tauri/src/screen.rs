//! Screenshots of the window the user is working in, for replies that read the screen. They stay
//! in memory and go only to the writing model on this PC: nothing is written to disk.

use crate::window_text;
use anyhow::{anyhow, Result};
use base64::Engine;
use image::{imageops, DynamicImage, ImageFormat, RgbaImage};
use std::io::Cursor;
use std::sync::mpsc::Receiver;
use std::time::Duration;
use xcap::{Monitor, Window};

/// About 4,500 tokens for a vision model, which leaves room for the prompt and the reply inside
/// the context window. A 3440x1440 screen fits as it is; larger ones get scaled down.
const MAX_PIXELS: u64 = 5_000_000;

/// How long a reply waits for the window's text after the user stops speaking.
const TEXT_WAIT: Duration = Duration::from_millis(2500);

/// What the reply hotkey saw. The text arrives a moment after the picture.
pub struct Capture {
    image: RgbaImage,
    text: Receiver<String>,
    window: Active,
}

/// The window the user is in: its title, and the name of the program it belongs to, like
/// "Microsoft Outlook", for when the title doesn't say which app it is.
#[derive(Clone, Debug, Default)]
pub struct Active {
    pub title: String,
    pub app: String,
}

/// What a reply read from the screen.
pub struct Seen {
    /// The screenshot, as base64 PNG.
    pub image: String,
    /// The window's text, empty if the app didn't share any.
    pub text: String,
    pub window: Active,
}

impl Capture {
    pub fn finish(self) -> Result<Seen> {
        let text = self.text.recv_timeout(TEXT_WAIT).unwrap_or_default();
        Ok(Seen {
            image: encode(self.image)?,
            text,
            window: self.window,
        })
    }
}

/// An empty picture, for warming up a model that can read images.
pub fn blank_image() -> Result<String> {
    encode(RgbaImage::new(256, 256))
}

pub fn capture() -> Result<Capture> {
    check_permission()?;
    let text = window_text::read_active();
    let (image, window) = screenshot()?;
    Ok(Capture {
        image,
        text,
        window,
    })
}

/// Without permission macOS still hands over a picture, of the wallpaper with the windows missing,
/// so this has to be asked. Asking also puts wisperChad on the list in System Settings.
#[cfg(target_os = "macos")]
fn check_permission() -> Result<()> {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }
    let allowed = unsafe { CGPreflightScreenCaptureAccess() || CGRequestScreenCaptureAccess() };
    anyhow::ensure!(
        allowed,
        "Reading the open window needs Screen Recording in System Settings, under Privacy & Security. Allow wisperChad there, then reopen it."
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn check_permission() -> Result<()> {
    Ok(())
}

/// The active window, for replies that use copied text and so take no screenshot.
pub fn active_window() -> Active {
    Window::all()
        .ok()
        .and_then(|windows| {
            windows
                .into_iter()
                .find(|w| w.is_focused().unwrap_or(false))
        })
        .map(|window| about(&window))
        .unwrap_or_default()
}

fn about(window: &Window) -> Active {
    Active {
        title: window.title().unwrap_or_default(),
        app: window.app_name().unwrap_or_default(),
    }
}

/// Takes the active window and says which it is, or the whole monitor if the window can't be
/// captured.
fn screenshot() -> Result<(RgbaImage, Active)> {
    let windows = Window::all().map_err(|e| anyhow!("Could not read the screen: {e}"))?;
    if let Some(window) = windows
        .into_iter()
        .find(|w| w.is_focused().unwrap_or(false))
    {
        let active = about(&window);
        if let Ok(image) = window.capture_image() {
            if !is_blank(&image) {
                return Ok((image, active));
            }
        }
        // Some apps paint nothing when asked for a window image, but always show on the monitor.
        if let Ok(image) = window.current_monitor().and_then(|m| m.capture_image()) {
            return Ok((image, active));
        }
    }
    let monitors = Monitor::all().map_err(|e| anyhow!("Could not read the screen: {e}"))?;
    let primary = monitors
        .into_iter()
        .find(|m| m.is_primary().unwrap_or(false))
        .ok_or_else(|| anyhow!("Could not find a screen to capture"))?;
    let image = primary
        .capture_image()
        .map_err(|e| anyhow!("Could not read the screen: {e}"))?;
    Ok((image, Active::default()))
}

/// Base64 PNG for the model. PNG rather than JPEG because small text is what the model has to read.
fn encode(image: RgbaImage) -> Result<String> {
    let (width, height) = fit_pixels(image.width(), image.height());
    let image = if width == image.width() {
        image
    } else {
        imageops::resize(&image, width, height, imageops::FilterType::Triangle)
    };
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(image)
        .to_rgb8()
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png))
}

fn is_blank(image: &RgbaImage) -> bool {
    let mut pixels = image.pixels().map(|p| [p[0], p[1], p[2]]);
    let first = pixels.next();
    pixels.all(|p| Some(p) == first)
}

fn fit_pixels(width: u32, height: u32) -> (u32, u32) {
    let pixels = width as u64 * height as u64;
    if pixels <= MAX_PIXELS {
        return (width, height);
    }
    let scale = (MAX_PIXELS as f64 / pixels as f64).sqrt();
    (
        (width as f64 * scale) as u32,
        (height as f64 * scale) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn only_large_screens_are_scaled_down() {
        assert_eq!(fit_pixels(3440, 1440), (3440, 1440));
        let (w, h) = fit_pixels(3840, 2160);
        assert!(w as u64 * h as u64 <= MAX_PIXELS);
        assert!((w as f64 / h as f64 - 3840.0 / 2160.0).abs() < 0.01);
    }

    #[test]
    fn blank_images_are_spotted() {
        assert!(is_blank(&RgbaImage::new(4, 4)));
        let mut image = RgbaImage::new(4, 4);
        image.put_pixel(2, 3, Rgba([255, 255, 255, 255]));
        assert!(!is_blank(&image));
    }

    #[test]
    fn encodes_to_base64_png() {
        let encoded = encode(RgbaImage::new(8, 8)).unwrap();
        assert!(encoded.starts_with("iVBOR"));
    }
}
