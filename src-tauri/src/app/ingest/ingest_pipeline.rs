use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::Emitter;
use futures::StreamExt;

use crate::infrastructure::database::{SqliteDb, DbOperations};
use crate::infrastructure::ai::AuraSeekEngine;
use crate::infrastructure::ingest::image_processor::{
    collect_files, scan_single_file, process_image_file,
    IMAGE_EXTENSIONS, VIDEO_EXTENSIONS,
};
use crate::infrastructure::ingest::video_processor;
use crate::core::models::{IngestSummary, IngestProgress};

pub async fn ingest_folder(
    source_dir: String,
    sqlite: Arc<std::sync::Mutex<Option<SqliteDb>>>,
    qdrant: Arc<Mutex<Option<qdrant_client::Qdrant>>>,
    engine: Arc<Mutex<Option<AuraSeekEngine>>>,
    app: Option<tauri::AppHandle>,
    thumb_cache_dir: Option<PathBuf>,
    abort_sync: Arc<std::sync::atomic::AtomicBool>,
    library_epoch: Arc<AtomicU64>,
    epoch_at_invoke: u64,
) -> Result<IngestSummary> {
    if library_epoch.load(Ordering::SeqCst) != epoch_at_invoke {
        crate::log_info!("🛑 ingest_folder skipped (library reset / stale epoch)");
        return Ok(IngestSummary {
            total_found: 0,
            newly_added: 0,
            skipped_dup: 0,
            errors: 0,
        });
    }
    abort_sync.store(false, std::sync::atomic::Ordering::SeqCst);

    let source_path = Path::new(&source_dir);
    if !source_path.exists() {
        crate::log_error!("Source directory not found: {}", source_dir);
        return Err(anyhow::anyhow!("Source directory not found: {}", source_dir));
    }

    {
        let guard = sqlite.lock().unwrap();
        if let Some(ref db) = *guard {
            if let Err(e) = DbOperations::set_source_dir(db, &source_dir) {
                crate::log_warn!("⚠️ Failed to persist source_dir to config: {}", e);
            } else {
                crate::log_info!("📝 source_dir saved to config_auraseek: {}", source_dir);
            }
        }
    }

    let (image_files, video_files) = collect_files(source_path);
    let total = image_files.len() + video_files.len();
    crate::log_info!(
        "📂 Ingest started: {} | {} images + {} videos found",
        source_dir, image_files.len(), video_files.len()
    );

    let mut summary = IngestSummary {
        total_found: total,
        newly_added: 0,
        skipped_dup: 0,
        errors: 0,
    };

    // ── Phase 1: Pre-load processed files into memory (1 DB query) ──────
    // This replaces N individual DB queries during the scan loop.
    let processed_set: std::collections::HashSet<(String, i64, Option<String>)> = {
        let guard = sqlite.lock().unwrap();
        if let Some(ref db) = *guard {
            let conn = db.conn();
            if let Ok(mut stmt) = conn.prepare(
                "SELECT file_name, file_size, meta_modified_at FROM media WHERE processed = 1"
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                }) {
                    rows.filter_map(|r| r.ok()).collect()
                } else {
                    std::collections::HashSet::new()
                }
            } else {
                std::collections::HashSet::new()
            }
        } else {
            std::collections::HashSet::new()
        }
    };
    crate::log_info!("📋 Pre-loaded {} processed file entries into memory", processed_set.len());

    // ── Phase 2: Batch scan — classify files as new/skip using in-memory set ──
    // Files that pass the in-memory check still go through scan_single_file for DB registration.
    let all_files: Vec<(PathBuf, bool, &str)> = image_files
        .into_iter()
        .map(|path| (path, false, "image"))
        .chain(video_files.into_iter().map(|path| (path, true, "video")))
        .collect();

    // Fast filter: skip files already in processed_set (no DB query needed)
    let mut to_process: Vec<(PathBuf, bool, String)> = Vec::new();
    for (path, is_video, media_type) in &all_files {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if let Ok(meta) = std::fs::metadata(path) {
            let key = (
                name,
                meta.len() as i64,
                crate::infrastructure::ingest::image_processor::file_modified_at_public(&meta),
            );
            if processed_set.contains(&key) {
                summary.skipped_dup += 1;
                continue;
            }
        }
        to_process.push((path.clone(), *is_video, media_type.to_string()));
    }
    crate::log_info!(
        "⚡ Fast filter: {} skipped (already processed), {} to process",
        summary.skipped_dup, to_process.len()
    );

    // ── Phase 3: Unified Concurrent Scanning & AI Ingestion Streaming ────
    let total_to_process = to_process.len();
    crate::log_info!("⚙️ Starting parallel ingestion pipeline for {} files...", total_to_process);

    // Scale up AI engine threads based on available RAM/CPU for fast ingestion
    if total_to_process > 0 {
        let safe_threads = crate::core::config::compute_safe_threads();
        crate::log_info!("🚀 Reinitializing AI engine with {} threads for ingest pipeline", safe_threads);
        {
            let mut engine_guard = engine.lock().await;
            let data_dir = if let Some(ref app) = app {
                use tauri::Manager;
                let state = app.state::<crate::app::state::AppState>();
                let guard = state.data_dir.lock().unwrap();
                guard.clone()
            } else {
                std::path::PathBuf::from("assets")
            };
            let config = crate::infrastructure::ai::engine::EngineConfig::new_with_dir(&data_dir);
            match AuraSeekEngine::new_with_threads(config, safe_threads) {
                Ok(e) => { *engine_guard = Some(e); }
                Err(e) => { crate::log_error!("⚠️ Failed to scale up engine threads: {}", e); }
            }
        }
    }

    struct EngineRestoreGuard {
        engine: Arc<Mutex<Option<AuraSeekEngine>>>,
        app: Option<tauri::AppHandle>,
    }

    impl Drop for EngineRestoreGuard {
        fn drop(&mut self) {
            let engine = self.engine.clone();
            let app = self.app.clone();
            tokio::spawn(async move {
                crate::log_info!("🚀 Restoring AI engine to 1 thread for idle/search state");
                let mut engine_guard = engine.lock().await;
                let data_dir = if let Some(ref app) = app {
                    use tauri::Manager;
                    let state = app.state::<crate::app::state::AppState>();
                    let guard = state.data_dir.lock().unwrap();
                    guard.clone()
                } else {
                    std::path::PathBuf::from("assets")
                };
                let config = crate::infrastructure::ai::engine::EngineConfig::new_with_dir(&data_dir);
                match AuraSeekEngine::new_with_threads(config, 1) {
                    Ok(e) => { *engine_guard = Some(e); }
                    Err(e) => { crate::log_error!("⚠️ Failed to restore engine threads: {}", e); }
                }
            });
        }
    }

    let _restore_guard = if total_to_process > 0 {
        Some(EngineRestoreGuard {
            engine: engine.clone(),
            app: app.clone(),
        })
    } else {
        None
    };

    // Shared thread-safe counters
    let newly_added_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let skipped_dup_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let errors_counter      = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let processed_counter   = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let safe_threads = crate::core::config::compute_safe_threads();
    let concurrency_limit = safe_threads.max(2);
    crate::log_info!("⚙️ Ingestion concurrency limit set to: {}", concurrency_limit);

    let app_handle = app.clone();
    let abort_flag = abort_sync.clone();
    let library_epoch_shared = library_epoch.clone();
    let thumb_cache_shared = thumb_cache_dir.clone();
    let sqlite_shared = sqlite.clone();
    let qdrant_shared = qdrant.clone();
    let engine_shared = engine.clone();
    let source_dir_shared = Arc::new(source_dir.clone());

    let mut stream = futures::stream::iter(to_process)
        .map(|(path, is_video, media_type)| {
            let app_handle = app_handle.clone();
            let abort_flag = abort_flag.clone();
            let library_epoch = library_epoch_shared.clone();
            let thumb_cache_dir = thumb_cache_shared.clone();
            let sqlite = sqlite_shared.clone();
            let qdrant = qdrant_shared.clone();
            let engine = engine_shared.clone();
            let source_dir = source_dir_shared.clone();
            let newly_added = newly_added_counter.clone();
            let skipped_dup = skipped_dup_counter.clone();
            let errors      = errors_counter.clone();
            let processed   = processed_counter.clone();

            tokio::spawn(async move {
                if abort_flag.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                if library_epoch.load(Ordering::SeqCst) != epoch_at_invoke {
                    return;
                }

                let file_name_only = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                check_system_congestion_and_throttle();

                // 1. Scan/register the file in the database (hashing is parallel outside DB lock)
                let scan_res = scan_single_file(&path, &sqlite, &source_dir, &media_type);

                match scan_res {
                    Ok(Some(media_id)) => {
                        newly_added.fetch_add(1, Ordering::SeqCst);
                        
                        // 2. Perform AI Ingestion
                        let path_str = path.to_string_lossy().to_string();
                        crate::log_info!("🤖 Ingesting: {}", file_name_only);

                        if is_video {
                            let cache_ref = thumb_cache_dir.as_deref();
                            match video_processor::process_video(&path_str, &media_id, &sqlite, &qdrant, &engine, cache_ref).await {
                                Ok(Some(thumb)) => crate::log_info!("🎥 Video done, thumbnail: {}", thumb),
                                Ok(None)        => crate::log_info!("🎥 Video done (no thumbnail)"),
                                Err(e)          => crate::log_warn!("🎥 Video pipeline error for {}: {}", file_name_only, e),
                            }
                        } else {
                            let cache_ref = thumb_cache_dir.as_deref();
                            process_image_file(&path_str, &media_id, &file_name_only, &sqlite, &qdrant, &engine, cache_ref).await;
                        }
                    }
                    Ok(None) => {
                        skipped_dup.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(e) => {
                        crate::log_warn!("⚠️ Scan error {:?}: {}", path, e);
                        errors.fetch_add(1, Ordering::SeqCst);
                    }
                }

                // Increment progress and emit event
                let completed = processed.fetch_add(1, Ordering::SeqCst) + 1;
                if let Some(ref app) = app_handle {
                    let _ = app.emit(
                        "ingest-progress",
                        &IngestProgress {
                            processed: completed,
                            total: total_to_process,
                            current_file: file_name_only,
                        },
                    );
                }
            })
        })
        .buffer_unordered(concurrency_limit);

    // Consume the stream
    while let Some(res) = stream.next().await {
        if abort_flag.load(std::sync::atomic::Ordering::SeqCst) {
            crate::log_info!("🛑 Ingestion loop aborted");
            break;
        }
        if library_epoch.load(Ordering::SeqCst) != epoch_at_invoke {
            crate::log_info!("🛑 Ingestion loop stopped (library reset)");
            break;
        }
        if let Err(e) = res {
            crate::log_error!("⚠️ Task join error: {}", e);
        }
    }

    summary.newly_added = newly_added_counter.load(Ordering::SeqCst);
    summary.skipped_dup = skipped_dup_counter.load(Ordering::SeqCst) + (total - total_to_process);
    summary.errors = errors_counter.load(Ordering::SeqCst);

    crate::log_info!("✅ Ingest complete: {} new, {} skipped, {} errors",
        summary.newly_added, summary.skipped_dup, summary.errors);

    Ok(summary)
}

