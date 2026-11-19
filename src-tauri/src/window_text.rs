//! The text of the active window, read through the accessibility system that screen readers use:
//! UI Automation on Windows, the Accessibility API on macOS. It reaches text scrolled out of view,
//! which a screenshot can't, but not pictures, so replies send both. Like screenshots it stays in
//! memory and goes only to the model on this computer.

use std::sync::mpsc::{self, Receiver};

/// Put into the page's text where the user is typing, to show the model which post or message
/// they mean. The page around it can be a whole feed.
pub const CURSOR_MARK: &str = "[THE USER IS TYPING HERE]";

/// Counted in bytes, which follow a model's token count across languages better than characters
/// do. Alongside a screenshot this leaves the context window room for the reply.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
const MAX_BYTES: usize = 8000;

/// Reads on its own thread, so a slow app can't hold up the hotkey. Sends an empty string if the
/// app doesn't share its text, and always on Linux, where replies go by the screenshot.
pub fn read_active() -> Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        #[cfg(any(windows, target_os = "macos"))]
        let text = accessibility::active_window_text();
        #[cfg(not(any(windows, target_os = "macos")))]
        let text = String::new();
        let _ = sender.send(text);
    });
    receiver
}

/// Fits text that meets at the user's view into `limit` bytes, half from each side, with `join`
/// between them. A short side gives its room to the other.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn keep_around(before: &str, after: &str, limit: usize, join: &str) -> String {
    let from_after = after.len().min(limit - before.len().min(limit / 2));
    let from_before = before.len().min(limit - from_after);
    let mut start = before.len() - from_before;
    while !before.is_char_boundary(start) {
        start += 1;
    }
    let mut end = from_after;
    while !after.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{join}{}", &before[start..], &after[..end])
}

#[cfg(windows)]
mod accessibility {
    use super::{keep_around, CURSOR_MARK, MAX_BYTES};
    use std::mem::ManuallyDrop;
    use std::time::Duration;
    use windows::core::Result;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
        TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TreeScope_Descendants,
        UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_TextPatternId,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    const ATTEMPTS: u32 = 14;
    const RETRY: Duration = Duration::from_millis(150);

