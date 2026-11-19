//! Hotkeys made only of modifier keys, like Ctrl+Win. System hotkey APIs need a normal key in the
//! combination, so these are spotted by checking which keys are down, many times a second. A keyboard
//! hook won't do: it hears nothing while this app's own window is in front, which is where hotkeys
//! get set and tested. The check only runs while such a hotkey is set or Settings is waiting for one,
//! and it keeps nothing but which modifiers are down.
//!
//! Each system has its own `keys` module to read the keyboard. Win means Cmd on a Mac and Super on
//! Linux. Wayland doesn't let apps read the keyboard, so on Linux these hotkeys need X11.

use crate::Mode;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

const CTRL: u8 = 1;
const WIN: u8 = 2;
const ALT: u8 = 4;
const SHIFT: u8 = 8;
const NAMES: [(u8, &str); 4] = [(CTRL, "Ctrl"), (WIN, "Win"), (ALT, "Alt"), (SHIFT, "Shift")];

/// One key alone would go off during everyday shortcuts.
const MIN_KEYS: u32 = 2;

const CHECK_EVERY: Duration = Duration::from_millis(15);

static TRACKER: Mutex<Tracker> = Mutex::new(Tracker::new());
static EVENTS: OnceLock<Sender<Event>> = OnceLock::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Which run of `check_keys` is current. Any other one stops.
static RUN: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Pressed(Mode),
    /// More modifiers joined and made the other hotkey, as Ctrl+Win does on its way to Ctrl+Win+Alt.
    Extended(Mode),
    Released(Mode),
    /// Another key joined, so this was a shortcut like Ctrl+Win+Left rather than the hotkey.
    Abandoned(Mode),
    /// The combination Settings was waiting for.
    Captured(String),
}

/// The modifiers in a combination like "Ctrl+Win", or None if it has a normal key in it.
pub fn parse(combo: &str) -> Option<u8> {
    let mut keys = 0;
    for part in combo.split('+') {
        keys |= NAMES.iter().find(|(_, name)| *name == part)?.0;
    }
    (keys.count_ones() >= MIN_KEYS).then_some(keys)
}

fn name(keys: u8) -> String {
    NAMES
        .iter()
        .filter(|(key, _)| keys & key != 0)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join("+")
}

/// Hands events to `handler`, in order, on a thread of their own, because a reply takes a screenshot
/// as it starts and the keys still need checking meanwhile.
pub fn listen(handler: impl Fn(Event) + Send + 'static) {
    let (sender, events) = mpsc::channel();
    let _ = EVENTS.set(sender);
    std::thread::spawn(move || events.iter().for_each(handler));
}

/// Looks out for `chords`, and when `capturing`, for the next combination held down instead.
pub fn watch(chords: Vec<(u8, Mode)>, capturing: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        chords.is_empty() || keys::readable(),
        "Hotkeys like Ctrl+Super need an X11 session. Use one with a normal key instead"
    );
    let needed = (capturing || !chords.is_empty()) && keys::readable();
    TRACKER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .set(chords, capturing);
    run(needed);
    Ok(())
}

fn run(needed: bool) {
    if ACTIVE.swap(needed, Ordering::SeqCst) == needed {
        return;
    }
    let run = RUN.fetch_add(1, Ordering::SeqCst) + 1;
    if needed {
        std::thread::spawn(move || check_keys(run));
    }
}

fn check_keys(run: u64) {
    while RUN.load(Ordering::SeqCst) == run {
        let held = keys::modifiers_down();
        let other = held != 0 && keys::other_key_down();
        let (event, used_just_now) = {
            let mut tracker = TRACKER.lock().unwrap_or_else(|e| e.into_inner());
            let used_before = tracker.used;
            let event = match tracker.modifiers(held) {
                None if other => tracker.other_key(),
                event => event,
            };
            let extended = matches!(event, Some(Event::Extended(_)));
            (event, extended || (tracker.used && !used_before))
        };
        // The keys are down by now, so this lands before any of them comes up.
        if used_just_now {
            keys::mark_used();
        }
        if let (Some(event), Some(events)) = (event, EVENTS.get()) {
            let _ = events.send(event);
        }
        std::thread::sleep(CHECK_EVERY);
    }
}

