use crate::app::state::AppState;
use crate::infrastructure::database::QdrantService;

pub fn available_ram_percent() -> f64 {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();

    let total = sys.total_memory() as f64;
    let available = sys.available_memory() as f64;
    let free = sys.free_memory() as f64;
    let used = sys.used_memory() as f64;

    let avail = if available > 0.0 {
        available
    } else if free > 0.0 {
        free
    } else if total > 0.0 {
        (total - used).max(0.0)
    } else {
        0.0
    };

    if total > 0.0 {
        (avail / total) * 100.0
    } else {
        50.0
    }
}

pub fn restart_fs_watcher(state: &AppState, source_dir: &str) {
    if let Ok(mut guard) = state.watcher_handle.lock() {
        if let Some(old) = guard.take() {
            old.stop();
            crate::log_info!("👁️  Previous FS watcher stopped");
        }
    }
    if source_dir.is_empty() { return; }

    let thumb_cache_dir = Some(state.data_dir.lock().unwrap().join("thumbnails"));
    match crate::infrastructure::fs::FileWatcher::start(
        source_dir.to_string(),
        state.sqlite.clone(),
        state.qdrant_client.clone(),
        state.engine.clone(),
        state.sync_status.clone(),
        thumb_cache_dir,
    ) {
        Ok(handle) => {
            if let Ok(mut guard) = state.watcher_handle.lock() { *guard = Some(handle); }
        }
        Err(e) => { crate::log_warn!("⚠️ Failed to start FS watcher: {}", e); }
    }
}

pub async fn start_qdrant_sidecar(app: &tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    use tauri::Emitter;
    let state = app.state::<AppState>();

    {
        let guard = state.qdrant_child.lock().unwrap();
        if guard.is_some() { return Ok(()); }
    }

    let data_dir = state.data_dir.lock().unwrap().clone();
    let config = crate::core::config::AppConfig::global();
    let storage_dir = config.qdrant_storage_dir.clone();
    let grpc_port = config.qdrant_port;
    let http_port = config.qdrant_http_port;
    let dashboard_enabled = config.qdrant_dashboard_enabled;

    if !QdrantService::assets_present(&data_dir, dashboard_enabled) {
        let _ = app.emit("model-download-progress", crate::infrastructure::network::DownloadProgress {
            file: "qdrant".to_string(),
            progress: 0.0,
            message: if dashboard_enabled {
                "Đang tải Qdrant vector database và dashboard...".to_string()
            } else {
                "Đang tải Qdrant vector database...".to_string()
            },
            done: false,
            error: String::new(),
            file_index: 0,
            file_total: 1,
            bytes_done: 0,
            bytes_total: 0,
        });

        QdrantService::download_if_missing(&data_dir, dashboard_enabled)
            .await
            .map_err(|e| format!("Qdrant download failed: {:#}", e))?;

        let _ = app.emit("model-download-progress", crate::infrastructure::network::DownloadProgress {
            file: "qdrant".to_string(),
            progress: 1.0,
            message: if dashboard_enabled {
                "Đã tải xong Qdrant vector database và dashboard".to_string()
            } else {
                "Đã tải xong Qdrant vector database".to_string()
            },
            done: false,
            error: String::new(),
            file_index: 1,
            file_total: 1,
            bytes_done: 0,
            bytes_total: 0,
        });
    }

    match QdrantService::ensure(&data_dir, &storage_dir, grpc_port, http_port, dashboard_enabled) {
        Ok(started) => {
            state.qdrant_runtime_grpc_port
                .store(started.grpc_port, std::sync::atomic::Ordering::SeqCst);
            state.qdrant_runtime_http_port
                .store(started.http_port, std::sync::atomic::Ordering::SeqCst);
            if dashboard_enabled {
                crate::log_info!(
                    "🗄️  Qdrant sidecar started | grpc={} dashboard=http://127.0.0.1:{}/dashboard",
                    started.grpc_port, started.http_port
                );
            } else {
                crate::log_info!("🗄️  Qdrant sidecar started | grpc={} dashboard=disabled", started.grpc_port);
            }
            *state.qdrant_child.lock().unwrap() = Some(started.child);
            Ok(())
        }
        Err(e) => {
            crate::log_warn!("⚠️  Qdrant sidecar start failed: {}", e);
            Err(e.to_string())
        }
    }
}