    pub fn active_window_text() -> String {
        // Browsers only build this text once something asks for it, so the first reads can come
        // back empty or partial. Two matching reads in a row mean it has settled.
        let mut text = String::new();
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let window = GetForegroundWindow();
            for _ in 0..ATTEMPTS {
                let read = read_window(window).unwrap_or_default();
                let settled = !read.is_empty() && read == text;
                text = read;
                if settled {
                    break;
                }
                std::thread::sleep(RETRY);
            }
            CoUninitialize();
        }
        text
    }

    fn read_window(window: HWND) -> Result<String> {
        unsafe {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let documents = automation.ElementFromHandle(window)?.FindAll(
                TreeScope_Descendants,
                &automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &document_type())?,
            )?;
            // Frames count as documents too, and the page around them already holds their text.
            let mut text = String::new();
            let mut page = None;
            for i in 0..documents.Length()? {
                let Ok((pattern, found)) = read_document(&documents.GetElement(i)?) else {
                    continue;
                };
                if found.len() > text.len() {
                    text = found;
                    page = Some(pattern);
                }
            }
            let Some(page) = page else {
                return Ok(text);
            };
            if let Ok((before, after)) = split_at_cursor(&automation, &page) {
                let mark = format!("\n{CURSOR_MARK}\n");
                return Ok(keep_around(&before, &after, MAX_BYTES, &mark));
            }
            if text.len() <= MAX_BYTES {
                return Ok(text);
            }
            Ok(match split_at_view(&page) {
                Ok((before, after)) => keep_around(&before, &after, MAX_BYTES, ""),
                Err(_) => keep_around("", &text, MAX_BYTES, ""),
            })
        }
    }

    fn read_document(
        document: &IUIAutomationElement,
    ) -> Result<(IUIAutomationTextPattern, String)> {
        unsafe {
            let pattern: IUIAutomationTextPattern =
                document.GetCurrentPatternAs(UIA_TextPatternId)?;
            let text = pattern.DocumentRange()?.GetText(-1)?.to_string();
            Ok((pattern, text))
        }
    }

    /// The page's text before and after the box the user clicked into. Fails if that isn't in the
    /// page, like a pop-up reply window.
    fn split_at_cursor(
        automation: &IUIAutomation,
        page: &IUIAutomationTextPattern,
    ) -> Result<(String, String)> {
        unsafe {
            let cursor = page.RangeFromChild(&automation.GetFocusedElement()?)?;
            let before = page.DocumentRange()?;
            before.MoveEndpointByRange(
                TextPatternRangeEndpoint_End,
                &cursor,
                TextPatternRangeEndpoint_Start,
            )?;
            let after = page.DocumentRange()?;
            after.MoveEndpointByRange(
                TextPatternRangeEndpoint_Start,
                &cursor,
                TextPatternRangeEndpoint_End,
            )?;
            Ok((
                before.GetText(-1)?.to_string(),
                after.GetText(-1)?.to_string(),
            ))
        }
    }

    /// The page's text before and after the middle line on screen. That line is most likely in
    /// the open message, because sidebars have fewer lines.
    fn split_at_view(page: &IUIAutomationTextPattern) -> Result<(String, String)> {
        unsafe {
            let lines = page.GetVisibleRanges()?;
            let middle = lines.GetElement(lines.Length()? / 2)?;
            let before = page.DocumentRange()?;
            before.MoveEndpointByRange(
                TextPatternRangeEndpoint_End,
                &middle,
                TextPatternRangeEndpoint_Start,
            )?;
            let after = page.DocumentRange()?;
            after.MoveEndpointByRange(
                TextPatternRangeEndpoint_Start,
                &middle,
                TextPatternRangeEndpoint_Start,
            )?;
            Ok((
                before.GetText(-1)?.to_string(),
                after.GetText(-1)?.to_string(),
            ))
        }
    }

    fn document_type() -> VARIANT {
        VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_I4,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 {
                        lVal: UIA_DocumentControlTypeId.0,
                    },
                }),
            },
        }
    }
}

/// On macOS, web pages through the Accessibility API, which wisperChad is already allowed to use for
/// pasting: Safari's, Chrome's and Brave's, an email in Mail, and apps built on web pages like
/// Slack. They give a page's text through text markers, as they do to VoiceOver. Other apps' windows
/// go by the screenshot, as on Windows, where only documents are read.
#[cfg(target_os = "macos")]
mod accessibility {
    use super::{keep_around, CURSOR_MARK, MAX_BYTES};
    use std::collections::VecDeque;
    use std::ffi::{c_char, c_void, CStr, CString};
    use std::time::Duration;

    type CFTypeRef = *const c_void;
    type CFIndex = isize;
    type AXError = i32;
    type CopyMarker = unsafe extern "C" fn(CFTypeRef) -> CFTypeRef;
    type MakeRange = unsafe extern "C" fn(CFTypeRef, CFTypeRef, CFTypeRef) -> CFTypeRef;
    type TypeId = unsafe extern "C" fn() -> usize;

    const AX_SUCCESS: AXError = 0;
    const UTF8: u32 = 0x0800_0100;

