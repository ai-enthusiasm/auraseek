use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::Client;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use tokio::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::config::{AI_ASSETS, AI_ASSETS_BASE_URL};
use crate::infrastructure::database::QdrantService;

static IS_DOWNLOADING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub file: String,
    pub progress: f32,
    pub message: String,
    pub done: bool,
    pub error: String,
    pub file_index: usize,
    pub file_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

pub fn all_models_present(data_dir: &Path) -> bool {
    let models_ok = AI_ASSETS.iter().all(|(_, rel)| data_dir.join(rel).exists());
    let dashboard_enabled = crate::core::config::AppConfig::global().qdrant_dashboard_enabled;
    let qdrant_ok = QdrantService::assets_present(data_dir, dashboard_enabled);
    models_ok && qdrant_ok
}

pub struct ModelDownloader;

impl ModelDownloader {
    pub async fn download_if_missing(app: &AppHandle, data_dir: &Path) -> Result<()> {
        if IS_DOWNLOADING.swap(true, Ordering::SeqCst) {
            crate::log_info!("Download already in progress, skipping duplicate request.");
            return Ok(());
        }

        let res = Self::download_internal(app, data_dir).await;
        IS_DOWNLOADING.store(false, Ordering::SeqCst);
        res
    }

    async fn download_internal(app: &AppHandle, data_dir: &Path) -> Result<()> {
        let _ = app.emit("model-download-progress", DownloadProgress {
            file: "qdrant".to_string(),
            progress: 0.0,
            message: "Đang tải Qdrant vector database...".to_string(),
            done: false, error: String::new(),
            file_index: 0, file_total: 1,
            bytes_done: 0, bytes_total: 0,
        });

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
            let _ = app.emit("model-download-progress", DownloadProgress {
                file: "done".into(),
                progress: 1.0,
                message: "Sẵn sàng khởi động AI Engine...".into(),
                done: true,
                error: String::new(),
                file_index: 0,
                file_total: 0,
                bytes_done: 0,
                bytes_total: 0,
            });
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

            let _ = app.emit("model-download-progress", DownloadProgress {
                file: name.to_string(),
                progress: 0.0,
                message: format!("Đang tải {} ({}/{})", name, file_index, total),
                done: false, error: String::new(),
                file_index, file_total: total,
                bytes_done: 0, bytes_total: 0,
            });

            let res = client.get(&url).send().await
                .with_context(|| format!("connect to {}", url))?;

            if !res.status().is_success() {
                let msg = format!("HTTP {} for {}", res.status(), name);
                let _ = app.emit("model-download-progress", DownloadProgress {
                    file: name.to_string(), progress: 0.0,
                    message: msg.clone(), done: false, error: msg.clone(),
                    file_index, file_total: total, bytes_done: 0, bytes_total: 0,
                });
                anyhow::bail!("{}", msg);
            }

            let bytes_total = res.content_length().unwrap_or(0);
            let mut bytes_done: u64 = 0;
            let mut stream = res.bytes_stream();

            let tmp = dest.with_extension("tmp");
            let mut file = tokio::fs::File::create(&tmp).await
                .with_context(|| format!("create {}", tmp.display()))?;

            let mut last_emit = std::time::Instant::now();

            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("read chunk")?;
                tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await
                    .context("write chunk")?;
                bytes_done += chunk.len() as u64;

                if last_emit.elapsed().as_millis() > 100 {
                    last_emit = std::time::Instant::now();
                    let progress = if bytes_total > 0 {
                        bytes_done as f32 / bytes_total as f32
                    } else { 0.0 };
                    let _ = app.emit("model-download-progress", DownloadProgress {
                        file: name.to_string(), progress,
                        message: format!("Đang tải {} ({}/{})", name, file_index, total),
                        done: false, error: String::new(),
                        file_index, file_total: total,
                        bytes_done, bytes_total,
                    });
                }
            }

            tokio::io::AsyncWriteExt::flush(&mut file).await.context("flush")?;
            drop(file);
            fs::rename(&tmp, dest).await
                .with_context(|| format!("rename to {}", dest.display()))?;

            crate::log_info!("✅ Done: {}", name);

            let _ = app.emit("model-download-progress", DownloadProgress {
                file: name.to_string(), progress: 1.0,
                message: format!("Đã tải xong {} ({}/{})", name, file_index, total),
                done: false, error: String::new(),
                file_index, file_total: total,
                bytes_done, bytes_total,
            });
        }

        let face_db = data_dir.join("face_db");
        if !face_db.exists() {
            fs::create_dir_all(&face_db).await?;
        }

        let _ = app.emit("model-download-progress", DownloadProgress {
            file: "done".into(), progress: 1.0,
            message: "Sẵn sàng khởi động AI Engine...".into(),
            done: true, error: String::new(),
            file_index: total, file_total: total,
            bytes_done: 0, bytes_total: 0,
        });

        crate::log_info!("✅ All model downloads complete");
        Ok(())
    }
}
