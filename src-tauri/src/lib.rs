//! wisperChad: free, local, private voice dictation and reply writing.
//!
//! Flow: hold a hotkey -> record mic -> Whisper or Parakeet transcribes -> (optional) a local
//! writing model cleans it up or writes a reply -> text is pasted where your cursor is.

mod applog;
mod audio;
mod catalog;
mod checks;
mod chords;
mod download;
mod engine;
mod hardware;
mod input;
mod llm;
mod numbers;
mod permissions;
mod presets;
mod presses;
mod prompts;
mod reply;
mod screen;
mod settings;
mod speakers;
mod stt;
mod window_text;

use serde::Serialize;
use settings::Settings;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const OLLAMA: &str = "ollama";
const BUILT_IN: &str = "builtin";
/// How many more tries a reply gets to fix the slips checks.rs finds in it. Each is a few seconds
/// without a graphics card, and two fixed most of a small model's slips.
const FIX_TRIES: usize = 2;

/// Where the model stops when finding the message: past who it's from and the message's first
/// lines, which say where it is on the page. The rest is read from there. Copying all of a long
/// email word for word took a small model more time than writing the reply.
const FIND_TOKENS: u32 = 120;
/// Where the model stops when saying what the message's pictures show. One or two sentences fit;
/// past that it was describing a company's logo, at length.
const PICTURE_TOKENS: u32 = 80;

/// Passed when the system starts the app at login, as opposed to the user opening it.
const AUTOSTART_FLAG: &str = "--autostart";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Dictate,
    Reply,
}

pub struct AppState {
    settings: Mutex<Settings>,
    config_dir: PathBuf,
    /// Holds the models, and reply-prompts.json when replies are being tuned.
    data_dir: PathBuf,
    models_dir: PathBuf,
    recorder: audio::Recorder,
    stt: Arc<stt::Stt>,
    engine: Arc<engine::Engine>,
    hotkeys: Mutex<Vec<(Shortcut, Mode)>>,
    recording: Mutex<Option<Mode>>,
    keys_down: Mutex<Vec<Mode>>,
    presses: Mutex<presses::Presses>,
    /// Serialises (un)registering hotkeys; overlapping calls can leak an OS-level key grab.
    hotkey_lock: Mutex<()>,
    busy: AtomicBool,
    /// The dictation or reply being worked on, so the bar's cancel button can drop it.
    pipeline: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// The active window as it looked when the reply hotkey went down, until the reply uses it.
    capture: Mutex<Option<screen::Capture>>,
    /// The message a reply is for, being read from `capture` while the user talks.
    reading: Mutex<Option<Reading>>,
    /// The window a reply is for, when it uses copied text instead of `capture`.
    reply_window: Mutex<screen::Active>,
}

impl AppState {
    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
}

// ---------------------------------------------------------------- status pill

#[derive(Clone, Serialize)]
struct PillStatus {
    state: &'static str, // listening | transcribing | writing | done | error | hidden
    mode: Option<Mode>,
    message: String,
}

/// The window's own `show` activates it. That does nothing while another app is in front, but when
/// wisperChad is, the bar takes the focus and the paste lands on the bar instead of in the box.
#[cfg(windows)]
fn show_pill(window: &tauri::WebviewWindow, visible: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindowAsync, SW_HIDE, SW_SHOWNOACTIVATE};
    let command = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
    if let Ok(handle) = window.hwnd() {
        let _ = unsafe { ShowWindowAsync(HWND(handle.0), command) };
    }
}

/// Puts the bar on screen without making it the window that takes typing.
#[cfg(target_os = "macos")]
fn show_pill(window: &tauri::WebviewWindow, visible: bool) {
    if !visible {
        let _ = window.hide();
        return;
    }
    let bar = window.clone();
    let _ = window.run_on_main_thread(move || {
        if let Ok(handle) = bar.ns_window() {
            let handle = handle.cast::<objc2::runtime::AnyObject>();
            let _: () = unsafe { objc2::msg_send![handle, orderFrontRegardless] };
        }
    });
}

#[cfg(target_os = "linux")]
fn show_pill(window: &tauri::WebviewWindow, visible: bool) {
    let _ = if visible {
        window.show()
    } else {
        window.hide()
    };
}

/// Lets the bar show on every desktop and over a full-screen app, where most typing happens on a
/// laptop. It's only a setting on the window, and runs during setup, on the main thread.
#[cfg(target_os = "macos")]
fn show_pill_everywhere(window: &tauri::WebviewWindow) {
    const ON_ALL_DESKTOPS: usize = 1 << 0;
    const OVER_FULL_SCREEN_APPS: usize = 1 << 8;
    if let Ok(handle) = window.ns_window() {
        let handle = handle.cast::<objc2::runtime::AnyObject>();
        let behavior = ON_ALL_DESKTOPS | OVER_FULL_SCREEN_APPS;
        let _: () = unsafe { objc2::msg_send![handle, setCollectionBehavior: behavior] };
    }
}

fn pill(app: &AppHandle, state: &'static str, mode: Option<Mode>, message: impl Into<String>) {
    let message = message.into();
    if let Some(w) = app.get_webview_window("pill") {
        show_pill(&w, state != "hidden");
    }
    let _ = app.emit(
        "pill-status",
        PillStatus {
            state,
            mode,
            message,
        },
    );
}

fn hide_pill_later(app: &AppHandle, ms: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(ms)).await;
        let st = app.state::<AppState>();
        if st.recording.lock().unwrap().is_none() && !st.busy.load(Ordering::SeqCst) {
            pill(&app, "hidden", None, "");
        }
    });
}

/// How far the bar's window sits above the taskbar or the Dock, in points. The bar is drawn in the
/// middle of its window, 17 points up, so it shows 59 points above either one. That's where it has
/// always been on Windows.
const ABOVE_TASKBAR: f64 = 42.0;

fn create_pill_window(app: &AppHandle) -> tauri::Result<()> {
    let (w, h) = (420.0, 76.0);
    let win = WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("pill.html".into()))
        .title("wisperChad")
        .inner_size(w, h)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()?;
    #[cfg(target_os = "macos")]
    show_pill_everywhere(&win);
    // The work area leaves out the taskbar and the Dock, so the bar keeps the same gap above
    // either one, whatever its size, and comes down to the screen's edge when it's hidden.
    if let Ok(Some(mon)) = win.primary_monitor() {
        let area = mon.work_area();
        let scale = mon.scale_factor();
        let x = area.position.x + (area.size.width as i32 - (w * scale) as i32) / 2;
        let bottom = area.position.y + area.size.height as i32;
        let y = bottom - ((h + ABOVE_TASKBAR) * scale) as i32;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    Ok(())
}

// ---------------------------------------------------------------- hotkeys