    /// As on Windows: browsers build a page's text only once something asks for it, so the first
    /// reads can come back empty or partial. Two matching reads in a row mean it has settled.
    const ATTEMPTS: u32 = 14;
    const RETRY: Duration = Duration::from_millis(150);
    /// Reads that found no page before giving up on one: an app like Finder has none to find.
    const READS_WITHOUT_A_PAGE: u32 = 6;
    /// How long an app has to answer each question before it's given up on.
    const ANSWER_SECONDS: f32 = 1.0;
    /// Elements looked through for the page, past a window's toolbars and tabs.
    const MOST_ELEMENTS: usize = 1000;
    /// Steps up from the box the user is typing in to the page around it.
    const MOST_PARENTS: usize = 80;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXUIElementCreateSystemWide() -> CFTypeRef;
        fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> AXError;
        fn AXUIElementCopyAttributeValue(
            element: CFTypeRef,
            attribute: CFTypeRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementCopyParameterizedAttributeValue(
            element: CFTypeRef,
            attribute: CFTypeRef,
            parameter: CFTypeRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementSetAttributeValue(
            element: CFTypeRef,
            attribute: CFTypeRef,
            value: CFTypeRef,
        ) -> AXError;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: CFTypeRef;
        fn CFRetain(value: CFTypeRef) -> CFTypeRef;
        fn CFRelease(value: CFTypeRef);
        fn CFEqual(a: CFTypeRef, b: CFTypeRef) -> u8;
        fn CFGetTypeID(value: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFArrayGetTypeID() -> usize;
        fn CFStringCreateWithCString(
            allocator: CFTypeRef,
            text: *const c_char,
            encoding: u32,
        ) -> CFTypeRef;
        fn CFStringGetLength(text: CFTypeRef) -> CFIndex;
        fn CFStringGetMaximumSizeForEncoding(length: CFIndex, encoding: u32) -> CFIndex;
        fn CFStringGetCString(
            text: CFTypeRef,
            buffer: *mut c_char,
            size: CFIndex,
            encoding: u32,
        ) -> u8;
        fn CFArrayGetCount(array: CFTypeRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: CFTypeRef, index: CFIndex) -> CFTypeRef;
    }

    extern "C" {
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    /// A Core Foundation value held here, let go of when dropped.
    struct Owned(CFTypeRef);

    impl Owned {
        /// Takes over a value from a function with Copy or Create in its name. None for none.
        fn take(value: CFTypeRef) -> Option<Self> {
            (!value.is_null()).then_some(Self(value))
        }

        /// Holds on to a value borrowed from another, like an item in an array.
        fn retain(value: CFTypeRef) -> Option<Self> {
            (!value.is_null()).then(|| Self(unsafe { CFRetain(value) }))
        }

        fn get(&self) -> CFTypeRef {
            self.0
        }
    }

    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CFRelease(self.0) }
        }
    }

    pub fn active_window_text() -> String {
        // Without the Accessibility permission, which pasting needs too, apps share nothing, and
        // the reply goes by the screenshot.
        if !unsafe { AXIsProcessTrusted() } {
            return String::new();
        }
        let mut text = String::new();
        for read in 1..=ATTEMPTS {
            let now = read_front_window().unwrap_or_default();
            let settled = !now.is_empty() && now == text;
            text = now;
            if settled || (text.is_empty() && read >= READS_WITHOUT_A_PAGE) {
                break;
            }
            std::thread::sleep(RETRY);
        }
        text
    }

    /// The text of the largest page in the window in front, with the mark where the user is
    /// typing when that's in the page.
    fn read_front_window() -> Option<String> {
        let system = Owned::take(unsafe { AXUIElementCreateSystemWide() })?;
        unsafe { AXUIElementSetMessagingTimeout(system.get(), ANSWER_SECONDS) };
        let app = attribute(system.get(), "AXFocusedApplication")?;
        // Chrome, Brave, Edge and apps built on Electron build their pages' text only once asked
        // to. Others say they don't know what this is, which is fine.
        if let Some(name) = cf_string("AXManualAccessibility") {
            unsafe { AXUIElementSetAttributeValue(app.get(), name.get(), kCFBooleanTrue) };
        }
        let window = attribute(app.get(), "AXFocusedWindow")?;
        let (page, whole, text) = pages(window.get())
            .into_iter()
            .filter_map(|page| {
                let (whole, text) = page_text(page.get())?;
                Some((page, whole, text))
            })
            .max_by_key(|(_, _, text)| text.len())?;
        let typing = attribute(app.get(), "AXFocusedUIElement");
        let split =
            typing.and_then(|typing| split_at_cursor(page.get(), whole.get(), typing.get()));
        Some(match split {
            Some((before, after)) => {
                let mark = format!("\n{CURSOR_MARK}\n");
                keep_around(&before, &after, MAX_BYTES, &mark)
            }
            None => keep_around("", &text, MAX_BYTES, ""),
        })
    }

