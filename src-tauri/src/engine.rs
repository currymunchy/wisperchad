//! The built-in writing engine: llama.cpp's server, downloaded on first use and run as a hidden
//! background process, so cleanup and replies work without installing anything else. It only
//! listens on this PC, and nothing leaves the machine.

use crate::catalog::LlmModel;
use crate::llm::NUM_CTX;
use crate::prompts::{clean_output, Prompt, Slot};
use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::hash_map::RandomState;
use std::collections::VecDeque;
use std::fs::File;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BUILD: &str = "b12020";

pub struct Download {
    pub url: &'static str,
    sha256: &'static str,
    pub bytes: u64,
}

/// The Vulkan build runs on NVIDIA, AMD and Intel GPUs and falls back to the CPU, and needs nothing
/// but the graphics driver.
#[cfg(windows)]
pub const DOWNLOAD: Download = Download {
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12020/llama-b12020-bin-win-vulkan-x64.zip",
    sha256: "aa3452dfa16cdf7d902b44e5445e43e13023c64fb179ca7a13d60611d4371691",
    bytes: 37_515_077,
};

/// Runs on the Mac's GPU through Metal, and on the CPU.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const DOWNLOAD: Download = Download {
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12020/llama-b12020-bin-macos-arm64.tar.gz",
    sha256: "b630ebd5da34b02e86f7294dd5743aba842a0a02cb023130debd64599721c541",
    bytes: 16_509_249,
};

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
pub const DOWNLOAD: Download = Download {
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12020/llama-b12020-bin-macos-x64.tar.gz",
    sha256: "774fba46a668cf6c94eb8d515b2dfb5897878ab6da08943585f487a1dd1aa6a0",
    bytes: 12_248_997,
};

/// Vulkan again, and it picks the best CPU code for the machine on its own.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub const DOWNLOAD: Download = Download {
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12020/llama-b12020-bin-ubuntu-vulkan-x64.tar.gz",
    sha256: "8b7ec1fbef304ea4e024cf34f68ef1276556b38b666dc71b8af5de6e9d93b42c",
    bytes: 35_664_875,
};

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub const DOWNLOAD: Download = Download {
    url: "https://github.com/ggml-org/llama.cpp/releases/download/b12020/llama-b12020-bin-ubuntu-vulkan-arm64.tar.gz",
    sha256: "e4db03071cffa07d3ff278cc39545db7ae729fa6ca907fc96af3f01a70272b30",
    bytes: 28_986_505,
};

const ARCHIVE_EXTENSION: &str = if cfg!(windows) { "zip" } else { "tar.gz" };
const SERVER: &str = if cfg!(windows) {
    "llama-server.exe"
} else {
    "llama-server"
};

const LOAD_TIMEOUT: Duration = Duration::from_secs(180);
const EXIT_GRACE: Duration = Duration::from_millis(500);
const REPLY_TIMEOUT: Duration = Duration::from_secs(300);

/// Now and then a model comes back with nothing at all, then answers normally on a later try.
const REPLY_ATTEMPTS: u32 = 3;

/// A full-size screenshot takes minutes without a strong GPU, so slower PCs get a smaller one. At
/// this size Qwen3.5 4B took under a minute on a desktop CPU and could still read a chart.
const SLOW_PC_IMAGE_TOKENS: &str = "1024";

const LOG_LINES: usize = 40;

/// How many slots the engine keeps: one per `Slot` on the processor, where keeping the writing
/// instructions read between replies saved re-reading them, and one on a graphics card, where a
/// second slot made Qwen3.5 4B three times slower (20 seconds for a reply instead of 7, on an RTX
/// 3070) and reading them again takes a moment.
fn slots(cpu_only: bool) -> u32 {
    if cpu_only || !crate::hardware::has_gpu() {
        2
    } else {
        1
    }
}

type Log = Arc<Mutex<VecDeque<String>>>;