struct Tracker {
    chords: Vec<(u8, Mode)>,
    capturing: bool,
    held: u8,
    /// Everything held since the modifiers were last all up, for capturing.
    peak: u8,
    active: Option<Mode>,
    /// Nothing counts until the modifiers have all come up.
    spoiled: bool,
    /// Letting go shouldn't open Start or a menu bar, because the keys were used for something.
    used: bool,
}

impl Tracker {
    const fn new() -> Self {
        Self {
            chords: Vec::new(),
            capturing: false,
            held: 0,
            peak: 0,
            active: None,
            spoiled: false,
            used: false,
        }
    }

    fn set(&mut self, chords: Vec<(u8, Mode)>, capturing: bool) {
        // Nothing checks the keys now, so nothing would notice these coming up.
        if chords.is_empty() && !capturing {
            self.held = 0;
        }
        self.chords = chords;
        self.capturing = capturing;
        self.peak = 0;
        self.active = None;
        self.spoiled = self.held != 0;
    }

    fn modifiers(&mut self, held: u8) -> Option<Event> {
        let before = std::mem::replace(&mut self.held, held);
        let released = before & !held != 0;
        let event = if held == before || self.spoiled {
            None
        } else if self.capturing {
            self.capture(held, released)
        } else {
            self.compare(held, released)
        };
        if held == 0 {
            self.peak = 0;
            self.spoiled = false;
            self.used = false;
        }
        event
    }

    fn other_key(&mut self) -> Option<Event> {
        self.spoiled = self.held != 0;
        self.active.take().map(Event::Abandoned)
    }

    fn compare(&mut self, held: u8, released: bool) -> Option<Event> {
        let found = self.chords.iter().find(|(keys, _)| *keys == held);
        match (self.active, found.map(|(_, mode)| *mode)) {
            (Some(mode), _) if released => {
                self.finish();
                Some(Event::Released(mode))
            }
            (Some(_), Some(mode)) => {
                self.active = Some(mode);
                Some(Event::Extended(mode))
            }
            (Some(mode), None) => {
                self.finish();
                Some(Event::Abandoned(mode))
            }
            (None, Some(mode)) if !released => {
                self.active = Some(mode);
                self.used = true;
                Some(Event::Pressed(mode))
            }
            (None, _) => None,
        }
    }

    fn finish(&mut self) {
        self.active = None;
        self.spoiled = true;
    }

    fn capture(&mut self, held: u8, released: bool) -> Option<Event> {
        self.peak |= held;
        self.used = self.peak.count_ones() >= MIN_KEYS;
        if released && self.used {
            self.spoiled = true;
            return Some(Event::Captured(name(self.peak)));
        }
        None
    }
}

#[cfg(windows)]
mod keys {
    use super::{ALT, CTRL, SHIFT, WIN};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
        VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
    };

    const MODIFIERS: [(VIRTUAL_KEY, u8); 8] = [
        (VK_LCONTROL, CTRL),
        (VK_RCONTROL, CTRL),
        (VK_LWIN, WIN),
        (VK_RWIN, WIN),
        (VK_LMENU, ALT),
        (VK_RMENU, ALT),
        (VK_LSHIFT, SHIFT),
        (VK_RSHIFT, SHIFT),
    ];

    /// No key has this code, so apps ignore it.
    const UNASSIGNED_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

    pub fn readable() -> bool {
        true
    }

    fn down(vk: u16) -> bool {
        unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
    }

    pub fn modifiers_down() -> u8 {
        MODIFIERS
            .iter()
            .filter(|(key, _)| down(key.0))
            .fold(0, |held, (_, bit)| held | bit)
    }

    /// Whether a normal key is down. Starts above the mouse buttons, which also have key codes.
    pub fn other_key_down() -> bool {
        let is_modifier = |vk: u16| {
            [VK_SHIFT, VK_CONTROL, VK_MENU, UNASSIGNED_KEY]
                .iter()
                .chain(MODIFIERS.iter().map(|(key, _)| key))
                .any(|key| key.0 == vk)
        };
        (0x08..0xFF).any(|vk| !is_modifier(vk) && down(vk))
    }

    /// Windows opens Start, or a window's menu bar, when Win or Alt comes up with nothing pressed
    /// since it went down. This counts as something pressed.
    pub fn mark_used() {
        let event = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: UNASSIGNED_KEY,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        let events = [event(KEYBD_EVENT_FLAGS(0)), event(KEYEVENTF_KEYUP)];
        unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };
    }
}

