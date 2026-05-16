//! Debug Model Downloader — CLI-only model downloading logic.
//!
//! This module is completely separate from production download logic.
//! It can be safely removed when building for production.

use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::fs;

use crate::core::config::{AI_ASSETS, AI_ASSETS_BASE_URL};
use crate::infrastructure::database::QdrantService;

static IS_DOWNLOADING: AtomicBool = AtomicBool::new(false);

pub fn all_models_present(data_dir: &Path) -> bool {
    let models_ok = AI_ASSETS.iter().all(|(_, rel)| data_dir.join(rel).exists());
    let dashboard_enabled = crate::core::config::AppConfig::global().qdrant_dashboard_enabled;
    let qdrant_ok = QdrantService::assets_present(data_dir, dashboard_enabled);
    models_ok && qdrant_ok
}

pub struct DebugModelDownloader;

impl DebugModelDownloader {
    /// Download models for debug CLI usage (no AppHandle needed)
    pub async fn download_if_missing(data_dir: &Path) -> Result<()> {
        if IS_DOWNLOADING.swap(true, Ordering::SeqCst) {
            crate::log_info!("Download already in progress, skipping duplicate request.");
            return Ok(());
        }

        let res = Self::download_internal(data_dir).await;
        IS_DOWNLOADING.store(false, Ordering::SeqCst);
        res
    }

    async fn download_internal(data_dir: &Path) -> Result<()> {
        let dashboard_enabled = crate::core::config::AppConfig::global().qdrant_dashboard_enabled;
        if let Err(e) = QdrantService::download_if_missing(data_dir, dashboard_enabled).await {
            crate::log_error!("❌ Failed to download Qdrant: {}", e);
            return Err(e);
        }

        let needed: Vec<(&str, PathBuf)> = AI_ASSETS
            .iter()
            .filter_map(|(name, rel)| {
                let dest = data_dir.join(rel);
                if dest.exists() { None } else { Some((*name, dest)) }
            })
            .collect();

        if needed.is_empty() {
            crate::log_info!("✅ All model assets already present");
            return Ok(());
        }

        let total = needed.len();
        crate::log_info!("📥 Downloading {} asset(s) to {}", total, data_dir.display());

        let client = Client::new();

        for (i, (name, dest)) in needed.iter().enumerate() {
            let file_index = i + 1;

            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).await
                    .with_context(|| format!("create dir {}", parent.display()))?;
            }

            let url = format!("{}/{}", AI_ASSETS_BASE_URL, name);
            crate::log_info!("📥 [{}/{}] {}", file_index, total, name);

            let res = client.get(&url).send().await
                .with_context(|| format!("connect to {}", url))?;

            if !res.status().is_success() {
                let msg = format!("HTTP {} for {}", res.status(), name);
                crate::log_error!("❌ {}", msg);
                anyhow::bail!("{}", msg);
            }

            let bytes_total = res.content_length().unwrap_or(0);
            let mut bytes_done: u64 = 0;
            let mut stream = res.bytes_stream();

            let tmp = dest.with_extension("tmp");
            let mut file = tokio::fs::File::create(&tmp).await
                .with_context(|| format!("create {}", tmp.display()))?;

            let mut last_log = std::time::Instant::now();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("read chunk")?;
                tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await
                    .context("write chunk")?;
                bytes_done += chunk.len() as u64;

                if last_log.elapsed().as_millis() > 1000 {
                    last_log = std::time::Instant::now();
                    let progress = if bytes_total > 0 {
                        (bytes_done as f32 / bytes_total as f32) * 100.0
                    } else { 0.0 };
                    crate::log_info!("📥 [{}/{}] {}: {:.1}% ({:.1}MB/{:.1}MB)",
                        file_index, total, name, progress,
                        bytes_done as f32 / 1024.0 / 1024.0,
                        bytes_total as f32 / 1024.0 / 1024.0);
                }
            }

            tokio::io::AsyncWriteExt::flush(&mut file).await.context("flush")?;
            drop(file);
            fs::rename(&tmp, dest).await
                .with_context(|| format!("rename to {}", dest.display()))?;

            crate::log_info!("✅ Done: {}", name);
        }

        let face_db = data_dir.join("face_db");
        if !face_db.exists() {
            fs::create_dir_all(&face_db).await?;
        }

        crate::log_info!("✅ All model downloads complete");
        Ok(())
    }
}