/// The Microsoft Store package carries the engine beside the app, since Store apps mustn't download
/// code to run. Every other install downloads it the first time a writing model is used.
fn bundled() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.join("engine");
    dir.join("llama-server.exe").exists().then_some(dir)
}

pub struct Engine {
    dir: PathBuf,
    server: tokio::sync::Mutex<Option<Server>>,
    uses: AtomicU64,
}

impl Engine {
    pub fn new(app_data_dir: &Path) -> Arc<Self> {
        Arc::new(Self {
            dir: bundled().unwrap_or_else(|| app_data_dir.join("engine").join(BUILD)),
            server: tokio::sync::Mutex::new(None),
            uses: AtomicU64::new(0),
        })
    }

    pub fn is_installed(&self) -> bool {
        self.dir.join(SERVER).exists()
    }

    pub fn archive_path(&self) -> PathBuf {
        self.dir.with_extension(ARCHIVE_EXTENSION)
    }

    /// Checks the download is the build we expect before anything in it can run.
    pub fn unpack(&self, archive: &Path) -> Result<()> {
        let mut hash = Sha256::new();
        std::io::copy(&mut File::open(archive)?, &mut hash)?;
        if format!("{:x}", hash.finalize()) != DOWNLOAD.sha256 {
            let _ = std::fs::remove_file(archive);
            bail!("The writing engine download was damaged. Try again.");
        }
        let unpacking = self.dir.with_extension("unpacking");
        let _ = std::fs::remove_dir_all(&unpacking);
        std::fs::rename(extract(archive, &unpacking)?, &self.dir)?;
        let _ = std::fs::remove_dir_all(&unpacking);
        std::fs::remove_file(archive)?;
        Ok(())
    }

    /// `images` are base64 PNGs sent along with the prompt, for models that can read them. With
    /// `most_tokens`, the model stops there.
    pub async fn chat(
        self: &Arc<Self>,
        model: &LlmModel,
        models_dir: &Path,
        prompt: &Prompt,
        images: &[String],
        keep_alive_minutes: u32,
        most_tokens: Option<u32>,
    ) -> Result<Answer> {
        // Held throughout: one request at a time, and the unload timer waits its turn.
        let mut server = self.server.lock().await;
        let mut reply = self
            .ask(&mut server, model, models_dir, prompt, images, most_tokens, false)
            .await;
        // The GPU can give out partway through, on a big screenshot say. Another try on the same
        // GPU would only do the same.
        if reply.is_err() && server.is_none() {
            reply = self
                .ask(&mut server, model, models_dir, prompt, images, most_tokens, true)
                .await;
        }
        drop(server);
        self.unload_later(keep_alive_minutes);
        reply
    }

    #[allow(clippy::too_many_arguments)]
    async fn ask(
        &self,
        server: &mut Option<Server>,
        model: &LlmModel,
        models_dir: &Path,
        prompt: &Prompt,
        images: &[String],
        most_tokens: Option<u32>,
        cpu_only: bool,
    ) -> Result<Answer> {
        if !server.as_mut().is_some_and(|s| s.serves(model)) {
            *server = None; // Frees the GPU for the next one.
            *server = Some(if cpu_only {
                self.launch(model, models_dir, true).await?
            } else {
                self.start(model, models_dir).await?
            });
        }
        let Some(running) = server.as_mut() else {
            bail!("The writing engine didn't start");
        };
        let reply = running.reply(model, prompt, images, most_tokens).await;
        if reply.is_err() {
            // The error can reach us just before the crashed server has finished exiting.
            tokio::time::sleep(EXIT_GRACE).await;
        }
        if !running.is_alive() {
            *server = None;
        }
        reply
    }

    async fn start(&self, model: &LlmModel, models_dir: &Path) -> Result<Server> {
        match self.launch(model, models_dir, false).await {
            Ok(server) => Ok(server),
            // Another app can be filling the GPU, and some GPUs aren't up to it at all. The CPU is
            // slower but needs neither.
            Err(_) => self.launch(model, models_dir, true).await,
        }
    }

