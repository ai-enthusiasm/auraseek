use tauri::State;
use crate::app::state::AppState;
use crate::core::models::TimelineGroup;
use crate::infrastructure::database::DbOperations;

#[tauri::command]
pub async fn cmd_toggle_favorite(media_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::toggle_favorite(db, &media_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_get_timeline(limit: Option<usize>, state: State<'_, AppState>) -> Result<Vec<TimelineGroup>, String> {
    let source_dir = state.source_dir.lock().await.clone();
    let guard      = state.sqlite.lock().unwrap();
    let db         = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::get_timeline(db, limit.unwrap_or(5000), &source_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_move_to_trash(media_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let source_dir = state.source_dir.lock().await.clone();
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::move_to_trash(db, &source_dir, &media_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_restore_from_trash(media_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let source_dir = state.source_dir.lock().await.clone();
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::restore_from_trash(db, &source_dir, &media_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_get_trash(state: State<'_, AppState>) -> Result<Vec<TimelineGroup>, String> {
    let source_dir = state.source_dir.lock().await.clone();
    let guard      = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::get_trash(db, &source_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_empty_trash(state: State<'_, AppState>) -> Result<(), String> {
    let source_dir = state.source_dir.lock().await.clone();
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::empty_trash(db, &source_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_hard_delete_trash_item(media_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let source_dir = state.source_dir.lock().await.clone();
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::hard_delete_trash_item(db, &source_dir, &media_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_hide_photo(media_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::hide_photo(db, &media_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_unhide_photo(media_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::unhide_photo(db, &media_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_get_hidden_photos(state: State<'_, AppState>) -> Result<Vec<TimelineGroup>, String> {
    let source_dir = state.source_dir.lock().await.clone();
    let guard      = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::get_hidden_photos(db, &source_dir).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cmd_get_timeline_page(
    offset: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<crate::core::models::TimelinePageResponse, String> {
    let source_dir = state.source_dir.lock().await.clone();
    let guard = state.sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or("DB not initialized")?;

    let (items, total) = DbOperations::get_timeline_page(db, offset, limit, &source_dir)
        .map_err(|e| e.to_string())?;

    Ok(crate::core::models::TimelinePageResponse { items, total, offset, limit })
}

#[tauri::command]
pub async fn cmd_generate_missing_thumbnails(state: State<'_, AppState>) -> Result<usize, String> {
    let source_dir = state.source_dir.lock().await.clone();
    let cache_dir = state.data_dir.lock().unwrap().join("thumbnails");

    // 1. Get the list of media items needing thumbnails while holding the lock briefly
    let items = {
        let guard = state.sqlite.lock().unwrap();
        let db = guard.as_ref().ok_or("DB not initialized")?;
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM media
             WHERE thumbnail IS NULL AND deleted_at IS NULL AND media_type = 'image'"
        ).map_err(|e| e.to_string())?;

        let rows: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
        rows
    }; // Lock is released here!

    if items.is_empty() {
        return Ok(0);
    }

    let base = source_dir.trim_end_matches('/');
    let mut count = 0usize;

    // 2. Loop over items and generate thumbnails without holding the main DB lock
    for (media_id, file_name) in &items {
        let full_path = std::path::Path::new(base).join(file_name);
        if !full_path.exists() {
            continue;
        }

        // Generate the thumbnail (this is the cpu-heavy part, runs without lock)
        match crate::infrastructure::ingest::generate_thumbnail(&full_path, &cache_dir, media_id) {
            Ok(thumb_path) => {
                // Lock DB briefly to update the record
                let guard = state.sqlite.lock().unwrap();
                if let Some(ref db) = *guard {
                    let conn = db.conn();
                    let _ = conn.execute(
                        "UPDATE media SET thumbnail = ?2 WHERE id = ?1",
                        rusqlite::params![media_id, thumb_path],
                    );
                    count += 1;
                }
            }
            Err(e) => {
                crate::log_warn!("⚠️ Thumbnail gen failed for {}: {}", file_name, e);
            }
        }
    }

    crate::log_info!("🖼️ Generated {} missing thumbnails", count);
    Ok(count)
}