/// Reads the keyboard state macOS keeps for every app, which isn't one of the things its privacy
/// settings ask about. Hasn't been run on a real Mac.
#[cfg(target_os = "macos")]
pub(crate) mod keys {
    use super::{ALT, CTRL, SHIFT, WIN};

    /// The keys as the hardware has them, rather than as any one app has seen them.
    const HARDWARE_STATE: i32 = 1;

    const MODIFIERS: [(u64, u8); 4] = [
        (0x0004_0000, CTRL),
        (0x0010_0000, WIN),
        (0x0008_0000, ALT),
        (0x0002_0000, SHIFT),
    ];

    const LAST_KEY: u16 = 0x7E;
    /// Right Command through Fn, with Caps Lock among them.
    const MODIFIER_KEYS: std::ops::RangeInclusive<u16> = 54..=63;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceFlagsState(state: i32) -> u64;
        fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    }

    pub fn readable() -> bool {
        true
    }

    pub fn modifiers_down() -> u8 {
        let flags = unsafe { CGEventSourceFlagsState(HARDWARE_STATE) };
        MODIFIERS
            .iter()
            .filter(|(flag, _)| flags & flag != 0)
            .fold(0, |held, (_, bit)| held | bit)
    }

    pub fn other_key_down() -> bool {
        (0..=LAST_KEY).any(|key| {
            !MODIFIER_KEYS.contains(&key) && unsafe { CGEventSourceKeyState(HARDWARE_STATE, key) }
        })
    }

    pub fn mark_used() {}
}

/// X11 hands over a map of every key that's down, for any window. Wayland has nothing like it, and
/// what its X11 layer reports only covers windows of older apps, so it doesn't count.
#[cfg(target_os = "linux")]
pub(crate) mod keys {
    use super::{ALT, CTRL, SHIFT, WIN};
    use std::sync::OnceLock;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;
    use x11rb::rust_connection::RustConnection;

    /// X11's names for these keys, each with a left and a right.
    const MODIFIERS: [(u32, u8); 8] = [
        (0xFFE3, CTRL),
        (0xFFE4, CTRL),
        (0xFFEB, WIN),
        (0xFFEC, WIN),
        (0xFFE9, ALT),
        (0xFFEA, ALT),
        (0xFFE1, SHIFT),
        (0xFFE2, SHIFT),
    ];

    struct Keyboard {
        connection: RustConnection,
        /// Which modifier each key code is, 0 for a normal key.
        modifiers: [u8; 256],
    }

