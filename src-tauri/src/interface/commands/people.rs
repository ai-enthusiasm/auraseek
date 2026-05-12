use tauri::State;
use crate::app::state::AppState;
use crate::core::models::PersonGroup;
use crate::infrastructure::database::DbOperations;

#[tauri::command]
pub async fn cmd_get_people(state: State<'_, AppState>) -> Result<Vec<PersonGroup>, String> {
    let source_dir = state.source_dir.lock().await.clone();
    let guard      = state.sqlite.lock().unwrap();
    let db         = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::get_people(db, &source_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_name_person(face_id: String, name: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::name_person(db, &face_id, &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_merge_people(target_face_id: String, source_face_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::merge_people(db, &target_face_id, &source_face_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_delete_person(face_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::delete_person(db, &face_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_remove_face_from_person(media_id: String, face_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::remove_face_from_person(db, &media_id, &face_id).map_err(|e| e.to_string())
}
