//! The system permissions wisperChad needs, for setup to check and ask for. On Windows that's the
//! microphone, which the privacy settings can switch off for desktop apps; macOS has more of them.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Permission {
    /// What `request` takes.
    pub id: &'static str,
    pub name: &'static str,
    /// Why wisperChad needs it.
    pub why: &'static str,
    /// Whether it's allowed. None when the system can't say until wisperChad has asked.
    pub allowed: Option<bool>,
}

/// The permissions this system has, and whether each one is allowed.
pub fn list() -> Vec<Permission> {
    imp::list()
}

/// Asks for one: shows the system's prompt, or opens the settings page where it's switched on.
pub fn request(id: &str) -> anyhow::Result<()> {
    imp::request(id)
}

/// Windows keeps "Allow" or "Deny" for each kind of access. Anything but "Deny", like no setting
/// at all, lets the app in.
#[cfg_attr(not(windows), allow(dead_code))]
fn consent_allows(value: Option<&str>) -> bool {
    !value.is_some_and(|value| value.trim().eq_ignore_ascii_case("deny"))
}

#[cfg(windows)]
mod imp {
    use super::{consent_allows, Permission};
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
    };

    const MICROPHONE: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone";

    /// The "Value" text at `key`, if there is one.
    fn consent(root: HKEY, key: &str) -> Option<String> {
        let key: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
        let mut text = [0u16; 64];
        let mut size = std::mem::size_of_val(&text) as u32;
        let status = unsafe {
            RegGetValueW(
                root,
                PCWSTR(key.as_ptr()),
                w!("Value"),
                RRF_RT_REG_SZ,
                None,
                Some(text.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status.is_err() {
            return None;
        }
        // The size is in bytes and counts the closing null.
        let len = (size as usize / 2).saturating_sub(1).min(text.len());
        Some(String::from_utf16_lossy(&text[..len]))
    }

    pub fn list() -> Vec<Permission> {
        // Switched off for the whole PC, for this user, or for desktop apps: any one stops it.
        let desktop_apps = format!("{MICROPHONE}\\NonPackaged");
        let allowed = [
            consent(HKEY_LOCAL_MACHINE, MICROPHONE),
            consent(HKEY_CURRENT_USER, MICROPHONE),
            consent(HKEY_CURRENT_USER, &desktop_apps),
        ]
        .iter()
        .all(|value| consent_allows(value.as_deref()));
        vec![Permission {
            id: "microphone",
            name: "Microphone",
            why: "To hear you while you hold the hotkey.",
            allowed: Some(allowed),
        }]
    }

    pub fn request(id: &str) -> anyhow::Result<()> {
        match id {
            "microphone" => {
                std::process::Command::new("explorer")
                    .arg("ms-settings:privacy-microphone")
                    .spawn()?;
                Ok(())
            }
            _ => anyhow::bail!("Unknown permission: {id}"),
        }
    }
}

/// The microphone, pressing keys and reading the screen are each a switch in System Settings, under
/// Privacy & Security. The hotkeys need none: they're registered with the system, or read from the
/// keyboard state every app can see. Hasn't been tried on a real Mac.
#[cfg(target_os = "macos")]
mod imp {
    use super::Permission;
    use block2::RcBlock;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    use std::sync::atomic::{AtomicBool, Ordering};

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        static AVMediaTypeAudio: *const AnyObject;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    /// The microphone's answer before macOS has asked, and once it's allowed.
    const NOT_ASKED: isize = 0;
    const ALLOWED: isize = 3;

    /// macOS shows each of these prompts once at most. Past that, Allow opens the switch instead.
    static ASKED_ACCESSIBILITY: AtomicBool = AtomicBool::new(false);
    static ASKED_SCREEN: AtomicBool = AtomicBool::new(false);

    fn microphone() -> isize {
        let audio = unsafe { AVMediaTypeAudio };
        unsafe { msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: audio] }
    }

    pub fn list() -> Vec<Permission> {
        let microphone = match microphone() {
            NOT_ASKED => None,
            status => Some(status == ALLOWED),
        };
        vec![
            Permission {
                id: "microphone",
                name: "Microphone",
                why: "To hear you while you hold the hotkey.",
                allowed: microphone,
            },
            Permission {
                id: "accessibility",
                name: "Accessibility",
                why: "To press Cmd+V for you, so your words land where your cursor is.",
                allowed: Some(unsafe { AXIsProcessTrusted() }),
            },
            Permission {
                id: "screen",
                name: "Screen Recording",
                why: "To read the open window when you ask for a reply.",
                allowed: Some(unsafe { CGPreflightScreenCaptureAccess() }),
            },
        ]
    }

    pub fn request(id: &str) -> anyhow::Result<()> {
        match id {
            "microphone" if microphone() == NOT_ASKED => {
                let audio = unsafe { AVMediaTypeAudio };
                let answered = RcBlock::new(|_allowed: Bool| {});
                let _: () = unsafe {
                    msg_send![class!(AVCaptureDevice), requestAccessForMediaType: audio, completionHandler: &*answered]
                };
                Ok(())
            }
            "microphone" => open_settings("Privacy_Microphone"),
            "accessibility" if !ASKED_ACCESSIBILITY.swap(true, Ordering::SeqCst) => {
                // Starting the keyboard with its default settings asks, and adds wisperChad to the list.
                let _ = enigo::Enigo::new(&enigo::Settings::default());
                Ok(())
            }
            "accessibility" => open_settings("Privacy_Accessibility"),
            "screen" if !ASKED_SCREEN.swap(true, Ordering::SeqCst) => {
                unsafe { CGRequestScreenCaptureAccess() };
                Ok(())
            }
            "screen" => open_settings("Privacy_ScreenCapture"),
            _ => anyhow::bail!("Unknown permission: {id}"),
        }
    }

    fn open_settings(pane: &str) -> anyhow::Result<()> {
        std::process::Command::new("open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?{pane}"
            ))
            .spawn()?;
        Ok(())
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::Permission;

    pub fn list() -> Vec<Permission> {
        Vec::new()
    }

    pub fn request(id: &str) -> anyhow::Result<()> {
        anyhow::bail!("Unknown permission: {id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_deny_keeps_an_app_out() {
        assert!(consent_allows(None));
        assert!(consent_allows(Some("Allow")));
        assert!(!consent_allows(Some("Deny")));
        assert!(!consent_allows(Some(" deny ")));
    }

    #[test]
    fn every_permission_says_what_it_is_for() {
        for permission in list() {
            assert!(!permission.name.is_empty() && !permission.why.is_empty());
            assert!(request("no such thing").is_err());
        }
    }
}
