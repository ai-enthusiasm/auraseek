use tauri::State;
use crate::app::state::AppState;
use crate::infrastructure::database::DbOperations;
use surrealdb::types::SurrealValue;

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
pub async fn cmd_set_db_config(addr: String, user: String, pass: String, state: State<'_, AppState>) -> Result<(), String> {
    *state.surreal_addr.lock().map_err(|e| e.to_string())? = addr;
    *state.surreal_user.lock().map_err(|e| e.to_string())? = user;
    *state.surreal_pass.lock().map_err(|e| e.to_string())? = pass;
    *state.db.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn cmd_get_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let (engine_ready, face_model_loaded) = {
        let guard = state.engine.lock().await;
        let ready = guard.is_some();
        let face_loaded = guard.as_ref().map(|e| e.face.is_some()).unwrap_or(false);
        (ready, face_loaded)
    };
    let db_ready     = state.db.lock().await.is_some();
    let vector_count = {
        let db_guard = state.db.lock().await;
        if let Some(ref sdb) = *db_guard { DbOperations::embedding_count(sdb).await.unwrap_or(0) } else { 0 }
    };
    let (people_count, media_with_faces) = {
        let db_guard = state.db.lock().await;
        if let Some(ref sdb) = *db_guard {
            #[derive(serde::Deserialize, SurrealValue)]
            struct CountRow { c: Option<u64> }
            let mut p = sdb.db.query("SELECT count() AS c FROM person GROUP ALL").await
                .map_err(|e| e.to_string())?;
            let mut m = sdb.db.query("SELECT count() AS c FROM media WHERE array::len(faces) > 0 GROUP ALL").await
                .map_err(|e| e.to_string())?;
            let p_rows: Vec<CountRow> = p.take(0).unwrap_or_default();
            let m_rows: Vec<CountRow> = m.take(0).unwrap_or_default();
            (
                p_rows.first().and_then(|r| r.c).unwrap_or(0),
                m_rows.first().and_then(|r| r.c).unwrap_or(0),
            )
        } else {
            (0, 0)
        }
    };
    let source_dir = state.source_dir.lock().await.clone();
    let log_path = crate::shared::logging::Logger::active_log_path()
        .unwrap_or_else(|| crate::core::config::AppConfig::global().log_path.to_string_lossy().to_string());
    Ok(serde_json::json!({
        "engine_ready": engine_ready,
        "face_model_loaded": face_model_loaded,
        "db_ready": db_ready,
        "vector_count": vector_count,
        "people_count": people_count,
        "media_with_faces": media_with_faces,
        "source_dir": source_dir,
        "log_path": log_path
    }))
}
