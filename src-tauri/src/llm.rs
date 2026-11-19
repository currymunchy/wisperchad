//! Talks to a local Ollama server (https://ollama.com). Nothing leaves the machine.

use crate::prompts::Prompt;
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

/// Context window for every request. Left unset, Ollama can use the model's maximum (262,144
/// tokens for Qwen3), which reserves gigabytes of memory that short rewrites never touch. The
/// longest prompt the app sends, a 3440x1440 screenshot with a page's text, is about 6,100 tokens.
pub const NUM_CTX: u32 = 8192;

/// Ollama's runner can crash while compiling for the GPU, then work on a later try.
const LOAD_ATTEMPTS: u32 = 4;

/// Now and then a model comes back with nothing at all, then answers normally on a later try.
const REPLY_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct OllamaStatus {
    pub running: bool,
    pub installed: Vec<String>,
    /// Download size in bytes of each installed model.
    pub sizes: HashMap<String, u64>,
}

#[derive(Deserialize)]
struct Tags {
    models: Vec<TagModel>,
}
#[derive(Deserialize)]
struct TagModel {
    name: String,
    #[serde(default)]
    size: u64,
}

/// What a model really used when loaded on this PC.
#[derive(Debug, Clone, Serialize)]
pub struct Measured {
    pub total_bytes: u64,
    pub vram_bytes: u64,
}

#[derive(Deserialize)]
struct Running {
    models: Vec<RunningModel>,
}
#[derive(Deserialize)]
struct RunningModel {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    size_vram: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PullProgress {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub completed: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

fn client(timeout: Option<Duration>) -> reqwest::Client {
    let mut b = reqwest::Client::builder().connect_timeout(Duration::from_secs(3));
    if let Some(t) = timeout {
        b = b.timeout(t);
    }
    b.build().expect("http client")
}

pub async fn status(base: &str) -> OllamaStatus {
    let res = client(Some(Duration::from_secs(4)))
        .get(format!("{base}/api/tags"))
        .send()
        .await;
    match res {
        Ok(r) if r.status().is_success() => {
            let models = r.json::<Tags>().await.map(|t| t.models).unwrap_or_default();
            OllamaStatus {
                running: true,
                installed: models.iter().map(|m| m.name.clone()).collect(),
                sizes: models.into_iter().map(|m| (m.name, m.size)).collect(),
            }
        }
        _ => OllamaStatus {
            running: false,
            installed: vec![],
            sizes: HashMap::new(),
        },
    }
}

/// Loads a model, reads how much memory it really took, then unloads it again. Ollama only lists
/// download sizes, and those are a poor guide: memory use can be far above or below them.
pub async fn measure(base: &str, tag: &str) -> Result<Measured> {
    let http = client(Some(Duration::from_secs(180)));
    let load = json!({
        "model": tag,
        "stream": false,
        "keep_alive": "2m",
        "options": { "num_ctx": NUM_CTX },
    });
    send_loading(&http, &format!("{base}/api/generate"), &load, tag).await?;
    let running: Running = http
        .get(format!("{base}/api/ps"))
        .send()
        .await?
        .json()
        .await?;
    let _ = http
        .post(format!("{base}/api/generate"))
        .json(&json!({ "model": tag, "stream": false, "keep_alive": 0 }))
        .send()
        .await;
    running
        .models
        .into_iter()
        .find(|m| is_installed(std::slice::from_ref(&m.name), tag))
        .map(|m| Measured {
            total_bytes: m.size,
            vram_bytes: m.size_vram,
        })
        .ok_or_else(|| anyhow!("Ollama didn't report the model as loaded"))
}

/// Sends a request that may have to load the model and returns the response body.
async fn send_loading(
    http: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
    tag: &str,
) -> Result<String> {
    let mut attempt = 1;
    loop {
        let res = http
            .post(url)
            .json(body)
            .send()
            .await
            .context("Could not reach Ollama. Is it open?")?;
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        if status.is_success() {
            return Ok(text);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(anyhow!(
                "Writing model {tag} isn't installed in Ollama. Install it on the Models page."
            ));
        }
        if !runner_crashed(&text) || attempt == LOAD_ATTEMPTS {
            return Err(anyhow!(load_error(&text, tag)));
        }
        attempt += 1;
    }
}

fn runner_crashed(error: &str) -> bool {
    error.contains("CUDA error") || error.contains("terminated")
}

/// Turns Ollama's `{"error":"..."}` responses into something fit to show in the status pill.
fn load_error(body: &str, tag: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .unwrap_or_else(|| body.to_string());
    let lower = message.to_lowercase();
    if ["out of memory", "out-of-memory", "failed to allocate"]
        .iter()
        .any(|s| lower.contains(s))
    {
        return format!(
            "Not enough free memory for {tag}. Close other apps or pick a smaller writing model."
        );
    }
    if runner_crashed(&message) {
        return format!("Ollama crashed while loading {tag}. Try again, or restart Ollama.");
    }
    message
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(200)
        .collect()
}

/// `installed` holds names like "qwen3:4b" or "llama3.2:latest".
pub fn is_installed(installed: &[String], tag: &str) -> bool {
    installed
        .iter()
        .any(|n| n == tag || (!tag.contains(':') && n == &format!("{tag}:latest")))
}

pub async fn pull(base: &str, tag: &str, mut on_progress: impl FnMut(PullProgress)) -> Result<()> {
    let res = client(None)
        .post(format!("{base}/api/pull"))
        .json(&json!({ "model": tag, "stream": true }))
        .send()
        .await
        .context("Could not reach Ollama. Is it installed and running?")?;
    if !res.status().is_success() {
        return Err(anyhow!("Ollama returned {}", res.status()));
    }
    let mut stream = res.bytes_stream();
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk?);
        while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            if let Ok(p) = serde_json::from_slice::<PullProgress>(&line) {
                if let Some(e) = &p.error {
                    return Err(anyhow!(e.clone()));
                }
                on_progress(p);
            }
        }
    }
    Ok(())
}