    async fn launch(&self, model: &LlmModel, models_dir: &Path, cpu_only: bool) -> Result<Server> {
        let port = TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port();
        let key = format!("{:016x}", RandomState::new().build_hasher().finish());
        let files = model.dir(models_dir);
        let slots = slots(cpu_only);
        let mut command = Command::new(self.dir.join(SERVER));
        command
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["--api-key", &key])
            // On the processor, a slot for reading and one for writing a reply, each with the full
            // context. Each keeps what it read, with the checkpoints that let a model like Qwen3.5
            // pick up from there, which costs about 0.6 GB. One slot has nothing to pick up, as
            // reading and writing take turns in it. Copies kept in memory beyond that only help a
            // long chat, and failing to get the memory for one crashes the server. The warm-up run
            // adds nothing.
            .args(["--ctx-size", &(NUM_CTX * slots).to_string()])
            .args(["--parallel", &slots.to_string()])
            .args(["--cache-ram", "0", "--no-warmup"])
            .args(["--no-webui", "--reasoning", "off"]);
        if slots == 1 {
            command.args(["--ctx-checkpoints", "0"]);
        }
        for (flag, file) in ["--model", "--mmproj"].iter().zip(&model.files) {
            command.arg(flag).arg(files.join(file.name));
        }
        if cpu_only {
            command.args(["--device", "none"]);
        }
        if cpu_only || !crate::hardware::has_strong_gpu() {
            command.args(["--image-max-tokens", SLOW_PC_IMAGE_TOKENS]);
        }
        hide_window(&mut command);
        let mut child = command
            .spawn()
            .context("Could not start the writing engine")?;
        let log = child.stderr.take().map(keep_log).unwrap_or_default();
        end_with_app(&child);
        let mut server = Server {
            child,
            model: model.id,
            base: format!("http://127.0.0.1:{port}"),
            key,
            slots,
        };
        server.wait_until_ready(model, &log).await?;
        Ok(server)
    }

    fn unload_later(self: &Arc<Self>, minutes: u32) {
        let this_use = self.uses.fetch_add(1, Ordering::SeqCst) + 1;
        let engine = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(minutes as u64 * 60)).await;
            if engine.uses.load(Ordering::SeqCst) == this_use {
                engine.stop().await;
            }
        });
    }

    /// Unloads the model, freeing its memory.
    pub async fn stop(&self) {
        *self.server.lock().await = None;
    }
}

struct Server {
    child: Child,
    model: &'static str,
    base: String,
    key: String,
    slots: u32,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn serves(&mut self, model: &LlmModel) -> bool {
        self.model == model.id && self.is_alive()
    }

