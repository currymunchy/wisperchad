//! Detects RAM and GPU memory so the app can say whether a model will run well.

use serde::Serialize;

/// How much of a Mac's memory macOS lets the GPU use, roughly.
#[cfg(target_os = "macos")]
const SHARED_WITH_GPU: f32 = 2.0 / 3.0;

/// Below this a GPU is the kind built into a processor, which borrows main memory.
#[cfg(any(windows, target_os = "linux"))]
const MIN_VRAM_GB: f32 = 2.0;

#[derive(Debug, Clone, Serialize)]
pub struct Gpu {
    pub name: String,
    pub vram_gb: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hardware {
    pub ram_gb: f32,
    pub cpu: String,
    pub cpu_cores: usize,
    /// NVIDIA GPUs via nvidia-smi, plus other makers' on Windows, AMD on Linux and Apple Silicon on
    /// macOS. Intel GPUs on Linux and Intel Macs aren't detected yet.
    pub gpus: Vec<Gpu>,
    /// True when built with GPU acceleration for speech recognition (CUDA or Vulkan).
    pub gpu_build: bool,
}

pub fn detect() -> Hardware {
    let (ram_gb, cpu, cpu_cores) = system();
    Hardware {
        ram_gb,
        cpu,
        cpu_cores,
        gpus: gpus(),
        gpu_build: cfg!(any(feature = "cuda", feature = "vulkan")),
    }
}

fn system() -> (f32, String, usize) {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    let cpu = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_default();
    (
        sys.total_memory() as f32 / 1024f32.powi(3),
        cpu,
        sys.cpus().len(),
    )
}

/// Whether there's a GPU that can read a full-size screenshot in reasonable time.
#[cfg(windows)]
pub fn has_strong_gpu() -> bool {
    !nvidia_gpus().is_empty()
}

#[cfg(not(windows))]
pub fn has_strong_gpu() -> bool {
    !gpus().is_empty()
}

/// Whether there's a graphics card the writing model can use, looked up once.
pub fn has_gpu() -> bool {
    static FOUND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| !gpus().is_empty())
}

#[cfg(windows)]
fn gpus() -> Vec<Gpu> {
    let mut gpus = nvidia_gpus();
    gpus.extend(other_gpus());
    gpus
}

#[cfg(target_os = "linux")]
fn gpus() -> Vec<Gpu> {
    let mut gpus = nvidia_gpus();
    gpus.extend(amd_gpus(std::path::Path::new("/sys/class/drm")));
    gpus
}

/// An Apple Silicon GPU has no memory of its own, so it's listed with its share of the Mac's. It
/// goes by the chip's name, which is also the processor's.
#[cfg(target_os = "macos")]
fn gpus() -> Vec<Gpu> {
    if cfg!(not(target_arch = "aarch64")) {
        return vec![];
    }
    let (ram_gb, name, _) = system();
    vec![Gpu {
        name,
        vram_gb: ram_gb * SHARED_WITH_GPU,
    }]
}

/// AMD and Intel cards, which nvidia-smi can't see. The writing engine runs on those too.
#[cfg(windows)]
fn other_gpus() -> Vec<Gpu> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };
    const NVIDIA: u32 = 0x10DE;
    let Ok(factory) = (unsafe { CreateDXGIFactory1::<IDXGIFactory1>() }) else {
        return vec![];
    };
    (0..)
        .map_while(|i| unsafe { factory.EnumAdapters1(i) }.ok())
        .filter_map(|adapter| unsafe { adapter.GetDesc1() }.ok())
        .filter(|d| d.VendorId != NVIDIA && d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 == 0)
        .map(|d| Gpu {
            name: String::from_utf16_lossy(&d.Description)
                .trim_end_matches('\0')
                .to_string(),
            vram_gb: d.DedicatedVideoMemory as f32 / 1024f32.powi(3),
        })
        .filter(|gpu| gpu.vram_gb >= MIN_VRAM_GB)
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn nvidia_gpus() -> Vec<Gpu> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=name,memory.total",
        "--format=csv,noheader,nounits",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.output() {
        Ok(out) if out.status.success() => parse_nvidia_smi(&String::from_utf8_lossy(&out.stdout)),
        _ => vec![],
    }
}

#[cfg(not(target_os = "macos"))]
pub fn parse_nvidia_smi(output: &str) -> Vec<Gpu> {
    output
        .lines()
        .filter_map(|line| {
            let (name, mib) = line.rsplit_once(',')?;
            let mib: f32 = mib.trim().parse().ok()?;
            Some(Gpu {
                name: name.trim().to_string(),
                vram_gb: mib / 1024.0,
            })
        })
        .collect()
}

/// AMD's Linux driver lists each card under `drm`, with its memory in bytes.
#[cfg(target_os = "linux")]
fn amd_gpus(drm: &std::path::Path) -> Vec<Gpu> {
    const AMD: &str = "0x1002";
    let read = |path: std::path::PathBuf| std::fs::read_to_string(path).ok();
    let is_card = |name: &str| name.starts_with("card") && !name.contains('-');
    let cards = std::fs::read_dir(drm).into_iter().flatten().flatten();
    cards
        .filter(|card| is_card(&card.file_name().to_string_lossy()))
        .filter_map(|card| {
            let device = card.path().join("device");
            if read(device.join("vendor"))?.trim() != AMD {
                return None;
            }
            let bytes: f32 = read(device.join("mem_info_vram_total"))?
                .trim()
                .parse()
                .ok()?;
            let vram_gb = bytes / 1024f32.powi(3);
            let name = read(device.join("product_name"));
            (vram_gb >= MIN_VRAM_GB).then(|| Gpu {
                name: name.map_or("AMD GPU".to_string(), |name| name.trim().to_string()),
                vram_gb,
            })
        })
        .collect()
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn parses_nvidia_smi_csv() {
        let gpus = parse_nvidia_smi("NVIDIA GeForce RTX 3070, 8192\n");
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].name, "NVIDIA GeForce RTX 3070");
        assert!((gpus[0].vram_gb - 8.0).abs() < 0.01);
        assert!(parse_nvidia_smi("garbage").is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn finds_amd_cards_with_their_own_memory() {
        let drm = std::env::temp_dir().join(format!("wisperchad-drm-{}", std::process::id()));
        let card = |name: &str, vendor: &str, bytes: u64| {
            let device = drm.join(name).join("device");
            std::fs::create_dir_all(&device).unwrap();
            std::fs::write(device.join("vendor"), format!("{vendor}\n")).unwrap();
            std::fs::write(device.join("mem_info_vram_total"), format!("{bytes}\n")).unwrap();
        };
        card("card0", "0x1002", 8 << 30);
        card("card1", "0x1002", 512 << 20); // Built into the processor.
        card("card2", "0x8086", 8 << 30); // Not AMD.
        card("card0-DP-1", "0x1002", 8 << 30); // A monitor socket on card0.
        card("renderD128", "0x1002", 8 << 30); // card0 again.

        let gpus = amd_gpus(&drm);
        std::fs::remove_dir_all(&drm).unwrap();

        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].name, "AMD GPU");
        assert!((gpus[0].vram_gb - 8.0).abs() < 0.01);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn no_drm_folder_means_no_gpus() {
        assert!(amd_gpus(std::path::Path::new("/no/such/folder")).is_empty());
    }
}