pub async fn delete(base: &str, tag: &str) -> Result<()> {
    let res = client(Some(Duration::from_secs(30)))
        .delete(format!("{base}/api/delete"))
        .json(&json!({ "model": tag }))
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(anyhow!("Ollama returned {}", res.status()));
    }
    Ok(())
}

#[derive(Deserialize)]
struct ShowResponse {
    #[serde(default)]
    capabilities: Vec<String>,
}

/// Whether a model can read images, which replies from a screenshot need.
pub async fn supports_vision(base: &str, tag: &str) -> Result<bool> {
    let show: ShowResponse = client(Some(Duration::from_secs(10)))
        .post(format!("{base}/api/show"))
        .json(&json!({ "model": tag }))
        .send()
        .await
        .context("Could not reach Ollama. Is it open?")?
        .json()
        .await?;
    Ok(show.capabilities.iter().any(|c| c == "vision"))
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}
#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

/// `images` are base64 images sent along with the prompt, for models that can read them.
pub async fn chat(
    base: &str,
    tag: &str,
    prompt: &Prompt,
    keep_alive_minutes: u32,
    thinking_model: bool,
    images: &[String],
) -> Result<String> {
    let mut user = json!({ "role": "user", "content": prompt.user });
    if !images.is_empty() {
        user["images"] = json!(images);
    }
    let mut body = json!({
        "model": tag,
        "stream": false,
        "keep_alive": format!("{keep_alive_minutes}m"),
        "messages": [
            { "role": "system", "content": prompt.system },
            user,
        ],
        "options": { "temperature": 0.3, "num_ctx": NUM_CTX },
    });
    if thinking_model {
        // Thinking adds seconds of delay and doesn't help short rewrites.
        body["think"] = json!(false);
    }
    let http = client(Some(Duration::from_secs(180)));
    let url = format!("{base}/api/chat");
    for _ in 0..REPLY_ATTEMPTS {
        let text = send_loading(&http, &url, &body, tag).await?;
        let r: ChatResponse = serde_json::from_str(&text)?;
        let reply = crate::prompts::clean_output(&r.message.content);
        if !reply.is_empty() {
            return Ok(reply);
        }
    }
    Err(anyhow!("{tag} came back with nothing. Try again."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_matching() {
        let have = vec!["qwen3:4b".to_string(), "llama3.2:latest".to_string()];
        assert!(is_installed(&have, "qwen3:4b"));
        assert!(is_installed(&have, "llama3.2"));
        assert!(!is_installed(&have, "qwen3:8b"));
    }

    #[test]
    fn tags_parse_with_and_without_sizes() {
        let tags: Tags =
            serde_json::from_str(r#"{"models":[{"name":"a:1","size":5},{"name":"b:2"}]}"#).unwrap();
        assert_eq!(tags.models[0].size, 5);
        assert_eq!(tags.models[1].size, 0);
    }

    #[test]
    fn load_errors_are_short_and_readable() {
        let no_memory = r#"{"error":"llama-server reported out-of-memory during startup: ggml_backend_cpu_buffer_type_alloc_buffer: failed to allocate buffer of size 7247757312\nggml"}"#;
        assert_eq!(
            load_error(no_memory, "qwen3:4b"),
            "Not enough free memory for qwen3:4b. Close other apps or pick a smaller writing model."
        );
        let crash = r#"{"error":"an error was encountered while running the model: CUDA error: a PTX JIT compilation failed\nCUDA error"}"#;
        assert!(load_error(crash, "m").starts_with("Ollama crashed"));
        assert_eq!(load_error("plain text", "m"), "plain text");
        assert_eq!(load_error(&"é".repeat(500), "m").chars().count(), 200);
    }

    #[test]
    fn only_runner_crashes_are_retried() {
        assert!(runner_crashed(
            "llama-server process has terminated: exit status 0xc0000409"
        ));
        assert!(runner_crashed("CUDA error: a PTX JIT compilation failed"));
        assert!(!runner_crashed(
            "failed to allocate buffer of size 7247757312"
        ));
    }
}