/// If hotkeys are re-registered while a key is held, its "released" event never arrives,
/// so an in-progress recording would hang. Cancel it instead.
fn cancel_recording(app: &AppHandle) {
    let st = app.state::<AppState>();
    st.keys_down.lock().unwrap().clear();
    st.presses.lock().unwrap().reset();
    *st.capture.lock().unwrap() = None;
    *st.reading.lock().unwrap() = None;
    let mut rec = st.recording.lock().unwrap();
    if rec.take().is_some() {
        let _ = st.recorder.stop();
        speakers::unmute();
        pill(app, "hidden", None, "");
    }
}

fn register_hotkeys(app: &AppHandle) -> Result<(), String> {
    cancel_recording(app);
    let st = app.state::<AppState>();
    let _guard = st.hotkey_lock.lock().unwrap();
    let s = st.settings();
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let mut list = Vec::new();
    let mut modifier_only = Vec::new();
    let mut errors = Vec::new();
    for (combo, mode) in [
        (&s.dictate_hotkey, Mode::Dictate),
        (&s.reply_hotkey, Mode::Reply),
    ] {
        if let Some(keys) = chords::parse(combo) {
            modifier_only.push((keys, mode));
            continue;
        }
        match parse_shortcut(combo) {
            Ok(sc) => match gs.register(sc) {
                Ok(()) => list.push((sc, mode)),
                Err(e) if e.to_string().contains("already registered") => errors.push(format!(
                    "{combo} is already used by another app. Pick a different one in Settings."
                )),
                Err(e) => errors.push(format!("{combo}: {e}")),
            },
            Err(e) => errors.push(format!("{combo}: {e}")),
        }
    }
    *st.hotkeys.lock().unwrap() = list;
    if let Err(e) = chords::watch(modifier_only, false) {
        errors.push(e.to_string());
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Could not register hotkey(s): {}",
            errors.join("; ")
        ))
    }
}

/// The Settings box says Win where the shortcut parser says Super.
fn parse_shortcut(combo: &str) -> Result<Shortcut, String> {
    combo
        .replace("Win", "Super")
        .parse::<Shortcut>()
        .map_err(|e| e.to_string())
}

fn on_hotkey(app: &AppHandle, shortcut: &Shortcut, state: ShortcutState) {
    let st = app.state::<AppState>();
    let Some(mode) = st
        .hotkeys
        .lock()
        .unwrap()
        .iter()
        .find(|(s, _)| s == shortcut)
        .map(|(_, m)| *m)
    else {
        return;
    };
    match state {
        ShortcutState::Pressed => hotkey_down(app, mode),
        ShortcutState::Released => hotkey_up(app, mode),
    }
}

fn on_chord(app: &AppHandle, event: chords::Event) {
    match event {
        chords::Event::Pressed(mode) => hotkey_down(app, mode),
        chords::Event::Extended(mode) => hotkey_extended(app, mode),
        chords::Event::Released(mode) => hotkey_up(app, mode),
        chords::Event::Abandoned(mode) => hotkey_abandoned(app, mode),
        chords::Event::Captured(combo) => {
            let _ = app.emit("hotkey-chord", combo);
        }
    }
}

fn hotkey_down(app: &AppHandle, mode: Mode) {
    let st = app.state::<AppState>();
    // Holding a key fires Pressed repeatedly (auto-repeat); only react to the first one.
    {
        let mut down = st.keys_down.lock().unwrap();
        if down.contains(&mode) {
            return;
        }
        down.push(mode);
    }
    let recording = *st.recording.lock().unwrap();
    if recording.is_some_and(|m| m != mode) {
        return; // The other hotkey is recording.
    }
    if st.settings().hotkey_style == "toggle" {
        if recording == Some(mode) {
            finish_recording(app, mode); // second press stops
        } else {
            start_recording(app, mode);
        }
        return;
    }
    let action = st.presses.lock().unwrap().down(Instant::now());
    match action {
        presses::Action::Start => start_recording(app, mode),
        presses::Action::Finish => finish_recording(app, mode),
        presses::Action::Lock if recording == Some(mode) => show_listening(app, &st, mode, true),
        // A tap while the last recording was still being written: nothing to keep listening to.
        presses::Action::Lock => st.presses.lock().unwrap().reset(),
        _ => {}
    }
}

fn hotkey_up(app: &AppHandle, mode: Mode) {
    let st = app.state::<AppState>();
    st.keys_down.lock().unwrap().retain(|m| *m != mode);
    let recording = *st.recording.lock().unwrap();
    if st.settings().hotkey_style == "toggle" || recording.is_some_and(|m| m != mode) {
        return;
    }
    let action = st.presses.lock().unwrap().up(Instant::now());
    match action {
        presses::Action::Finish => finish_recording(app, mode),
        presses::Action::WaitForSecondPress(tap) => finish_unless_pressed_again(app, mode, tap),
        _ => {}
    }
}

fn finish_unless_pressed_again(app: &AppHandle, mode: Mode, tap: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(presses::SECOND_PRESS_WINDOW).await;
        let st = app.state::<AppState>();
        let missed = st.presses.lock().unwrap().second_press_missed(tap);
        if missed {
            finish_recording(&app, mode);
        }
    });
}

/// With hotkeys like Ctrl+Win and Ctrl+Win+Alt, pressing the longer one passes through the shorter
/// one. What that started becomes the longer one's, so nothing said so far is lost.
fn hotkey_extended(app: &AppHandle, mode: Mode) {
    let st = app.state::<AppState>();
    st.keys_down.lock().unwrap().clear();
    let mut rec = st.recording.lock().unwrap();
    if rec.is_none() || *rec == Some(mode) {
        drop(rec);
        hotkey_down(app, mode);
        return;
    }
    st.keys_down.lock().unwrap().push(mode);
    *st.capture.lock().unwrap() = None;
    *st.reading.lock().unwrap() = None;
    match prepare_recording(&st, mode) {
        Ok(()) => {
            *rec = Some(mode);
            start_reading(app, &st);
            show_listening(app, &st, mode, false);
        }
        Err(e) => {
            *rec = None;
            let _ = st.recorder.stop();
            pill(app, "error", Some(mode), e);
            hide_pill_later(app, 4000);
        }
    }
}

fn hotkey_abandoned(app: &AppHandle, mode: Mode) {
    let st = app.state::<AppState>();
    st.keys_down.lock().unwrap().retain(|m| *m != mode);
    let recording = *st.recording.lock().unwrap();
    if recording == Some(mode) {
        cancel_recording(app);
    }
}

fn start_recording(app: &AppHandle, mode: Mode) {
    let st = app.state::<AppState>();
    let mut rec = st.recording.lock().unwrap();
    if rec.is_some() || st.busy.load(Ordering::SeqCst) {
        return;
    }
    if let Err(e) = prepare_recording(&st, mode) {
        pill(app, "error", Some(mode), e);
        hide_pill_later(app, 4000);
        return;
    }
    let s = st.settings();
    match st.recorder.start(&s.microphone) {
        Ok(()) => {
            *rec = Some(mode);
            if s.mute_while_recording {
                speakers::mute();
            }
            start_reading(app, &st);
            show_listening(app, &st, mode, false);
        }
        Err(e) => {
            applog::write(&format!("Couldn't start recording: {e}"));
            pill(app, "error", Some(mode), e.to_string());
            hide_pill_later(app, 4000);
        }
    }
}