    async fn wait_until_ready(&mut self, model: &LlmModel, log: &Log) -> Result<()> {
        let http = client(Duration::from_secs(2));
        let started = Instant::now();
        while started.elapsed() < LOAD_TIMEOUT {
            if !self.is_alive() {
                let lines = log.lock().unwrap();
                bail!(explain_failure(
                    lines.iter().map(String::as_str),
                    model.name
                ));
            }
            if let Ok(res) = http.get(format!("{}/health", self.base)).send().await {
                if res.status().is_success() {
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        bail!("{} took too long to load. Try again.", model.name)
    }

    async fn reply(
        &self,
        model: &LlmModel,
        prompt: &Prompt,
        images: &[String],
        most_tokens: Option<u32>,
    ) -> Result<Answer> {
        let http = client(REPLY_TIMEOUT);
        let url = format!("{}/v1/chat/completions", self.base);
        let body = request_body(prompt, images, self.slots, most_tokens);
        for _ in 0..REPLY_ATTEMPTS {
            let res = http
                .post(&url)
                .bearer_auth(&self.key)
                .json(&body)
                .send()
                .await
                .context("The writing engine stopped responding. Try again.")?;
            let status = res.status();
            let text = res.text().await?;
            if !status.is_success() {
                bail!("The writing engine couldn't answer ({status}). Try again.");
            }
            let (text, cut) = reply_text(&text)?;
            let reply = clean_output(&text);
            if !reply.is_empty() {
                return Ok(Answer { text: reply, cut });
            }
        }
        Err(anyhow!("{} came back with nothing. Try again.", model.name))
    }
}

/// Returns the folder that ends up holding the server.
#[cfg(windows)]
fn extract(archive: &Path, to: &Path) -> Result<PathBuf> {
    zip::ZipArchive::new(File::open(archive)?)?.extract(to)?;
    Ok(to.to_path_buf())
}

/// Returns the folder that ends up holding the server. These archives wrap it in one more folder
/// than the Windows one.
#[cfg(unix)]
fn extract(archive: &Path, to: &Path) -> Result<PathBuf> {
    tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?)).unpack(to)?;
    Ok(to.join(format!("llama-{BUILD}")))
}

fn client(timeout: Duration) -> reqwest::Client {
    // A proxy set up for the internet has no business seeing a connection inside this PC.
    reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .build()
        .expect("http client")
}

/// What the model wrote, and whether it stopped there only because it reached its limit.
pub struct Answer {
    pub text: String,
    pub cut: bool,
}

fn request_body(
    prompt: &Prompt,
    images: &[String],
    slots: u32,
    most_tokens: Option<u32>,
) -> Value {
    let user = if images.is_empty() {
        json!(prompt.user)
    } else {
        let mut parts = vec![json!({ "type": "text", "text": prompt.user })];
        parts.extend(images.iter().map(|image| {
            json!({
                "type": "image_url",
                "image_url": { "url": format!("data:image/png;base64,{image}") },
            })
        }));
        json!(parts)
    };
    let slot = match prompt.slot {
        Slot::Writing if slots > 1 => 1,
        _ => 0,
    };
    let mut body = json!({
        "temperature": 0.3,
        "id_slot": slot,
        "cache_prompt": true,
        "messages": [
            { "role": "system", "content": prompt.system },
            { "role": "user", "content": user },
        ],
    });
    if let Some(most) = most_tokens {
        body["max_tokens"] = json!(most);
    }
    body
}

#[derive(Deserialize)]
struct Completion {
    choices: Vec<Choice>,
}
#[derive(Deserialize)]
struct Choice {
    message: Message,
    #[serde(default)]
    finish_reason: Option<String>,
}
#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: Option<String>,
}

/// The answer's text, and whether the model was stopped at its limit.
fn reply_text(body: &str) -> Result<(String, bool)> {
    let completion: Completion = serde_json::from_str(body)?;
    Ok(completion
        .choices
        .into_iter()
        .next()
        .map_or((String::new(), false), |choice| {
            let cut = choice.finish_reason.as_deref() == Some("length");
            (choice.message.content.unwrap_or_default(), cut)
        }))
}

/// The server's last lines, kept in memory only, to tell why it stopped.
fn keep_log(stderr: ChildStderr) -> Log {
    let log = Log::default();
    let lines = log.clone();
    // Runs for as long as the server does: a pipe nobody reads fills up and stalls it.
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = Vec::new();
        while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            let mut lines = lines.lock().unwrap();
            if lines.len() == LOG_LINES {
                lines.pop_front();
            }
            lines.push_back(String::from_utf8_lossy(&line).trim_end().to_string());
            line.clear();
        }
    });
    log
}

fn explain_failure<'a>(lines: impl Iterator<Item = &'a str>, name: &str) -> String {
    let lines: Vec<&str> = lines.collect();
    let out_of_memory = |line: &&str| {
        [
            "failed to allocate",
            "unable to allocate",
            "OutOfDeviceMemory",
        ]
        .iter()
        .any(|s| line.contains(s))
    };
    if let Some(library) = lines.iter().find_map(|line| missing_library(line)) {
        format!(
            "The writing engine needs {library}, which isn't on this PC. Install it with your package manager, then try again."
        )
    } else if lines.iter().any(out_of_memory) {
        format!(
            "Not enough free memory for {name}. Close other apps or pick a smaller writing model."
        )
    } else {
        format!(
            "The writing engine couldn't load {name}. Try again, or pick another writing model."
        )
    }
}