pub async fn ingest_files(
    file_paths: Vec<String>,
    dest_dir: String,
    sqlite: Arc<std::sync::Mutex<Option<SqliteDb>>>,
    qdrant: Arc<Mutex<Option<qdrant_client::Qdrant>>>,
    engine: Arc<Mutex<Option<AuraSeekEngine>>>,
    thumb_cache_dir: Option<PathBuf>,
) -> Result<IngestSummary> {
    let dest_path = Path::new(&dest_dir);
    if !dest_path.exists() {
        return Err(anyhow::anyhow!("Destination directory not found: {}", dest_dir));
    }

    let mut summary = IngestSummary {
        total_found: file_paths.len(),
        newly_added: 0,
        skipped_dup: 0,
        errors: 0,
    };

    for src_path_str in &file_paths {
        check_system_congestion_and_throttle();

        let src = Path::new(src_path_str);
        let file_name = match src.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => { summary.errors += 1; continue; }
        };

        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let is_video = VIDEO_EXTENSIONS.contains(&ext.as_str());
        let is_image = IMAGE_EXTENSIONS.contains(&ext.as_str());
        if !is_image && !is_video {
            crate::log_warn!("⚠️ Skipping unsupported file: {}", file_name);
            summary.skipped_dup += 1;
            continue;
        }
        let media_type = if is_video { "video" } else { "image" };

        let dest = dest_path.join(&file_name);
        if src.canonicalize().ok() != dest.canonicalize().ok() {
            if let Err(e) = std::fs::copy(src, &dest) {
                crate::log_warn!("⚠️ Failed to copy {} -> {}: {}", src_path_str, dest.display(), e);
                summary.errors += 1;
                continue;
            }
        }

        let scan_result = scan_single_file(&dest, &sqlite, &dest_dir, media_type);

        match scan_result {
            Ok(Some(media_id)) => {
                crate::log_info!("📎 Copied+ingested: {} ({}) as {}", file_name, media_id, media_type);
                summary.newly_added += 1;

                let dest_str = dest.to_string_lossy().to_string();

                if is_video {
                    let cache_ref = thumb_cache_dir.as_deref();
                    if let Err(e) = video_processor::process_video(&dest_str, &media_id, &sqlite, &qdrant, &engine, cache_ref).await {
                        crate::log_warn!("🎥 Video pipeline error for {}: {}", file_name, e);
                    }
                } else {
                    let cache_ref = thumb_cache_dir.as_deref();
                    process_image_file(&dest_str, &media_id, &file_name, &sqlite, &qdrant, &engine, cache_ref).await;
                }
            }
            Ok(None) => { summary.skipped_dup += 1; }
            Err(e) => {
                crate::log_warn!("⚠️ Error ingesting {}: {}", file_name, e);
                summary.errors += 1;
            }
        }
    }

    Ok(summary)
}

fn check_system_congestion_and_throttle() {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();

    let free_pct = crate::app::helpers::available_ram_percent();
    let cpu_load = sys.global_cpu_usage();

    if free_pct < 10.0 || cpu_load > 90.0 {
        crate::log_warn!(
            "⚠️ System resource pressure detected! Available RAM: {:.1}%, CPU Load: {:.1}%. Throttling ingestion...",
            free_pct, cpu_load
        );
        std::thread::sleep(std::time::Duration::from_millis(1500));
    } else if free_pct < 20.0 || cpu_load > 75.0 {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
