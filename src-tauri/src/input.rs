//! Clipboard access and typing the result into whatever app has focus.

use anyhow::{anyhow, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// The key that goes with C and V for copying and pasting.
pub const SHORTCUT_KEY: &str = if cfg!(target_os = "macos") {
    "Cmd"
} else {
    "Ctrl"
};

/// What wisperChad pasted last. It stays on the clipboard, so it's no message to reply to.
static PASTED: Mutex<String> = Mutex::new(String::new());

const SUBMIT_DELAY: Duration = Duration::from_millis(500);

/// One long-lived clipboard handle. On Linux (X11/Wayland) clipboard contents only stay
/// available while the handle that set them is alive, so it must not be dropped.
fn clipboard() -> Result<std::sync::MutexGuard<'static, arboard::Clipboard>> {
    static CB: OnceLock<Mutex<arboard::Clipboard>> = OnceLock::new();
    if CB.get().is_none() {
        let cb = arboard::Clipboard::new().map_err(|e| anyhow!("clipboard: {e}"))?;
        let _ = CB.set(Mutex::new(cb));
    }
    Ok(CB.get().unwrap().lock().unwrap_or_else(|e| e.into_inner()))
}

pub fn read_clipboard() -> Option<String> {
    clipboard().ok()?.get_text().ok()
}

/// Wayland keeps apps from seeing or pressing keys outside their own windows.
pub fn wayland() -> bool {
    let var = |name| std::env::var_os(name);
    cfg!(target_os = "linux")
        && (var("WAYLAND_DISPLAY").is_some()
            || var("XDG_SESSION_TYPE").is_some_and(|session| session == "wayland"))
}

/// Whether `text` is what wisperChad pasted last, still on the clipboard, and not something the
/// user copied.
pub fn is_our_paste(text: &str) -> bool {
    let pasted = PASTED.lock().unwrap_or_else(|e| e.into_inner());
    !pasted.is_empty() && same_text(&pasted, text)
}

/// Waits until the user lets go of Ctrl/Alt/Shift/Win, so our simulated Ctrl+V isn't
/// turned into Ctrl+Alt+V by keys that are still held down from the hotkey.
pub fn wait_for_modifiers_released(timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !modifiers_down() {
            break;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    std::thread::sleep(Duration::from_millis(40));
}

#[cfg(windows)]
fn modifiers_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .iter()
        .any(|vk| unsafe { GetAsyncKeyState(*vk as i32) } as u16 & 0x8000 != 0)
}

#[cfg(not(windows))]
fn modifiers_down() -> bool {
    crate::chords::keys::modifiers_down() != 0
}

/// Presses Enter once a paste has had time to land. Any sooner and it submits an empty box.
pub fn press_enter(app: &AppHandle) -> Result<()> {
    if wayland() {
        return Ok(()); // Nothing was pasted, and this wouldn't reach the app in front anyway.
    }
    std::thread::sleep(SUBMIT_DELAY);
    press_keys(app, press_enter_key)
}

fn press_enter_key() -> Result<()> {
    Enigo::new(&Settings::default())
        .map_err(keyboard_error)?
        .key(Key::Return, Direction::Click)
        .map_err(|e| anyhow!("{e}"))
}

/// Another app can hold the clipboard for a moment, so this checks the text really got there.
fn put_on_clipboard(clipboard: &mut arboard::Clipboard, text: &str) -> bool {
    for _ in 0..20 {
        let _ = clipboard.set_text(text.to_string());
        if clipboard.get_text().is_ok_and(|now| same_text(&now, text)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// Windows can hand text back with different line endings.
fn same_text(a: &str, b: &str) -> bool {
    a.replace("\r\n", "\n") == b.replace("\r\n", "\n")
}

/// Puts `text` on the clipboard and presses Ctrl+V (Cmd+V on macOS). Pasting is far more reliable
/// than simulating each keystroke. The text stays on the clipboard, so if the user clicked out of
/// the box while it was being written, they can still paste it themselves. Under Wayland that's
/// all it does, because a press from here wouldn't reach the app in front.
pub fn paste_text(app: &AppHandle, text: &str) -> Result<()> {
    let mut board = clipboard()?;
    if !put_on_clipboard(&mut board, text) {
        // Pressing Ctrl+V now would paste whatever was there before.
        return Err(anyhow!(
            "Couldn't use the clipboard, so nothing was pasted. Try again."
        ));
    }
    drop(board);
    *PASTED.lock().unwrap_or_else(|e| e.into_inner()) = text.to_string();
    if wayland() {
        return Ok(());
    }
    std::thread::sleep(Duration::from_millis(30));

    press_keys(app, press_paste_keys)?;
    Ok(())
}

/// macOS only answers questions about the keyboard layout on the main thread, and finding the
/// V key needs one.
#[cfg(target_os = "macos")]
fn press_keys(app: &AppHandle, keys: fn() -> Result<()>) -> Result<()> {
    let (sender, result) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(keys());
    })?;
    result.recv()?
}

#[cfg(not(target_os = "macos"))]
fn press_keys(_app: &AppHandle, keys: fn() -> Result<()>) -> Result<()> {
    keys()
}

fn press_paste_keys() -> Result<()> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(keyboard_error)?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| anyhow!("{e}"))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| anyhow!("{e}"))?;
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| anyhow!("{e}"))
}

#[cfg(target_os = "macos")]
fn keyboard_error(_: enigo::NewConError) -> anyhow::Error {
    anyhow!("Your text is on the clipboard. To paste it for you, wisperChad needs Accessibility in System Settings, under Privacy & Security")
}

#[cfg(not(target_os = "macos"))]
fn keyboard_error(e: enigo::NewConError) -> anyhow::Error {
    anyhow!("keyboard: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_dont_make_text_different() {
        assert!(same_text("Hi\r\nThere", "Hi\nThere"));
        assert!(!same_text("Hi", "Bye"));
    }
}
