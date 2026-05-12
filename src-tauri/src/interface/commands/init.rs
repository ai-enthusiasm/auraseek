use tauri::State;
use crate::app::state::AppState;
use crate::app::helpers::start_qdrant_sidecar;
use crate::infrastructure::ai::AuraSeekEngine;
use crate::infrastructure::database::{SqliteDb, QdrantService, DbOperations};

#[tauri::command]
pub async fn cmd_check_models(state: State<'_, AppState>) -> Result<bool, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    Ok(crate::infrastructure::network::all_models_present(&data_dir))
}

#[tauri::command]
pub async fn cmd_download_models(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let data_dir = state.data_dir.lock().unwrap().clone();
    tokio::spawn(async move {
        if let Err(e) = crate::infrastructure::network::ModelDownloader::download_if_missing(&app, &data_dir).await {
            crate::log_error!("❌ Model download failed: {:#}", e);
            use tauri::Emitter;
            let _ = app.emit("model-download-progress", crate::infrastructure::network::DownloadProgress {
                file: String::new(), progress: 0.0,
                message: format!("Lỗi tải model: {}", e),
                done: false, error: e.to_string(),
                file_index: 0, file_total: 0,
                bytes_done: 0, bytes_total: 0,
            });
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn cmd_init(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let data_dir = state.data_dir.lock().unwrap().clone();

    {
        let mut engine_guard = state.engine.lock().await;
        if engine_guard.is_none() {
            crate::log_info!("🚀 Initializing AI engine from {}", data_dir.display());
            let _ = std::fs::create_dir_all(data_dir.join("face_db"));
            let config = crate::infrastructure::ai::engine::EngineConfig::new_with_dir(&data_dir);
            match AuraSeekEngine::new(config) {
                Ok(e) => {
                    crate::log_info!("✅ AI engine ready | face_model_loaded={}", e.face.is_some());
                    *engine_guard = Some(e);
                }
                Err(e) => return Err(format!("Engine init failed: {}. Download models first.", e)),
            }
        }
    }

    {
        let mut guard = state.sqlite.lock().unwrap();
        if guard.is_none() {
            let config = crate::core::config::AppConfig::global();
            match SqliteDb::open(&config.sqlite_path) {
                Ok(db) => {
                    crate::log_info!("✅ SQLite database opened: {}", config.sqlite_path.display());
                    *guard = Some(db);
                }
                Err(e) => return Err(format!("SQLite open failed: {}", e)),
            }
        }
    }

    {
        if state.qdrant_child.lock().unwrap().is_none() {
            start_qdrant_sidecar(&app).await?;
        }
        let mut guard = state.qdrant_client.lock().await;
        if guard.is_none() {
            let config = crate::core::config::AppConfig::global();
            let runtime_port = state
                .qdrant_runtime_grpc_port
                .load(std::sync::atomic::Ordering::SeqCst);
            let connect_port = if runtime_port == 0 {
                config.qdrant_port
            } else {
                runtime_port
            };
            match QdrantService::connect_client(connect_port).await {
                Ok(client) => {
                    if let Err(e) = QdrantService::ensure_collection(&client, &config.qdrant_collection, 384).await {
                        crate::log_warn!("⚠️ Failed to ensure Qdrant collection: {}", e);
                    }
                    crate::log_info!("✅ Qdrant client connected on port {}", connect_port);
                    *guard = Some(client);
                }
                Err(e) => return Err(format!("Qdrant connect failed: {}", e)),
            }
        }
    }

    {
        let source_dir_clone = state.source_dir.lock().await.clone();
        let guard = state.sqlite.lock().unwrap();
        if let Some(ref db) = *guard {
            if let Err(e) = DbOperations::auto_purge_trash(db, &source_dir_clone) {
                crate::log_warn!("Failed to auto-purge trash: {}", e);
            }
        }
    }

    {
        let loaded_dir = {
            let guard = state.sqlite.lock().unwrap();
            if let Some(ref db) = *guard {
                match DbOperations::get_source_dir(db) {
                    Ok(Some(dir)) => { crate::log_info!("📂 source_dir loaded from config: {}", dir); Some(dir) }
                    Ok(None) => { crate::log_info!("📂 No source_dir configured yet (first run)"); None }
                    Err(e) => { crate::log_warn!("⚠️ Failed to load source_dir: {}", e); None }
                }
            } else { None }
        };
        if let Some(dir) = loaded_dir {
            *state.source_dir.lock().await = dir;
        }
    }

    let count = {
        let guard = state.qdrant_client.lock().await;
        let config = crate::core::config::AppConfig::global();
        if let Some(ref client) = *guard {
            DbOperations::embedding_count(client, &config.qdrant_collection).await.unwrap_or(0)
        } else { 0 }
    };

    let source_dir = state.source_dir.lock().await.clone();
    crate::log_info!("✅ Init complete | embeddings={} source_dir='{}'", count, source_dir);
    Ok(format!("Ready. Embeddings: {}", count))
}
