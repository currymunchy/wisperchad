//! Downloads speech models with progress, writing to a .part file first so a cancelled
//! or failed download never leaves a corrupt model behind.

use anyhow::{anyhow, Result};
use futures_util::StreamExt;
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub async fn download(
    url: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<()> {
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let part = dest.with_extension("part");
    let res = reqwest::Client::new().get(url).send().await?;
    if !res.status().is_success() {
        return Err(anyhow!("Download failed: HTTP {}", res.status()));
    }
    let total = res.content_length();
    let mut file = tokio::fs::File::create(&part).await?;
    let mut done = 0u64;
    let mut last_report = 0u64;
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        done += chunk.len() as u64;
        if done - last_report > 512 * 1024 {
            last_report = done;
            on_progress(done, total);
        }
    }
    file.flush().await?;
    drop(file);
    if let Some(t) = total {
        if done != t {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(anyhow!("Download incomplete ({done} of {t} bytes)"));
        }
    }
    tokio::fs::rename(&part, dest).await?;
    on_progress(done, total);
    Ok(())
}