fn prepare_recording(st: &AppState, mode: Mode) -> Result<(), String> {
    preflight(st, mode)?;
    // Taken now, while the app being replied in is still in front and before the pill shows.
    if mode == Mode::Reply {
        if st.settings().reply_source == "screen" {
            let capture = screen::capture().map_err(|e| e.to_string())?;
            *st.capture.lock().unwrap() = Some(capture);
        } else {
            *st.reply_window.lock().unwrap() = screen::active_window();
        }
    }
    Ok(())
}

/// `locked` is a double-press, which listens until the next press whatever the hotkey style.
fn show_listening(app: &AppHandle, st: &AppState, mode: Mode, locked: bool) {
    let msg = match (mode, locked || st.settings().hotkey_style == "toggle") {
        (Mode::Reply, true) => "Listening for reply… press again to finish",
        (Mode::Reply, false) => "Listening… (reply)",
        (Mode::Dictate, true) => "Listening… press again to finish",
        (Mode::Dictate, false) => "Listening…",
    };
    // Setup's test has the user try a double-press as well as a hold, and needs to know which.
    if locked {
        let _ = app.emit("hotkey-locked", mode);
    }
    pill(app, "listening", Some(mode), msg);
}

fn finish_recording(app: &AppHandle, mode: Mode) {
    let st = app.state::<AppState>();
    let mut rec = st.recording.lock().unwrap();
    if *rec != Some(mode) {
        return;
    }
    *rec = None;
    speakers::unmute();
    st.presses.lock().unwrap().reset();
    st.busy.store(true, Ordering::SeqCst);
    let app_for_task = app.clone();
    let task = tauri::async_runtime::spawn(async move {
        let app = app_for_task;
        let result = run_pipeline(&app, mode).await;
        let st = app.state::<AppState>();
        st.busy.store(false, Ordering::SeqCst);
        match result {
            Ok(Some(msg)) => {
                pill(&app, "done", Some(mode), msg);
                // Under Wayland this says what to do next, so it needs reading.
                hide_pill_later(&app, if input::wayland() { 5000 } else { 1200 });
            }
            Ok(None) => pill(&app, "hidden", None, ""),
            Err(e) => {
                applog::write(&format!("{mode:?} failed: {e}"));
                pill(&app, "error", Some(mode), e.to_string());
                hide_pill_later(&app, 5000);
            }
        }
    });
    *st.pipeline.lock().unwrap() = Some(task);
}

/// The listening bar's cancel button: stops listening, or drops the dictation or reply being worked
/// on, so nothing is typed. What's already been sent to the model finishes on its own, unused.
#[tauri::command]
fn cancel(app: AppHandle) {
    let st = app.state::<AppState>();
    if let Some(task) = st.pipeline.lock().unwrap().take() {
        task.abort();
    }
    st.busy.store(false, Ordering::SeqCst);
    cancel_recording(&app);
    pill(&app, "hidden", None, "");
}

/// Checks that everything needed for `mode` is set up before we start recording.
fn preflight(st: &AppState, mode: Mode) -> Result<(), String> {
    let s = st.settings();
    let id = s
        .stt_model
        .ok_or("Pick a speech model in wisperChad first")?;
    let model = catalog::find_stt(&id).ok_or("Unknown speech model")?;
    if !model.is_installed(&st.models_dir) {
        return Err("Speech model not downloaded yet".into());
    }
    if mode == Mode::Reply && s.llm_model.is_none() {
        return Err("Replies need a writing model. Add one on the Models page.".into());
    }
    Ok(())
}

// ---------------------------------------------------------------- pipeline

async fn run_pipeline(app: &AppHandle, mode: Mode) -> anyhow::Result<Option<String>> {
    let st = app.state::<AppState>();
    let s = st.settings();
    let reading = st.reading.lock().unwrap().take();
    let reading = reading.or_else(|| {
        let capture = st.capture.lock().unwrap().take();
        capture.map(|capture| Reading::start(app, capture))
    });
    let recording = st.recorder.stop()?;
    let audio = audio::resample_to_16k(&recording.samples, recording.sample_rate);
    if audio.len() < (audio::WHISPER_RATE as usize) / 3 || audio::rms(&audio) < 0.0008 {
        return Ok(None); // too short or silent: do nothing
    }

    // Grab the context now, before we touch the clipboard ourselves. The last reply or dictation,
    // left there for pasting by hand, isn't a message to reply to.
    let context = if mode == Mode::Reply && reading.is_none() {
        input::read_clipboard()
            .filter(|copied| !input::is_our_paste(copied))
            .unwrap_or_default()
    } else {
        String::new()
    };
    if mode == Mode::Reply && reading.is_none() && context.trim().is_empty() {
        anyhow::bail!(
            "Copy the message you're replying to first ({}+C)",
            input::SHORTCUT_KEY
        );
    }

    pill(app, "transcribing", Some(mode), "Transcribing…");
    let model = catalog::find_stt(s.stt_model.as_deref().unwrap_or_default())
        .ok_or_else(|| anyhow::anyhow!("Unknown speech model"))?;
    let models_dir = st.models_dir.clone();
    let stt = st.stt.clone();
    let lang = s.language.clone();
    let english = lang == "en" || model.languages == "English";
    let transcript = tauri::async_runtime::spawn_blocking(move || {
        stt.transcribe(&model, &models_dir, &audio, &lang)
    })
    .await??;
    if transcript.is_empty() {
        return Ok(None);
    }
    // Numbers said in words are typed as digits, before clean-up, which keeps to what was said.
    let transcript = if english {
        numbers::to_digits(&transcript)
    } else {
        transcript
    };

    let output = match (mode, s.llm_model.as_deref()) {
        (Mode::Reply, Some(tag)) => {
            write_reply(app, &s, tag, reading, &context, &transcript).await?
        }
        (Mode::Dictate, Some(tag)) if s.cleanup_enabled || !s.write_language.is_empty() => {
            pill(app, "writing", Some(mode), "Cleaning up…");
            let p = prompts::dictation(&transcript, &s.write_language);
            let translated = !s.write_language.is_empty();
            match write(app, &s, tag, &p, &[]).await {
                // Only clean-up that kept to what was said: never an answer or a refusal.
                Ok(t)
                    if prompts::faithful_dictation(
                        prompts::unquoted(&t),
                        &transcript,
                        translated,
                    ) =>
                {
                    prompts::unquoted(&t).to_string()
                }
                _ => transcript.clone(), // LLM down, or it wrote something else? type what you said
            }
        }
        _ => transcript.clone(),
    };

    let _ = app.emit(
        "history",
        serde_json::json!({ "mode": mode, "heard": transcript, "output": output }),
    );
    let text = output.clone();
    let handle = app.clone();
    let submit = mode == Mode::Dictate && s.auto_submit;
    tauri::async_runtime::spawn_blocking(move || {
        input::wait_for_modifiers_released(Duration::from_secs(2));
        input::paste_text(&handle, &text)?;
        if submit {
            input::press_enter(&handle)?;
        }
        anyhow::Ok(())
    })
    .await??;
    Ok(Some(if input::wayland() {
        format!("Copied ✓ Press {}+V to paste", input::SHORTCUT_KEY)
    } else if mode == Mode::Reply {
        "Draft inserted ✓".into()
    } else {
        "Done ✓".into()
    }))
}