/// Linux names the system library it couldn't find when a program won't start.
fn missing_library(line: &str) -> Option<&str> {
    line.split("shared libraries: ").nth(1)?.split(':').next()
}

#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_window(_command: &mut Command) {}

/// Puts the server in a job that Windows ends along with this app, so a crash can't leave it
/// holding the GPU.
#[cfg(windows)]
fn end_with_app(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_BASIC_LIMIT_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // Never closed: the job is meant to last exactly as long as this process.
    static JOB: OnceLock<usize> = OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let Ok(job) = CreateJobObjectW(None, PCWSTR::null()) else {
            return 0;
        };
        let limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
            BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION {
                LimitFlags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                ..Default::default()
            },
            ..Default::default()
        };
        let _ = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _,
            std::mem::size_of_val(&limits) as u32,
        );
        job.0 as usize
    });
    if job != 0 {
        unsafe {
            let _ = AssignProcessToJobObject(
                HANDLE(job as *mut _),
                HANDLE(child.as_raw_handle() as *mut _),
            );
        }
    }
}

/// Leaves a small shell script watching, because neither macOS nor Linux can tie a process to
/// this app the way Windows can, and a crash or a quit shouldn't leave the server holding the GPU.
#[cfg(unix)]
fn end_with_app(child: &Child) {
    let watcher = Command::new("/bin/sh")
        .args(["-c", &end_with(std::process::id(), child.id())])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(mut watcher) = watcher {
        // Collected when it finishes, or it would stay in the process list until this app quits.
        std::thread::spawn(move || watcher.wait());
    }
}