    /// The web pages in a window. Not looked inside, as a frame's text is in the page around it.
    fn pages(window: CFTypeRef) -> Vec<Owned> {
        let mut found = Vec::new();
        let mut queue: VecDeque<Owned> = children(window).into();
        let mut seen = 0;
        while let Some(element) = queue.pop_front() {
            seen += 1;
            if seen > MOST_ELEMENTS {
                break;
            }
            if role(element.get()) == "AXWebArea" {
                found.push(element);
            } else {
                queue.extend(children(element.get()));
            }
        }
        found
    }

    /// All of a page's text, as VoiceOver reads it, and the range it spans.
    fn page_text(page: CFTypeRef) -> Option<(Owned, String)> {
        let whole = ask(page, "AXTextMarkerRangeForUIElement", page)?;
        let text = ask(page, "AXStringForTextMarkerRange", whole.get())?;
        Some((whole, read_string(text.get())?))
    }

    /// The page's text before and after the box the user is typing in. None when that isn't in
    /// the page, like a reply window of its own, or is the whole page, like an email being written
    /// in Mail.
    fn split_at_cursor(
        page: CFTypeRef,
        whole: CFTypeRef,
        typing: CFTypeRef,
    ) -> Option<(String, String)> {
        if same(typing, page) || !inside(typing, page) {
            return None;
        }
        let markers = Markers::find()?;
        let typing = ask(page, "AXTextMarkerRangeForUIElement", typing)?;
        let before = markers.range(&markers.start(whole)?, &markers.start(typing.get())?)?;
        let after = markers.range(&markers.end(typing.get())?, &markers.end(whole)?)?;
        let text =
            |range: Owned| read_string(ask(page, "AXStringForTextMarkerRange", range.get())?.get());
        Some((text(before)?, text(after)?))
    }

    /// Whether `element` is somewhere in `page`.
    fn inside(element: CFTypeRef, page: CFTypeRef) -> bool {
        let mut parent = attribute(element, "AXParent");
        for _ in 0..MOST_PARENTS {
            let Some(up) = parent else {
                return false;
            };
            if same(up.get(), page) {
                return true;
            }
            parent = attribute(up.get(), "AXParent");
        }
        false
    }

    /// HIServices' functions for taking a text marker range apart and making one, looked up as the
    /// app runs: older SDKs leave them out of their headers. Without them there's no mark.
    struct Markers {
        start: CopyMarker,
        end: CopyMarker,
        make: MakeRange,
        range_type: TypeId,
    }

    impl Markers {
        fn find() -> Option<Self> {
            let find = |name: &CStr| {
                // RTLD_DEFAULT: every library the app has loaded.
                let everywhere = std::ptr::without_provenance_mut::<c_void>(usize::MAX - 1);
                let found = unsafe { dlsym(everywhere, name.as_ptr()) };
                (!found.is_null()).then_some(found)
            };
            let start = find(c"AXTextMarkerRangeCopyStartMarker")?;
            let end = find(c"AXTextMarkerRangeCopyEndMarker")?;
            let make = find(c"AXTextMarkerRangeCreate")?;
            let range_type = find(c"AXTextMarkerRangeGetTypeID")?;
            unsafe {
                Some(Self {
                    start: std::mem::transmute::<*mut c_void, CopyMarker>(start),
                    end: std::mem::transmute::<*mut c_void, CopyMarker>(end),
                    make: std::mem::transmute::<*mut c_void, MakeRange>(make),
                    range_type: std::mem::transmute::<*mut c_void, TypeId>(range_type),
                })
            }
        }