/// What reading the message gives: the message, who it's from, and the window it's in.
type Read = anyhow::Result<(reply::Found, screen::Active)>;

/// The message a reply is for, read while the user talks: the page and screenshot taken as the
/// reply hotkey went down, then the model's first step, finding the message, and its look at any
/// pictures in it. Started then, a reply had only its writing left once the user stopped talking.
/// Dropped before it's read, as when the reply is cancelled or nothing was said, it stops.
struct Reading(Option<tauri::async_runtime::JoinHandle<Read>>);

impl Reading {
    fn start(app: &AppHandle, capture: screen::Capture) -> Self {
        let app = app.clone();
        Self(Some(tauri::async_runtime::spawn(async move {
            read_message(&app, capture).await
        })))
    }

    async fn finish(mut self) -> Read {
        match self.0.take() {
            Some(task) => task.await?,
            None => anyhow::bail!("The message wasn't read"),
        }
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}

/// Starts reading the message, when the reply hotkey took a capture of the window.
fn start_reading(app: &AppHandle, st: &AppState) {
    let capture = st.capture.lock().unwrap().take();
    if let Some(capture) = capture {
        *st.reading.lock().unwrap() = Some(Reading::start(app, capture));
    }
}

/// Finds the message the user is replying to, in the page's text, or in the screenshot for apps
/// that share none, along with what any pictures in it show.
async fn read_message(app: &AppHandle, capture: screen::Capture) -> Read {
    let st = app.state::<AppState>();
    let s = st.settings();
    let tag = s.llm_model.clone().ok_or_else(|| {
        anyhow::anyhow!("Replies need a writing model. Add one on the Models page.")
    })?;
    let tag = tag.as_str();
    let wording = reply::ReplyPrompts::load(&st.data_dir).0;
    let seen = tauri::async_runtime::spawn_blocking(move || capture.finish()).await??;
    let (image, page, window) = (seen.image, reply::tidy_page(&seen.text), seen.window);
    let title = window.title.clone();
    let sees = reads_images(&s, tag).await?;
    let found = if reply::has_page_text(&page) {
        let ask = reply::find(&wording, &title, &page);
        let answer = write_at_most(app, &s, tag, &ask, &[], Some(FIND_TOKENS)).await?;
        let mut found =
            reply::Found::parse(&answer.text).checked(&page, &answer.text, &s.user_name);
        if answer.cut {
            found = found.read_on(&page);
        }
        // Without a graphics card, reading the screenshot for pictures was half of a reply's
        // wait, while the page's text already holds the message.
        if sees && hardware::has_gpu() {
            let ask = reply::pictures(&wording, &title, &found.message);
            let image = std::slice::from_ref(&image);
            let shown = write_at_most(app, &s, tag, &ask, image, Some(PICTURE_TOKENS)).await?;
            // Cut off, it was describing more than the message's pictures.
            if !shown.cut {
                found.add_pictures(&shown.text);
            }
        }
        found
    } else {
        if !sees {
            anyhow::bail!(
                "This app shares no text, and {tag} can't read screenshots. Use a model like Qwen3.5 4B, or set Reply to copied text in Settings."
            );
        }
        let ask = reply::find_on_screen(&wording, &title);
        let answer = write(app, &s, tag, &ask, std::slice::from_ref(&image)).await?;
        reply::Found::parse(&answer).or_page(&page, &answer)
    };
    Ok((found, window))
}

/// A reply in two steps: find the message the user is replying to, then write their reply to it.
/// See reply.rs.
async fn write_reply(
    app: &AppHandle,
    s: &Settings,
    tag: &str,
    reading: Option<Reading>,
    copied: &str,
    heard: &str,
) -> anyhow::Result<String> {
    let st = app.state::<AppState>();
    let wording = reply::ReplyPrompts::load(&st.data_dir).0;
    pill(app, "writing", Some(Mode::Reply), "Reading the message…");
    let (found, window) = match reading {
        Some(reading) => reading.finish().await?,
        None => {
            let window = st.reply_window.lock().unwrap().clone();
            (reply::Found::copied(copied), window)
        }
    };

    let language = reply_language(app, s, tag, heard).await;
    let note = prompts::reply_note(heard);
    let found = if checks::only_thanks(&note.text) {
        found.not_mine(&s.user_name)
    } else {
        found
    };
    pill(app, "writing", Some(Mode::Reply), "Writing reply…");
    let kind = reply::kind_of(&window.title, &window.app, &found.kind);
    let p = reply::write(
        &wording,
        kind,
        &found,
        &note,
        &s.tone,
        &language,
        &s.user_name,
    );
    let mut draft = write(app, s, tag, &p, &[]).await?;
    // A refusal is never pasted. Asked again, told plainly the note is fine, it writes it.
    for last_try in [false, true] {
        if !prompts::not_the_message(&draft, &note.text) {
            break;
        }
        draft = write(app, s, tag, &reply::insist(&wording, &p, last_try), &[]).await?;
    }
    // The user's rough note sent back as it was, slang and all, is asked for again, properly.
    if reply::echoes_note(&draft, &note.text) {
        let again = write(app, s, tag, &reply::without_note_echo(&wording, &p), &[]).await?;
        if !prompts::not_the_message(&again, &note.text) {
            draft = again;
        }
    }
    // A small model's slips, like the user's own news told back to them as theirs or the time they
    // offered turned down, are asked about again, pointedly, the most serious first. A try is kept
    // when it has fewer of them.
    let comment = kind == prompts::MessageKind::Comment;
    let as_draft = reply::write_draft(
        &wording,
        kind,
        &found,
        &note,
        &s.tone,
        &language,
        &s.user_name,
    );
    let check = |reply: &str| checks::problems(reply, &note.text, &found.message, comment);
    let mut slips = check(&draft);
    for _ in 0..FIX_TRIES {
        let Some(slip) = slips.first() else {
            break;
        };
        let asked = if slip.fix == checks::Fix::Flip {
            &as_draft
        } else {
            &p
        };
        let again = write(app, s, tag, &reply::fix(&wording, asked, slip), &[]).await?;
        if prompts::not_the_message(&again, &note.text) {
            continue;
        }
        let still = check(&again);
        if still.len() < slips.len() {
            draft = again;
            slips = still;
        }
    }
    // Their own lines handed back, like wishing them well in "your studies", go the same way.
    if let Some(copied) = reply::echo(&draft, &found.message, &note.text) {
        let again = write(
            app,
            s,
            tag,
            &reply::without_echo(&wording, &p, &copied),
            &[],
        )
        .await?;
        if !prompts::not_the_message(&again, &note.text) {
            draft = again;
        }
        // Asked again, it now and then copies the same line, which is then taken out.
        if let Some(copied) = reply::echo(&draft, &found.message, &note.text) {
            draft = reply::drop_echo(&draft, &copied, &s.user_name);
        }
    }
    // A link from the message, still there after asking again, comes out last, as asking about
    // anything else could bring it back. They have it already, even when it's the user's own, like
    // their portfolio in the email they're following up.
    if let Some(link) = checks::their_link(&draft, &found.message, &note.text) {
        draft = reply::without_link(&draft, &link, &s.user_name);
    }
    // So do lines only they'd say, like "Please contact us", with their sentences.
    while let Some(theirs) = checks::speaks_for_them(&draft, &note.text) {
        let without = reply::drop_echo(&draft, theirs, &s.user_name);
        if without == draft {
            break;
        }
        draft = without;
    }
    // And a note that only thanks them gets a reply that only thanks them.
    if checks::only_thanks(&note.text) {
        draft = reply::just_thanks(&draft, &found.from, &s.user_name);
    }
    if prompts::not_the_message(&draft, &note.text) {
        anyhow::bail!("The writing model wouldn't write that reply. Try saying it another way.");
    }
    let reply = prompts::finish_reply(&draft, kind, &s.user_name);
    Ok(match kind {
        // A follow-up to the user's own email greets who it went to.
        prompts::MessageKind::Email => {
            let them = if found.mine { &found.to } else { &found.from };
            let reply = prompts::fit_sign_off(&reply, &found.message, &s.tone);
            reply::greet(&reply, them, &s.user_name)
        }
        _ => reply,
    })
}

/// Runs a prompt on whichever engine the user has.
async fn write(
    app: &AppHandle,
    s: &Settings,
    tag: &str,
    prompt: &prompts::Prompt,
    images: &[String],
) -> anyhow::Result<String> {
    Ok(write_at_most(app, s, tag, prompt, images, None).await?.text)
}

/// `write`, with the model stopped after `most_tokens`. Ollama writes on to the end.
async fn write_at_most(
    app: &AppHandle,
    s: &Settings,
    tag: &str,
    prompt: &prompts::Prompt,
    images: &[String],
    most_tokens: Option<u32>,
) -> anyhow::Result<engine::Answer> {
    if s.llm_engine == OLLAMA {
        let thinking = is_thinking_model(tag);
        let keep = s.keep_alive_minutes;
        let text = llm::chat(&s.ollama_url, tag, prompt, keep, thinking, images).await?;
        return Ok(engine::Answer { text, cut: false });
    }
    let st = app.state::<AppState>();
    let model = catalog::find_llm(tag)
        .filter(|m| m.is_installed(&st.models_dir))
        .ok_or_else(|| anyhow::anyhow!("{tag} isn't installed. Install it on the Models page."))?;
    let keep = s.keep_alive_minutes;
    st.engine
        .chat(&model, &st.models_dir, prompt, images, keep, most_tokens)
        .await
}

/// The language a reply should come out in. To match what was said, it's worked out from the note,
/// falling back to the message's language if the model can't name it.
async fn reply_language(app: &AppHandle, s: &Settings, tag: &str, note: &str) -> prompts::Language {
    if let Some(language) = prompts::Language::from_setting(&s.reply_language) {
        return language;
    }
    write(app, s, tag, &prompts::detect_language(note), &[])
        .await
        .ok()
        .and_then(|answer| prompts::language_name(&answer))
        .map_or(prompts::Language::OfMessage, prompts::Language::Named)
}

async fn reads_images(s: &Settings, tag: &str) -> anyhow::Result<bool> {
    if s.llm_engine == OLLAMA {
        return llm::supports_vision(&s.ollama_url, tag).await;
    }
    Ok(catalog::find_llm(tag).is_some_and(|m| m.reads_images()))
}

/// People who already picked a model did so in Ollama. Everyone else starts on the built-in engine,
/// with nothing more to install, and setup offers Ollama if it finds it.
fn first_engine(s: &Settings) -> &'static str {
    if s.llm_model.is_some() {
        OLLAMA
    } else {
        BUILT_IN
    }
}