/// Stops by itself once the server does, so restarts don't pile these up.
#[cfg(unix)]
fn end_with(app: u32, server: u32) -> String {
    format!("while kill -0 {app} && kill -0 {server}; do sleep 2; done; kill -9 {server}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt() -> Prompt {
        Prompt {
            system: "be brief".into(),
            user: "hello".into(),
            slot: Slot::Writing,
        }
    }

    #[test]
    fn text_goes_as_plain_content() {
        let body = request_body(&prompt(), &[], 2, None);
        assert_eq!(body["messages"][0]["content"], "be brief");
        assert_eq!(body["messages"][1]["content"], "hello");
        assert_eq!(body["id_slot"], 1);
        assert!(body.get("max_tokens").is_none());
        // With one slot, on a graphics card, reading and writing share it.
        assert_eq!(request_body(&prompt(), &[], 1, None)["id_slot"], 0);
        assert_eq!(request_body(&prompt(), &[], 1, Some(120))["max_tokens"], 120);
    }

    #[test]
    fn images_go_alongside_the_text() {
        let body = request_body(&prompt(), &["aGk=".to_string()], 2, None);
        let parts = &body["messages"][1]["content"];
        assert_eq!(parts[0]["text"], "hello");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,aGk=");
    }

    #[test]
    fn replies_are_read_from_the_first_choice() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"Hi there"},"finish_reason":"stop"}]}"#;
        assert_eq!(reply_text(body).unwrap(), ("Hi there".to_string(), false));
        let cut = r#"{"choices":[{"message":{"role":"assistant","content":"Hi th"},"finish_reason":"length"}]}"#;
        assert_eq!(reply_text(cut).unwrap(), ("Hi th".to_string(), true));
        let empty = r#"{"choices":[{"message":{"role":"assistant","content":null}}]}"#;
        assert_eq!(reply_text(empty).unwrap(), (String::new(), false));
        assert_eq!(reply_text(r#"{"choices":[]}"#).unwrap(), (String::new(), false));
    }

    #[test]
    fn failures_are_explained_in_plain_words() {
        // Real lines from a GPU another app had filled, and from a full PC.
        for line in [
            "ggml_vulkan: vk::Device::allocateMemory: ErrorOutOfDeviceMemory",
            "0.01.542.339 E ggml_backend_buft_alloc_buffer_n_default: failed to allocate Vulkan0 buffer of size 1066922496",
            "0.01.590.992 E llama_model_load: error loading model: unable to allocate CPU_REPACK buffer",
        ] {
            assert!(explain_failure([line].into_iter(), "Qwen3.5 4B")
                .starts_with("Not enough free memory for Qwen3.5 4B"));
        }
        let other = explain_failure(["E something else"].into_iter(), "Qwen3.5 4B");
        assert!(other.starts_with("The writing engine couldn't load"));
    }

    #[test]
    fn a_missing_system_library_is_named() {
        let line = "llama-server: error while loading shared libraries: libgomp.so.1: cannot open shared object file: No such file or directory";
        let failure = explain_failure([line].into_iter(), "Qwen3.5 4B");
        assert!(failure.starts_with("The writing engine needs libgomp.so.1,"));
    }

    #[test]
    fn the_download_is_this_build_for_this_system() {
        let name = DOWNLOAD.url.rsplit('/').next().unwrap();
        assert!(name.starts_with(&format!("llama-{BUILD}-bin-")));
        assert!(name.ends_with(ARCHIVE_EXTENSION));
        assert!(DOWNLOAD.url.contains(&format!("/download/{BUILD}/")));
        assert_eq!(DOWNLOAD.sha256.len(), 64);
    }

    #[test]
    fn a_damaged_download_is_thrown_away() {
        let dir = std::env::temp_dir().join(format!("wisperchad-engine-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let engine = Engine::new(&dir);
        let archive = engine.archive_path();
        std::fs::create_dir_all(archive.parent().unwrap()).unwrap();
        std::fs::write(&archive, b"not the engine").unwrap();
        assert!(engine.unpack(&archive).is_err());
        assert!(!archive.exists());
        assert!(!engine.is_installed());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn archives_unpack_with_their_links_and_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("wisperchad-unpack-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("engine.tar.gz");
        let gzip = flate2::write::GzEncoder::new(
            File::create(&archive).unwrap(),
            flate2::Compression::fast(),
        );
        let mut tar = tar::Builder::new(gzip);
        let top = format!("llama-{BUILD}");
        let mut file = tar::Header::new_gnu();
        file.set_size(2);
        file.set_mode(0o755);
        tar.append_data(&mut file, format!("{top}/{SERVER}"), &b"hi"[..])
            .unwrap();
        let mut link = tar::Header::new_gnu();
        link.set_entry_type(tar::EntryType::Symlink);
        link.set_size(0);
        tar.append_link(&mut link, format!("{top}/libllama.so.0"), SERVER)
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();

        let unpacked = extract(&archive, &dir.join("out")).unwrap();
        let mode = std::fs::metadata(unpacked.join(SERVER))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
        assert_eq!(
            std::fs::read_link(unpacked.join("libllama.so.0")).unwrap(),
            Path::new(SERVER)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_server_goes_when_the_app_does() {
        let sleeper = || Command::new("sleep").arg("30").spawn().unwrap();
        let (mut app, mut server) = (sleeper(), sleeper());
        let mut watcher = Command::new("/bin/sh")
            .args(["-c", &end_with(app.id(), server.id())])
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        app.kill().unwrap();
        app.wait().unwrap();
        // `sleep 30` finishing by itself would report a clean exit, not a kill.
        let ended = server.wait().unwrap();
        assert!(!ended.success());
        watcher.wait().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_watcher_stops_with_the_server() {
        let sleeper = || Command::new("sleep").arg("30").spawn().unwrap();
        let (mut app, mut server) = (sleeper(), sleeper());
        let mut watcher = Command::new("/bin/sh")
            .args(["-c", &end_with(app.id(), server.id())])
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        server.kill().unwrap();
        server.wait().unwrap();
        watcher.wait().unwrap();
        assert!(app.try_wait().unwrap().is_none());
        app.kill().unwrap();
        app.wait().unwrap();
    }
}
