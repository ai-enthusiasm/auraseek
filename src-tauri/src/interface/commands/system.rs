use tauri::State;
use crate::app::state::AppState;
use crate::infrastructure::database::DbOperations;

#[tauri::command]
pub fn cmd_get_device_name() -> Result<String, String> {
    let name = sysinfo::System::host_name()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "Thiết bị này".to_string());
    Ok(name)
}

#[tauri::command]
pub fn cmd_get_file_size(path: String) -> Result<u64, String> {
    let meta = std::fs::metadata(std::path::Path::new(&path))
        .map_err(|e| format!("Không đọc được thông tin file: {}", e))?;
    Ok(meta.len())
}

#[tauri::command]
pub async fn cmd_authenticate_os() -> Result<bool, String> {
    crate::platform::auth::authenticate_os()
}

#[tauri::command]
pub async fn cmd_get_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let (engine_ready, face_model_loaded) = {
        let guard = state.engine.lock().await;
        let ready = guard.is_some();
        let face_loaded = guard.as_ref().map(|e| e.face.is_some()).unwrap_or(false);
        (ready, face_loaded)
    };
    let db_ready = state.sqlite.lock().unwrap().is_some();
    let vector_count = {
        let guard = state.qdrant_client.lock().await;
        if let Some(ref client) = *guard {
            let config = crate::core::config::AppConfig::global();
            DbOperations::embedding_count(client, &config.qdrant_collection).await.unwrap_or(0)
        } else { 0 }
    };
    let (people_count, media_with_faces) = {
        let guard = state.sqlite.lock().unwrap();
        if let Some(ref db) = *guard {
            let conn = db.conn();
            let pc: u64 = conn.query_row("SELECT COUNT(*) FROM person", [], |r| r.get(0)).unwrap_or(0);
            let mc: u64 = conn.query_row("SELECT COUNT(DISTINCT media_id) FROM media_faces", [], |r| r.get(0)).unwrap_or(0);
            (pc, mc)
        } else {
            (0, 0)
        }
    };
    let source_dir = state.source_dir.lock().await.clone();
    let log_path = crate::shared::logging::Logger::active_log_path()
        .unwrap_or_else(|| crate::core::config::AppConfig::global().log_path.to_string_lossy().to_string());
    let qdrant_grpc_port = state
        .qdrant_runtime_grpc_port
        .load(std::sync::atomic::Ordering::SeqCst);
    let qdrant_http_port = state
        .qdrant_runtime_http_port
        .load(std::sync::atomic::Ordering::SeqCst);
    Ok(serde_json::json!({
        "engine_ready": engine_ready,
        "face_model_loaded": face_model_loaded,
        "db_ready": db_ready,
        "vector_count": vector_count,
        "people_count": people_count,
        "media_with_faces": media_with_faces,
        "source_dir": source_dir,
        "log_path": log_path,
        "qdrant_grpc_port": if qdrant_grpc_port == 0 { serde_json::Value::Null } else { serde_json::json!(qdrant_grpc_port) },
        "qdrant_http_port": if qdrant_http_port == 0 { serde_json::Value::Null } else { serde_json::json!(qdrant_http_port) }
    }))
}
