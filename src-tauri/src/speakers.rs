//! Mutes the speakers while the microphone is on, so a video or music doesn't end up in a dictation.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;

/// Runs in order on one thread, so hotkeys never wait on the system's audio calls and a quick press
/// can't unmute before it has muted.
fn commands() -> &'static Sender<bool> {
    static COMMANDS: OnceLock<Sender<bool>> = OnceLock::new();
    COMMANDS.get_or_init(|| {
        let (sender, receiver) = channel();
        std::thread::spawn(move || run(receiver));
        sender
    })
}

pub fn mute() {
    let _ = commands().send(true);
}

pub fn unmute() {
    let _ = commands().send(false);
}

#[cfg(windows)]
fn run(commands: Receiver<bool>) {
    use windows::core::Result;
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    fn speakers() -> Result<IAudioEndpointVolume> {
        unsafe {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            devices
                .GetDefaultAudioEndpoint(eRender, eConsole)?
                .Activate(CLSCTX_ALL, None)
        }
    }

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    // Only speakers muted here get unmuted: ones the user muted themselves stay that way.
    let mut muted_here: Option<IAudioEndpointVolume> = None;
    for mute in commands {
        unsafe {
            if !mute {
                if let Some(volume) = muted_here.take() {
                    let _ = volume.SetMute(false, std::ptr::null());
                }
            } else if muted_here.is_none() {
                if let Ok(volume) = speakers() {
                    if volume.GetMute().is_ok_and(|muted| !muted.as_bool())
                        && volume.SetMute(true, std::ptr::null()).is_ok()
                    {
                        muted_here = Some(volume);
                    }
                }
            }
        }
    }
}

/// macOS keeps a mute switch on each output device. Some, like HDMI screens, have none, and those
/// just keep playing.
#[cfg(target_os = "macos")]
fn run(commands: Receiver<bool>) {
    use std::ffi::c_void;
    use std::ptr::null;

    #[repr(C)]
    struct Property {
        selector: u32,
        scope: u32,
        element: u32,
    }

    const SYSTEM: u32 = 1;
    const DEFAULT_OUTPUT: Property = Property {
        selector: u32::from_be_bytes(*b"dOut"),
        scope: u32::from_be_bytes(*b"glob"),
        element: 0,
    };
    const MUTE: Property = Property {
        selector: u32::from_be_bytes(*b"mute"),
        scope: u32::from_be_bytes(*b"outp"),
        element: 0,
    };

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyData(
            object: u32,
            property: *const Property,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
        fn AudioObjectSetPropertyData(
            object: u32,
            property: *const Property,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: u32,
            data: *const c_void,
        ) -> i32;
    }

    fn read(object: u32, property: &Property) -> Option<u32> {
        let (mut value, mut size) = (0u32, 4u32);
        let data = (&mut value as *mut u32).cast();
        let status =
            unsafe { AudioObjectGetPropertyData(object, property, 0, null(), &mut size, data) };
        (status == 0).then_some(value)
    }

    fn set_muted(device: u32, muted: bool) -> bool {
        let value = u32::from(muted);
        let data = (&value as *const u32).cast();
        unsafe { AudioObjectSetPropertyData(device, &MUTE, 0, null(), 4, data) == 0 }
    }

    // Only a device muted here gets unmuted: one the user muted themselves stays that way.
    let mut muted_here = None;
    for mute in commands {
        if !mute {
            if let Some(device) = muted_here.take() {
                set_muted(device, false);
            }
        } else if muted_here.is_none() {
            if let Some(device) = read(SYSTEM, &DEFAULT_OUTPUT) {
                if read(device, &MUTE) == Some(0) && set_muted(device, true) {
                    muted_here = Some(device);
                }
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run(commands: Receiver<bool>) {
    for _ in commands {}
}
