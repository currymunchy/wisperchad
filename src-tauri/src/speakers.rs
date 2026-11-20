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

/// macOS keeps a mute switch and a volume on each output device, for the whole device or for each
/// of its channels, and many devices have only some of these. AirPods and many USB speakers have no
/// mute switch, so their volume goes to zero instead. Screens on HDMI or DisplayPort have neither,
/// and those just keep playing.
#[cfg(target_os = "macos")]
fn run(commands: Receiver<bool>) {
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::ptr::null;

    #[repr(C)]
    struct Property {
        selector: u32,
        scope: u32,
        element: u32,
    }

    const fn property(selector: &[u8; 4], scope: &[u8; 4], element: u32) -> Property {
        Property {
            selector: u32::from_be_bytes(*selector),
            scope: u32::from_be_bytes(*scope),
            element,
        }
    }

    const SYSTEM: u32 = 1;
    /// The whole device. Its channels count from 1.
    const MAIN: u32 = 0;
    const DEFAULT_OUTPUT: Property = property(b"dOut", b"glob", MAIN);
    const STEREO_CHANNELS: Property = property(b"dch2", b"outp", MAIN);
    const TRANSPORT: Property = property(b"tran", b"glob", MAIN);
    const MUTE: &[u8; 4] = b"mute";
    const VOLUME: &[u8; 4] = b"volm";

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectHasProperty(object: u32, property: *const Property) -> u8;
        fn AudioObjectIsPropertySettable(
            object: u32,
            property: *const Property,
            settable: *mut u8,
        ) -> i32;
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

    fn read<T: Copy + Default>(object: u32, property: &Property) -> Option<T> {
        let mut value = T::default();
        let mut size = size_of::<T>() as u32;
        let data = (&mut value as *mut T).cast();
        let status =
            unsafe { AudioObjectGetPropertyData(object, property, 0, null(), &mut size, data) };
        (status == 0).then_some(value)
    }

    fn write<T: Copy>(object: u32, property: &Property, value: T) -> bool {
        let data = (&value as *const T).cast();
        let size = size_of::<T>() as u32;
        unsafe { AudioObjectSetPropertyData(object, property, 0, null(), size, data) == 0 }
    }

    fn settable(object: u32, property: &Property) -> bool {
        let mut settable = 0u8;
        unsafe {
            AudioObjectHasProperty(object, property) != 0
                && AudioObjectIsPropertySettable(object, property, &mut settable) == 0
                && settable != 0
        }
    }

    /// Where the device can change `selector`: for the whole device, or else for each of its left
    /// and right channels. Empty when it can't at all.
    fn controls(device: u32, selector: &[u8; 4]) -> Vec<Property> {
        let whole = property(selector, b"outp", MAIN);
        if settable(device, &whole) {
            return vec![whole];
        }
        let [left, right] = read::<[u32; 2]>(device, &STEREO_CHANNELS).unwrap_or([1, 2]);
        let mut channels: Vec<Property> = [left, right]
            .into_iter()
            .map(|channel| property(selector, b"outp", channel))
            .filter(|channel| settable(device, channel))
            .collect();
        channels.dedup_by_key(|channel| channel.element);
        channels
    }

    /// What was turned down here, so only that is turned back up: speakers the user muted, or
    /// turned all the way down, stay that way.
    enum Quieted {
        Muted(u32, Vec<Property>),
        Lowered(u32, Vec<(Property, f32)>),
    }

    fn quiet() -> Result<Quieted, String> {
        let device = read::<u32>(SYSTEM, &DEFAULT_OUTPUT)
            .ok_or("Couldn't mute the speakers: macOS named no output device")?;
        let switches = controls(device, MUTE);
        if !switches.is_empty() {
            let muted = switches
                .into_iter()
                .filter(|switch| {
                    read::<u32>(device, switch) == Some(0) && write(device, switch, 1u32)
                })
                .collect();
            return Ok(Quieted::Muted(device, muted));
        }
        let volumes = controls(device, VOLUME);
        if !volumes.is_empty() {
            let lowered = volumes
                .into_iter()
                .filter_map(|volume| {
                    let level = read::<f32>(device, &volume)?;
                    (level > 0.0 && write(device, &volume, 0f32)).then_some((volume, level))
                })
                .collect();
            return Ok(Quieted::Lowered(device, lowered));
        }
        // A four-letter code: "hdmi" and "dprt" are screens, "blue" is Bluetooth, "usb " is USB.
        let kind = read::<u32>(device, &TRANSPORT).unwrap_or(0).to_be_bytes();
        Err(format!(
            "Couldn't mute the speakers: the output device ({}) has no mute or volume control",
            String::from_utf8_lossy(&kind)
        ))
    }

    fn restore(quieted: Quieted) {
        match quieted {
            Quieted::Muted(device, switches) => {
                for switch in switches {
                    write(device, &switch, 0u32);
                }
            }
            Quieted::Lowered(device, volumes) => {
                for (volume, level) in volumes {
                    write(device, &volume, level);
                }
            }
        }
    }

    let mut quieted = None;
    let mut told = false;
    for mute in commands {
        if !mute {
            if let Some(quieted) = quieted.take() {
                restore(quieted);
            }
        } else if quieted.is_none() {
            match quiet() {
                Ok(done) => quieted = Some(done),
                // Once is enough for the log, as it won't change until the output does.
                Err(why) if !told => {
                    told = true;
                    crate::applog::write(&why);
                }
                Err(_) => {}
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn run(commands: Receiver<bool>) {
    for _ in commands {}
}