fn is_thinking_model(tag: &str) -> bool {
    catalog::llm_models()
        .iter()
        .find(|m| m.id == tag)
        .map(|m| m.thinking)
        .unwrap_or(tag.starts_with("qwen3"))
}

// ---------------------------------------------------------------- commands

#[derive(Serialize)]
struct SttEntry {
    #[serde(flatten)]
    model: catalog::SttModel,
    installed: bool,
}

#[derive(Serialize)]
struct LlmEntry {
    #[serde(flatten)]
    model: catalog::LlmModel,
    installed: bool,
}

#[derive(Serialize)]
struct Catalog {
    stt: Vec<SttEntry>,
    llm: Vec<LlmEntry>,
    featured: [&'static str; 4],
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: tauri::State<AppState>,
    settings: Settings,
) -> Result<(), String> {
    settings
        .save(&state.config_dir)
        .map_err(|e| e.to_string())?;
    let stt_changed = state.settings().stt_model != settings.stt_model;
    let engine_changed = state.settings().llm_engine != settings.llm_engine;
    let launch_changed = state.settings().launch_on_startup != settings.launch_on_startup;
    if launch_changed {
        set_launch_on_startup(&app, &settings.launch_on_startup)?;
    }
    *state.settings.lock().unwrap() = settings;
    if stt_changed {
        state.stt.unload();
    }
    if engine_changed {
        let engine = state.engine.clone();
        tauri::async_runtime::spawn(async move { engine.stop().await });
    }
    register_hotkeys(&app)
}

fn set_launch_on_startup(app: &AppHandle, setting: &str) -> Result<(), String> {
    // A Store app starts with Windows through its package's startup task, which the user turns on
    // in Windows Settings. The plugin's registry entry would have no effect there.
    if store_family().is_some() {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    if setting != "off" && running_from_a_temporary_copy() {
        let why = "Move wisperChad into your Applications folder and open it from there first. \
            Until then macOS runs a temporary copy that's gone after a restart, so it can't launch \
            on startup.";
        return Err(why.into());
    }
    let autostart = app.autolaunch();
    let changed = if setting == "off" {
        autostart.disable()
    } else {
        autostart.enable()
    };
    changed.map_err(|e| format!("Couldn't change launching on startup: {e}"))
}

/// macOS runs a downloaded app from a hidden copy until it's moved into Applications, and from the
/// disk image when it's opened there. Neither is there after a restart.
#[cfg(target_os = "macos")]
fn running_from_a_temporary_copy() -> bool {
    std::env::current_exe().is_ok_and(|exe| {
        let exe = exe.to_string_lossy();
        exe.contains("/AppTranslocation/") || exe.starts_with("/Volumes/")
    })
}

/// Whether the computer still has wisperChad down to start with it. Installing an update by hand
/// runs the old version's uninstaller, which takes it off on Windows.
#[cfg(windows)]
fn has_startup_entry(app: &AppHandle) -> bool {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
    };
    let name = &app.package_info().name;
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let places = [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE];
    places.into_iter().any(|root| {
        let mut size = 0u32;
        let status = unsafe {
            RegGetValueW(
                root,
                w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            )
        };
        status.is_ok()
    })
}

/// The launch agent has to name the app where it is now, so one written while macOS ran a
/// temporary copy, or before the app was moved, counts as missing.
#[cfg(target_os = "macos")]
fn has_startup_entry(app: &AppHandle) -> bool {
    let (Some(home), Ok(exe)) = (
        std::env::var_os("HOME"),
        std::env::current_exe().and_then(|exe| exe.canonicalize()),
    ) else {
        return false;
    };
    let agent = PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", app.package_info().name));
    std::fs::read_to_string(agent)
        .is_ok_and(|text| text.contains(&format!("<string>{}</string>", exe.display())))
}

#[cfg(target_os = "linux")]
fn has_startup_entry(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(true)
}

/// Called while the user is recording a new hotkey in Settings, so pressing the current
/// hotkey types into the box instead of starting dictation.
#[tauri::command]
fn pause_hotkeys(app: AppHandle) {
    cancel_recording(&app);
    let st = app.state::<AppState>();
    let _guard = st.hotkey_lock.lock().unwrap();
    let _ = app.global_shortcut().unregister_all();
    st.hotkeys.lock().unwrap().clear();
    let _ = chords::watch(vec![], true);
}

#[tauri::command]
fn resume_hotkeys(app: AppHandle) -> Result<(), String> {
    register_hotkeys(&app)
}

#[tauri::command]
fn get_catalog(state: tauri::State<AppState>) -> Catalog {
    Catalog {
        stt: catalog::stt_models()
            .into_iter()
            .map(|m| SttEntry {
                installed: m.is_installed(&state.models_dir),
                model: m,
            })
            .collect(),
        llm: catalog::llm_models()
            .into_iter()
            .map(|m| LlmEntry {
                installed: m.is_installed(&state.models_dir),
                model: m,
            })
            .collect(),
        featured: catalog::FEATURED,
    }
}

#[tauri::command]
fn get_hardware() -> hardware::Hardware {
    hardware::detect()
}

#[tauri::command]
fn uses_wayland() -> bool {
    input::wayland()
}

#[tauri::command]
fn list_microphones() -> Vec<String> {
    audio::microphones()
}

#[derive(Serialize)]
struct About {
    version: String,
    data_dir: PathBuf,
    log_dir: PathBuf,
    /// Replies use reply-prompts.json from the data folder, which is for tuning them.
    custom_reply_prompts: bool,
    /// Installed from the Microsoft Store, where Windows Settings controls launching on startup.
    store: bool,
}

#[tauri::command]
fn get_about(app: AppHandle) -> Result<About, String> {
    let path = app.path();
    let data_dir = path.app_data_dir().map_err(|e| e.to_string())?;
    Ok(About {
        version: app.package_info().version.to_string(),
        custom_reply_prompts: reply::ReplyPrompts::load(&data_dir).1,
        data_dir: real_folder(data_dir),
        log_dir: real_folder(path.app_log_dir().map_err(|e| e.to_string())?),
        store: store_family().is_some(),
    })
}

/// The Microsoft Store package's family name, read from the folder Windows installed it in:
/// "PhuaSiEn.wisperChad_0.11.25.0_x64__1ajj5bppa1wpr" is "PhuaSiEn.wisperChad_1ajj5bppa1wpr".
/// None for every other install.
fn store_family() -> Option<String> {
    package_family(&std::env::current_exe().ok()?)
}

fn package_family(exe: &Path) -> Option<String> {
    if !exe.to_string_lossy().contains("\\WindowsApps\\") {
        return None;
    }
    let folder = exe.parent()?.file_name()?.to_str()?;
    let (name_and_version, publisher) = folder.split_once("__")?;
    let name = name_and_version.split('_').next()?;
    Some(format!("{name}_{publisher}"))
}

/// Where a folder under AppData really is. Inside the Store package, Windows keeps the app's
/// AppData in the package's own folder, which File Explorer, outside the package, has to be sent to.
fn real_folder(folder: PathBuf) -> PathBuf {
    let (Some(family), Some(local)) = (store_family(), std::env::var_os("LOCALAPPDATA")) else {
        return folder;
    };
    let local = PathBuf::from(local);
    let cache = local.join("Packages").join(family).join("LocalCache");
    let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
    if let Some(rest) = roaming.and_then(|r| folder.strip_prefix(r).ok().map(PathBuf::from)) {
        return cache.join("Roaming").join(rest);
    }
    match folder.strip_prefix(&local) {
        Ok(rest) => cache.join("Local").join(rest),
        Err(_) => folder,
    }
}

/// The permissions setup asks for, and whether each is allowed yet.
#[tauri::command]
fn get_permissions() -> Vec<permissions::Permission> {
    permissions::list()
}

#[tauri::command]
fn request_permission(id: String) -> Result<(), String> {
    permissions::request(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_folder(app: AppHandle, which: String) -> Result<(), String> {
    let path = app.path();
    let folder = match which.as_str() {
        "data" => path.app_data_dir(),
        "log" => path.app_log_dir(),
        _ => return Err("Unknown folder".into()),
    }
    .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    open_with_system(real_folder(folder).as_os_str())
}

/// The web pages the app links to. Only these can be opened, so nothing in the page can send the
/// browser anywhere else.
const LINKS: [(&str, &str); 1] = [("donate", "https://currymunchy.github.io/wisperchad-app/#support")];

#[tauri::command]
fn open_link(which: String) -> Result<(), String> {
    let (_, url) = LINKS
        .iter()
        .find(|(name, _)| *name == which)
        .ok_or("Unknown link")?;
    open_with_system(std::ffi::OsStr::new(url))
}

/// Opens a folder in the file manager, or a web page in the default browser.
fn open_with_system(target: &std::ffi::OsStr) -> Result<(), String> {
    let opener = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener)
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_presets(languages: Vec<String>) -> Vec<presets::Preset> {
    presets::for_computer(&languages, &hardware::detect())
}

/// Downloads one file of a model. `finished` and `whole` put its progress in terms of the model.
async fn fetch(
    app: &AppHandle,
    id: &str,
    url: &str,
    dest: &Path,
    finished: u64,
    whole: Option<u64>,
) -> Result<(), String> {
    let app = app.clone();
    let id = id.to_string();
    download::download(url, dest, move |done, total| {
        let _ = app.emit(
            "model-download",
            serde_json::json!({
                "id": id,
                "done": finished + done,
                "total": whole.or(total),
            }),
        );
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn download_stt(app: AppHandle, id: String) -> Result<(), String> {
    let model = catalog::find_stt(&id).ok_or("Unknown model")?;
    let dir = model.dir(&app.state::<AppState>().models_dir);
    // Multi-file models report progress against the whole model, not each file.
    let whole_model =
        (model.files.len() > 1).then_some(model.requirements.disk_mb as u64 * 1_000_000);
    let mut finished = 0u64;
    for file in &model.files {
        let dest = dir.join(file.name);
        // Files that finished before an interrupted download don't need fetching again.
        if let Ok(meta) = std::fs::metadata(&dest) {
            finished += meta.len();
            continue;
        }
        // WISPERCHAD_MODEL_MIRROR lets people behind a firewall (or tests) use another host
        // that serves the same ggml-*.bin files.
        let url = match std::env::var("WISPERCHAD_MODEL_MIRROR") {
            Ok(m) if !m.trim().is_empty() && model.folder.is_empty() => {
                format!("{}/{}", m.trim_end_matches('/'), file.name)
            }
            _ => file.url.clone(),
        };
        fetch(&app, &id, &url, &dest, finished, whole_model).await?;
        finished += std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    }
    // First model downloaded? Select it automatically.
    let st = app.state::<AppState>();
    let mut s = st.settings();
    if s.stt_model.is_none() {
        s.stt_model = Some(id);
        s.save(&st.config_dir).map_err(|e| e.to_string())?;
        *st.settings.lock().unwrap() = s;
    }
    Ok(())
}

#[tauri::command]
fn delete_stt(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    let model = catalog::find_stt(&id).ok_or("Unknown model")?;
    state.stt.unload();
    let dir = model.dir(&state.models_dir);
    for file in &model.files {
        let path = dir.join(file.name);
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    if !model.folder.is_empty() {
        let _ = std::fs::remove_dir(&dir);
    }
    let mut s = state.settings();
    if s.stt_model.as_deref() == Some(id.as_str()) {
        s.stt_model = None;
        s.save(&state.config_dir).map_err(|e| e.to_string())?;
        *state.settings.lock().unwrap() = s;
    }
    Ok(())
}

#[tauri::command]
async fn ollama_status(app: AppHandle) -> serde_json::Value {
    let url = app.state::<AppState>().settings().ollama_url;
    let status = llm::status(&url).await;
    // Which catalog models are already pulled (handles "name" vs "name:latest").
    let catalog_installed: Vec<&str> = catalog::llm_models()
        .iter()
        .map(|m| m.id)
        .filter(|id| llm::is_installed(&status.installed, id))
        .collect();
    serde_json::json!({
        "running": status.running,
        "installed": status.installed,
        "sizes": status.sizes,
        "catalogInstalled": catalog_installed,
    })
}

/// Loads a writing model to see how much memory it really uses on this PC.
#[tauri::command]
async fn measure_llm(app: AppHandle, tag: String) -> Result<llm::Measured, String> {
    let url = app.state::<AppState>().settings().ollama_url;
    llm::measure(&url, &tag).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn install_llm(app: AppHandle, tag: String) -> Result<(), String> {
    let st = app.state::<AppState>();
    let s = st.settings();
    let ollama = s.llm_engine == OLLAMA;
    if ollama {
        let app2 = app.clone();
        let tag2 = tag.clone();
        llm::pull(&s.ollama_url, &tag, move |p| {
            let _ = app2.emit("llm-pull", serde_json::json!({ "id": tag2, "progress": p }));
        })
        .await
        .map_err(|e| e.to_string())?;
    } else {
        download_built_in(&app, &tag).await?;
    }
    // First model installed for this engine? Select it automatically.
    let selected = s.llm_model.as_deref().and_then(catalog::find_llm);
    let selected_works = ollama || selected.is_some_and(|m| m.is_installed(&st.models_dir));
    let mut s = st.settings();
    if s.llm_model.is_none() || !selected_works {
        s.llm_model = Some(tag.clone());
        s.save(&st.config_dir).map_err(|e| e.to_string())?;
        *st.settings.lock().unwrap() = s;
    }
    if !ollama {
        warm_up(&app, &tag).await;
    }
    Ok(())
}

/// Downloads the engine, if this is the first model, and a model's files, as one progress bar.
async fn download_built_in(app: &AppHandle, tag: &str) -> Result<(), String> {
    let st = app.state::<AppState>();
    let model = catalog::find_llm(tag)
        .filter(|m| !m.files.is_empty())
        .ok_or("This model needs Ollama. Switch to it in Settings.")?;
    let engine_bytes = if st.engine.is_installed() {
        0
    } else {
        engine::DOWNLOAD.bytes
    };
    let whole = Some(model.requirements.disk_mb as u64 * 1_000_000 + engine_bytes);
    if !st.engine.is_installed() {
        let archive = st.engine.archive_path();
        fetch(app, tag, engine::DOWNLOAD.url, &archive, 0, whole).await?;
        let engine = st.engine.clone();
        tauri::async_runtime::spawn_blocking(move || engine.unpack(&archive))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
    }
    let dir = model.dir(&st.models_dir);
    let mut finished = engine_bytes;
    for file in &model.files {
        let dest = dir.join(file.name);
        // Files that finished before an interrupted download don't need fetching again.
        if !dest.exists() {
            fetch(app, tag, &file.url, &dest, finished, whole).await?;
        }
        finished += std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    }
    Ok(())
}

/// A GPU compiles what the engine asks of it the first time, which adds up to a minute. Doing that
/// here spares the first real dictation and reply, and Windows keeps the result for next time.
async fn warm_up(app: &AppHandle, tag: &str) {
    let _ = app.emit(
        "llm-pull",
        serde_json::json!({
            "id": tag,
            "progress": { "status": "Getting the model ready, this can take a minute…" },
        }),
    );
    let s = app.state::<AppState>().settings();
    let hello = prompts::Prompt {
        system: "Reply with OK.".into(),
        user: "OK?".into(),
        slot: prompts::Slot::Reading,
    };
    if write(app, &s, tag, &hello, &[]).await.is_err() {
        return; // Too little memory right now, say. The first real use will explain.
    }
    if catalog::find_llm(tag).is_some_and(|m| m.reads_images()) {
        if let Ok(image) = screen::blank_image() {
            let _ = write(app, &s, tag, &hello, &[image]).await;
        }
    }
}

#[tauri::command]
async fn delete_llm(app: AppHandle, tag: String) -> Result<(), String> {
    let st = app.state::<AppState>();
    let mut s = st.settings();
    if s.llm_engine == OLLAMA {
        llm::delete(&s.ollama_url, &tag)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        let model = catalog::find_llm(&tag).ok_or("Unknown model")?;
        st.engine.stop().await;
        let dir = model.dir(&st.models_dir);
        for file in &model.files {
            let path = dir.join(file.name);
            if path.exists() {
                std::fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
        let _ = std::fs::remove_dir(&dir);
    }
    if s.llm_model.as_deref() == Some(tag.as_str()) {
        s.llm_model = None;
        s.save(&st.config_dir).map_err(|e| e.to_string())?;
        *st.settings.lock().unwrap() = s;
    }
    Ok(())
}

// ---------------------------------------------------------------- app

/// Brings the main window forward. Closing it only hid it, so when it was hidden the page plays its
/// opening screen again, as it does when the app starts. That's set going before the window shows,
/// so the app doesn't flash up first.
fn open_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if !w.is_visible().unwrap_or(true) {
            let _ = w.emit("app-opened", ());
        }
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Wayland has no global hotkeys, so people point a shortcut in their desktop's settings at
/// `wisperchad --dictate` or `wisperchad --reply` instead. Each launch starts or finishes listening.
#[cfg(target_os = "linux")]
fn listen_from_command_line(app: &AppHandle, args: &[String]) -> bool {
    let Some(mode) = requested_mode(args) else {
        return false;
    };
    if let Some(st) = app.try_state::<AppState>() {
        let recording = *st.recording.lock().unwrap();
        if recording == Some(mode) {
            finish_recording(app, mode);
        } else {
            start_recording(app, mode);
            if *st.recording.lock().unwrap() == Some(mode) {
                show_listening(app, &st, mode, true);
            }
        }
    }
    true
}

#[cfg(target_os = "linux")]
fn requested_mode(args: &[String]) -> Option<Mode> {
    args.iter().find_map(|arg| match arg.as_str() {
        "--dictate" => Some(Mode::Dictate),
        "--reply" => Some(Mode::Reply),
        _ => None,
    })
}

#[cfg(not(target_os = "linux"))]
fn listen_from_command_line(_app: &AppHandle, _args: &[String]) -> bool {
    false
}

pub fn run() {
    tauri::Builder::default()
        // A second launch just brings the existing window forward (hotkeys can only be owned once).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if !listen_from_command_line(app, &args) {
                open_main(app);
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| on_hotkey(app, shortcut, event.state()))
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_FLAG]),
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            applog::init(&app.path().app_log_dir()?);
            applog::write(&format!("Started version {}", app.package_info().version));
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let models_dir = data_dir.join("models");
            std::fs::create_dir_all(&models_dir)?;
            let mut settings = Settings::load(&config_dir);
            if settings.llm_engine.is_empty() {
                settings.llm_engine = first_engine(&settings).into();
            }
            // Anyone with a speech model from before setup existed has no need for it.
            if settings.setup_done.is_none() {
                settings.setup_done = Some(settings.stt_model.is_some());
            }
            let _ = settings.save(&config_dir);
            let stay_hidden = settings.launch_on_startup == "hidden"
                && std::env::args().any(|arg| arg == AUTOSTART_FLAG);
            let launch = settings.launch_on_startup.clone();
            app.manage(AppState {
                settings: Mutex::new(settings),
                config_dir,
                data_dir: data_dir.clone(),
                models_dir,
                recorder: audio::Recorder::spawn(),
                stt: Arc::new(stt::Stt::default()),
                engine: engine::Engine::new(&data_dir),
                hotkeys: Mutex::new(vec![]),
                recording: Mutex::new(None),
                keys_down: Mutex::new(vec![]),
                presses: Mutex::new(presses::Presses::default()),
                hotkey_lock: Mutex::new(()),
                busy: AtomicBool::new(false),
                pipeline: Mutex::new(None),
                capture: Mutex::new(None),
                reading: Mutex::new(None),
                reply_window: Mutex::new(screen::Active::default()),
            });
            // The setting only writes the startup entry when it changes, so an entry that's gone
            // missing since is put back here.
            if launch != "off" && !has_startup_entry(&handle) {
                if let Err(e) = set_launch_on_startup(&handle, &launch) {
                    applog::write(&e);
                }
            }

            create_pill_window(&handle)?;
            let chord_handle = handle.clone();
            chords::listen(move |event| on_chord(&chord_handle, event));
            if let Err(e) = register_hotkeys(&handle) {
                applog::write(&e);
            }

            let open = MenuItem::with_id(app, "open", "Open wisperChad", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("wisperChad")
                .menu(&menu)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => open_main(app),
                    "quit" => {
                        speakers::unmute();
                        std::thread::sleep(Duration::from_millis(100));
                        app.exit(0)
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if let (false, Some(main)) = (stay_hidden, app.get_webview_window("main")) {
                main.show()?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the main window keeps the app running in the tray so hotkeys still work.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            pause_hotkeys,
            resume_hotkeys,
            get_catalog,
            get_hardware,
            uses_wayland,
            get_presets,
            list_microphones,
            get_about,
            open_folder,
            open_link,
            get_permissions,
            request_permission,
            cancel,
            download_stt,
            delete_stt,
            ollama_status,
            measure_llm,
            install_llm,
            delete_llm,
        ])
        .build(tauri::generate_context!())
        .expect("error while running wisperChad")
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            match _event {
                // Clicking the Dock icon brings back a window that was closed to the menu bar.
                tauri::RunEvent::Reopen { .. } => open_main(_app),
                // Cmd+Q and the Dock's Quit don't go through the menu bar's Quit.
                tauri::RunEvent::Exit => {
                    speakers::unmute();
                    std::thread::sleep(Duration::from_millis(100));
                }
                _ => {}
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_ui_style_hotkeys_parse() {
        let d = Settings::default();
        for combo in [&d.dictate_hotkey, &d.reply_hotkey] {
            assert!(
                chords::parse(combo).is_some() || parse_shortcut(combo).is_ok(),
                "failed to parse {combo}"
            );
        }
        for combo in [
            "Ctrl+Shift+F9",
            "Alt+Shift+1",
            "Ctrl+Alt+Backquote",
            "Super+Alt+D",
            "Ctrl+Win+D",
            "F8",
        ] {
            assert!(parse_shortcut(combo).is_ok(), "failed to parse {combo}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_command_line_picks_what_to_listen_for() {
        let requested =
            |list: &[&str]| requested_mode(&list.iter().map(|a| a.to_string()).collect::<Vec<_>>());
        assert_eq!(requested(&["wisperchad", "--dictate"]), Some(Mode::Dictate));
        assert_eq!(requested(&["wisperchad", "--reply"]), Some(Mode::Reply));
        assert_eq!(requested(&["wisperchad"]), None);
    }

    // Windows paths only split into folders on Windows.
    #[cfg(windows)]
    #[test]
    fn the_store_package_is_found_from_its_folder() {
        let store = Path::new(
            "C:\\Program Files\\WindowsApps\\PhuaSiEn.wisperChad_0.11.25.0_x64__1ajj5bppa1wpr\\wisperchad.exe",
        );
        assert_eq!(
            package_family(store).as_deref(),
            Some("PhuaSiEn.wisperChad_1ajj5bppa1wpr")
        );
        let installer = Path::new("C:\\Users\\you\\AppData\\Local\\wisperChad\\wisperchad.exe");
        assert_eq!(package_family(installer), None);
    }
}