    fn keyboard() -> Option<&'static Keyboard> {
        static KEYBOARD: OnceLock<Option<Keyboard>> = OnceLock::new();
        KEYBOARD.get_or_init(connect).as_ref()
    }

    fn connect() -> Option<Keyboard> {
        let (connection, _) = x11rb::connect(None).ok()?;
        let first = connection.setup().min_keycode;
        let count = connection.setup().max_keycode - first + 1;
        let layout = connection
            .get_keyboard_mapping(first, count)
            .ok()?
            .reply()
            .ok()?;
        let modifiers = modifier_keys(first, layout.keysyms_per_keycode, &layout.keysyms);
        Some(Keyboard {
            connection,
            modifiers,
        })
    }

    /// `layout` lists what each key types, `per_key` entries each, starting at key code `first`.
    fn modifier_keys(first: u8, per_key: u8, layout: &[u32]) -> [u8; 256] {
        let mut modifiers = [0; 256];
        for (key, symbols) in layout.chunks(per_key.max(1).into()).enumerate() {
            if let Some(modifier) = modifiers.get_mut(first as usize + key) {
                *modifier = MODIFIERS
                    .iter()
                    .filter(|(symbol, _)| symbols.contains(symbol))
                    .fold(0, |held, (_, bit)| held | bit);
            }
        }
        modifiers
    }

    /// The key codes in X11's map, which has one bit for each.
    fn codes_down(map: &[u8; 32]) -> impl Iterator<Item = usize> + '_ {
        (0..256).filter(|code| map[code / 8] & (1 << (code % 8)) != 0)
    }

    fn held(modifiers: &[u8; 256], map: &[u8; 32]) -> u8 {
        codes_down(map).fold(0, |held, code| held | modifiers[code])
    }

    fn any_normal_key(modifiers: &[u8; 256], map: &[u8; 32]) -> bool {
        codes_down(map).any(|code| modifiers[code] == 0)
    }

    fn read() -> Option<(&'static Keyboard, [u8; 32])> {
        let keyboard = keyboard()?;
        let map = keyboard.connection.query_keymap().ok()?.reply().ok()?;
        Some((keyboard, map.keys))
    }

    pub fn readable() -> bool {
        !crate::input::wayland() && keyboard().is_some()
    }

    pub fn modifiers_down() -> u8 {
        read().map_or(0, |(keyboard, map)| held(&keyboard.modifiers, &map))
    }

    pub fn other_key_down() -> bool {
        read().is_some_and(|(keyboard, map)| any_normal_key(&keyboard.modifiers, &map))
    }

    pub fn mark_used() {}

    #[cfg(test)]
    mod tests {
        use super::*;

        const NO_SYMBOL: u32 = 0;
        const LETTER_A: u32 = 0x61;
        const SHIFT_LEFT: u32 = 0xFFE1;
        const SUPER_LEFT: u32 = 0xFFEB;
        /// Control_R, as it comes on a layout that has one key type two things.
        const LAYOUT: [u32; 8] = [
            LETTER_A, 0x41, SHIFT_LEFT, NO_SYMBOL, 0xFFE4, NO_SYMBOL, SUPER_LEFT, NO_SYMBOL,
        ];

        fn map_with(codes: &[usize]) -> [u8; 32] {
            let mut map = [0; 32];
            for code in codes {
                map[code / 8] |= 1 << (code % 8);
            }
            map
        }

        #[test]
        fn modifiers_are_found_in_the_layout() {
            let modifiers = modifier_keys(8, 2, &LAYOUT);
            assert_eq!(&modifiers[8..12], &[0, SHIFT, CTRL, WIN]);
            assert_eq!(modifiers[7], 0);
            assert_eq!(modifiers[12], 0);
        }

        #[test]
        fn a_layout_that_runs_past_the_last_key_code_is_cut_short() {
            let modifiers = modifier_keys(254, 2, &LAYOUT);
            assert_eq!(modifiers[255], SHIFT);
        }

        #[test]
        fn the_map_is_read_key_by_key() {
            let modifiers = modifier_keys(8, 2, &LAYOUT);
            assert_eq!(held(&modifiers, &map_with(&[9, 11])), SHIFT | WIN);
            assert!(!any_normal_key(&modifiers, &map_with(&[9, 11])));
            assert!(any_normal_key(&modifiers, &map_with(&[8, 11])));
            assert_eq!(held(&modifiers, &map_with(&[])), 0);
        }

        #[test]
        fn the_highest_key_code_is_in_range() {
            assert_eq!(codes_down(&map_with(&[255])).collect::<Vec<_>>(), [255]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DICTATE: u8 = CTRL | WIN;
    const REPLY: u8 = CTRL | WIN | ALT;

    fn tracker() -> Tracker {
        let mut tracker = Tracker::new();
        tracker.set(vec![(DICTATE, Mode::Dictate), (REPLY, Mode::Reply)], false);
        tracker
    }

    fn capturing() -> Tracker {
        let mut tracker = Tracker::new();
        tracker.set(vec![], true);
        tracker
    }

    #[test]
    fn reads_combinations_made_only_of_modifiers() {
        assert_eq!(parse("Ctrl+Win"), Some(DICTATE));
        assert_eq!(parse("Ctrl+Win+Alt"), Some(REPLY));
        assert_eq!(parse("Ctrl"), None);
        assert_eq!(parse("Ctrl+Alt+Space"), None);
        assert_eq!(parse("F8"), None);
        assert_eq!(name(REPLY | SHIFT), "Ctrl+Win+Alt+Shift");
    }

    #[test]
    fn holding_then_letting_go_is_one_press_and_one_release() {
        let mut t = tracker();
        assert_eq!(t.modifiers(CTRL), None);
        assert_eq!(t.modifiers(DICTATE), Some(Event::Pressed(Mode::Dictate)));
        assert_eq!(t.modifiers(DICTATE), None); // auto-repeat
        assert_eq!(t.modifiers(WIN), Some(Event::Released(Mode::Dictate)));
        assert_eq!(t.modifiers(0), None);
    }

    #[test]
    fn the_longer_hotkey_takes_over_from_the_shorter_one() {
        let mut t = tracker();
        t.modifiers(CTRL);
        assert_eq!(t.modifiers(DICTATE), Some(Event::Pressed(Mode::Dictate)));
        assert_eq!(t.modifiers(REPLY), Some(Event::Extended(Mode::Reply)));
        assert_eq!(t.modifiers(DICTATE), Some(Event::Released(Mode::Reply)));
    }

    #[test]
    fn the_longer_hotkey_can_arrive_without_passing_the_shorter_one() {
        let mut t = tracker();
        t.modifiers(CTRL);
        t.modifiers(CTRL | ALT);
        assert_eq!(t.modifiers(REPLY), Some(Event::Pressed(Mode::Reply)));
    }

    #[test]
    fn letting_go_of_the_longer_hotkey_never_starts_the_shorter_one() {
        let mut t = tracker();
        t.modifiers(REPLY);
        assert_eq!(t.modifiers(DICTATE), Some(Event::Released(Mode::Reply)));
        assert_eq!(t.modifiers(CTRL), None);
        assert_eq!(t.modifiers(0), None);
        t.modifiers(CTRL);
        assert_eq!(t.modifiers(DICTATE), Some(Event::Pressed(Mode::Dictate)));
    }

    #[test]
    fn coming_down_to_a_hotkey_does_not_press_it() {
        let mut t = tracker();
        t.modifiers(DICTATE | SHIFT);
        assert_eq!(t.modifiers(DICTATE), None);
    }

    #[test]
    fn another_key_abandons_the_hotkey_until_everything_is_up() {
        let mut t = tracker();
        t.modifiers(DICTATE);
        assert_eq!(t.other_key(), Some(Event::Abandoned(Mode::Dictate)));
        assert_eq!(t.modifiers(WIN), None);
        assert_eq!(t.modifiers(DICTATE), None);
        t.modifiers(0);
        assert_eq!(t.modifiers(DICTATE), Some(Event::Pressed(Mode::Dictate)));
    }

    #[test]
    fn another_modifier_abandons_the_hotkey() {
        let mut t = tracker();
        t.modifiers(DICTATE);
        assert_eq!(
            t.modifiers(DICTATE | SHIFT),
            Some(Event::Abandoned(Mode::Dictate))
        );
    }

    #[test]
    fn a_shortcut_in_progress_does_not_turn_into_a_hotkey() {
        let mut t = tracker();
        t.modifiers(CTRL);
        assert_eq!(t.other_key(), None);
        assert_eq!(t.modifiers(DICTATE), None);
    }

    #[test]
    fn plain_typing_changes_nothing() {
        let mut t = tracker();
        assert_eq!(t.other_key(), None);
        t.modifiers(CTRL);
        assert_eq!(t.modifiers(DICTATE), Some(Event::Pressed(Mode::Dictate)));
    }

    #[test]
    fn keys_only_count_as_used_once_a_hotkey_goes_off() {
        let mut t = tracker();
        t.modifiers(WIN);
        assert!(!t.used);
        t.modifiers(DICTATE);
        assert!(t.used);
        t.modifiers(0);
        assert!(!t.used);
    }

    #[test]
    fn capturing_takes_everything_held_at_the_first_release() {
        let mut t = capturing();
        t.modifiers(CTRL);
        t.modifiers(DICTATE);
        assert!(t.used);
        t.modifiers(REPLY);
        assert_eq!(
            t.modifiers(WIN | ALT),
            Some(Event::Captured("Ctrl+Win+Alt".into()))
        );
        assert_eq!(t.modifiers(ALT), None);
        assert_eq!(t.modifiers(0), None);
    }

    #[test]
    fn capturing_skips_one_key_alone_and_keys_with_a_normal_key() {
        let mut t = capturing();
        t.modifiers(CTRL);
        assert_eq!(t.modifiers(0), None);
        t.modifiers(CTRL);
        t.modifiers(DICTATE);
        t.other_key();
        assert_eq!(t.modifiers(CTRL), None);
    }

    #[test]
    fn new_hotkeys_ignore_keys_still_held_from_setting_them() {
        let mut t = capturing();
        t.modifiers(REPLY);
        t.modifiers(DICTATE);
        t.set(vec![(CTRL | ALT, Mode::Dictate)], false);
        assert_eq!(t.modifiers(CTRL), None);
        assert_eq!(t.modifiers(CTRL | ALT), None);
        t.modifiers(0);
        t.modifiers(CTRL);
        assert_eq!(t.modifiers(CTRL | ALT), Some(Event::Pressed(Mode::Dictate)));
    }
}