        /// Whether an app's answer is a text marker range, the only thing the others can take.
        fn is_range(&self, value: CFTypeRef) -> bool {
            !value.is_null() && unsafe { CFGetTypeID(value) == (self.range_type)() }
        }

        fn start(&self, range: CFTypeRef) -> Option<Owned> {
            if !self.is_range(range) {
                return None;
            }
            Owned::take(unsafe { (self.start)(range) })
        }

        fn end(&self, range: CFTypeRef) -> Option<Owned> {
            if !self.is_range(range) {
                return None;
            }
            Owned::take(unsafe { (self.end)(range) })
        }

        fn range(&self, from: &Owned, to: &Owned) -> Option<Owned> {
            Owned::take(unsafe { (self.make)(std::ptr::null(), from.get(), to.get()) })
        }
    }

    fn attribute(element: CFTypeRef, name: &str) -> Option<Owned> {
        let name = cf_string(name)?;
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe { AXUIElementCopyAttributeValue(element, name.get(), &mut value) };
        if error == AX_SUCCESS {
            Owned::take(value)
        } else {
            None
        }
    }

    /// An attribute that takes a parameter, like the text of a range.
    fn ask(element: CFTypeRef, name: &str, parameter: CFTypeRef) -> Option<Owned> {
        let name = cf_string(name)?;
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyParameterizedAttributeValue(element, name.get(), parameter, &mut value)
        };
        if error == AX_SUCCESS {
            Owned::take(value)
        } else {
            None
        }
    }

    fn role(element: CFTypeRef) -> String {
        attribute(element, "AXRole")
            .and_then(|role| read_string(role.get()))
            .unwrap_or_default()
    }

    fn children(element: CFTypeRef) -> Vec<Owned> {
        let Some(list) = attribute(element, "AXChildren") else {
            return Vec::new();
        };
        unsafe {
            if CFGetTypeID(list.get()) != CFArrayGetTypeID() {
                return Vec::new();
            }
            (0..CFArrayGetCount(list.get()))
                .filter_map(|i| Owned::retain(CFArrayGetValueAtIndex(list.get(), i)))
                .collect()
        }
    }

    fn same(a: CFTypeRef, b: CFTypeRef) -> bool {
        unsafe { CFEqual(a, b) != 0 }
    }

    fn cf_string(text: &str) -> Option<Owned> {
        let text = CString::new(text).ok()?;
        Owned::take(unsafe { CFStringCreateWithCString(std::ptr::null(), text.as_ptr(), UTF8) })
    }

    /// A CFString's text. None for anything else.
    fn read_string(value: CFTypeRef) -> Option<String> {
        unsafe {
            if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
                return None;
            }
            let size = CFStringGetMaximumSizeForEncoding(CFStringGetLength(value), UTF8) + 1;
            let mut buffer = vec![0u8; usize::try_from(size).ok()?];
            if CFStringGetCString(value, buffer.as_mut_ptr().cast(), size, UTF8) == 0 {
                return None;
            }
            let text = CStr::from_bytes_until_nul(&buffer).ok()?;
            Some(text.to_string_lossy().into_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::keep_around;

    #[test]
    fn keeps_text_from_both_sides_of_the_view() {
        assert_eq!(keep_around("aaaa", "bbbb", 4, ""), "aabb");
    }

    #[test]
    fn a_short_side_gives_its_room_to_the_other() {
        assert_eq!(keep_around("a", "bbbbbb", 4, ""), "abbb");
        assert_eq!(keep_around("aaaaaa", "b", 4, ""), "aaab");
        assert_eq!(keep_around("", "bbbbbb", 4, ""), "bbbb");
    }

    #[test]
    fn never_cuts_a_character_in_half() {
        assert_eq!(keep_around("éé", "éé", 3, ""), "é");
        assert_eq!(keep_around("", "你好", 4, ""), "你");
    }

    #[test]
    fn the_mark_sits_between_the_two_sides() {
        assert_eq!(keep_around("post", "next", 8, "|"), "post|next");
    }
}
