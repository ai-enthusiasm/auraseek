mod utils;
mod model;
mod processor;
mod db;
mod ingest;
mod search;

use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::Result;
use tauri::State;

use processor::AuraSeekEngine;
use db::{MongoDb, VectorStore, DbOperations};
use db::models::{SearchResult, TimelineGroup, PersonGroup, DuplicateGroup};
use ingest::image_ingest::{ingest_folder, IngestSummary};
use search::pipeline::{SearchPipeline, SearchQuery};
use utils::logger::Logger;

// ─── App State ───────────────────────────────────────────────────────────────

pub struct AppState {
    pub engine:       Arc<Mutex<Option<AuraSeekEngine>>>,
    pub db:           Arc<Mutex<Option<MongoDb>>>,
    pub vector_store: Arc<VectorStore>,
    pub mongo_uri:    Mutex<String>,
}

impl AppState {
    fn new() -> Self {
        Self {
            engine: Arc::new(Mutex::new(None)),
            db: Arc::new(Mutex::new(None)),
            vector_store: Arc::new(VectorStore::new()),
            mongo_uri: Mutex::new("mongodb://localhost:27017".to_string()),
        }
    }
}

// ─── Tauri Commands ──────────────────────────────────────────────────────────

/// Initialize AI engine and database connection.
#[tauri::command]
async fn cmd_init(state: State<'_, AppState>) -> Result<String, String> {
    // Init engine
    {
        let mut engine_guard = state.engine.lock().await;
        if engine_guard.is_none() {
            match AuraSeekEngine::new_default() {
                Ok(e) => { *engine_guard = Some(e); }
                Err(e) => return Err(format!("Engine init failed: {}", e)),
            }
        }
    }

    // Init DB
    {
        let uri = state.mongo_uri.lock().await.clone();
        let mut db_guard = state.db.lock().await;
        if db_guard.is_none() {
            match MongoDb::connect(&uri).await {
                Ok(db) => {
                    // Load vector store
                    let _ = DbOperations::load_vector_store(&db, &state.vector_store).await;
                    *db_guard = Some(db);
                }
                Err(e) => return Err(format!("DB connection failed: {}", e)),
            }
        }
    }

    Ok(format!("Ready. Vectors loaded: {}", state.vector_store.len()))
}

/// Scan a folder for images/videos and run AI pipeline.
#[tauri::command]
async fn cmd_scan_folder(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<IngestSummary, String> {
    let engine_arc = state.engine.clone();
    let vector_store = state.vector_store.clone();

    let db_guard = state.db.lock().await;
    let db = db_guard.as_ref()
        .ok_or_else(|| "Database not initialized. Call cmd_init first.".to_string())?;
    let db_arc = Arc::new(MongoDb { db: db.db.clone() });
    drop(db_guard);

    ingest_folder(source_path, db_arc, engine_arc, vector_store, None)
        .await
        .map_err(|e| e.to_string())
}

/// Search by text query.
#[tauri::command]
async fn cmd_search_text(
    query: String,
    filters: Option<search::pipeline::SearchQueryFilters>,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    let search_query = SearchQuery {
        mode: search::pipeline::SearchMode::Text,
        text: Some(query.clone()),
        image_path: None,
        filters: filters.unwrap_or_default(),
    };
    run_search(search_query, Some(query), None, &state).await
}

/// Search by uploaded image.
#[tauri::command]
async fn cmd_search_image(
    image_path: String,
    filters: Option<search::pipeline::SearchQueryFilters>,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    let search_query = SearchQuery {
        mode: search::pipeline::SearchMode::Image,
        text: None,
        image_path: Some(image_path.clone()),
        filters: filters.unwrap_or_default(),
    };
    run_search(search_query, None, Some(image_path), &state).await
}

/// Combined text + image search (returns intersection).
#[tauri::command]
async fn cmd_search_combined(
    text: String,
    image_path: String,
    filters: Option<search::pipeline::SearchQueryFilters>,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    let search_query = SearchQuery {
        mode: search::pipeline::SearchMode::Combined,
        text: Some(text.clone()),
        image_path: Some(image_path.clone()),
        filters: filters.unwrap_or_default(),
    };
    run_search(search_query, Some(text), Some(image_path), &state).await
}

/// Search by COCO object class name.
#[tauri::command]
async fn cmd_search_object(
    class_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    let search_query = SearchQuery {
        mode: search::pipeline::SearchMode::ObjectFilter,
        text: None,
        image_path: None,
        filters: search::pipeline::SearchQueryFilters {
            object: Some(class_name),
            ..Default::default()
        },
    };
    run_search(search_query, None, None, &state).await
}

/// Search by face name (person).
#[tauri::command]
async fn cmd_search_face(
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    let search_query = SearchQuery {
        mode: search::pipeline::SearchMode::FaceFilter,
        text: None,
        image_path: None,
        filters: search::pipeline::SearchQueryFilters {
            face: Some(name),
            ..Default::default()
        },
    };
    run_search(search_query, None, None, &state).await
}

/// Get timeline grouped by month (most recent first).
#[tauri::command]
async fn cmd_get_timeline(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<TimelineGroup>, String> {
    let db_guard = state.db.lock().await;
    let db = db_guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::get_timeline(db, limit.unwrap_or(5000))
        .await
        .map_err(|e| e.to_string())
}

/// Get all recognized people / face clusters.
#[tauri::command]
async fn cmd_get_people(
    state: State<'_, AppState>,
) -> Result<Vec<PersonGroup>, String> {
    let db_guard = state.db.lock().await;
    let db = db_guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::get_people(db).await.map_err(|e| e.to_string())
}

/// Name a face cluster.
#[tauri::command]
async fn cmd_name_person(
    face_id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_guard = state.db.lock().await;
    let db = db_guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::name_person(db, &face_id, &name)
        .await
        .map_err(|e| e.to_string())
}

/// Find duplicate images (by SHA-256).
#[tauri::command]
async fn cmd_get_duplicates(
    state: State<'_, AppState>,
) -> Result<Vec<DuplicateGroup>, String> {
    let db_guard = state.db.lock().await;
    let db = db_guard.as_ref().ok_or("DB not initialized")?;
    DbOperations::get_duplicates(db).await.map_err(|e| e.to_string())
}

/// Get search history (most recent first).
#[tauri::command]
async fn cmd_get_search_history(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let db_guard = state.db.lock().await;
    let db = db_guard.as_ref().ok_or("DB not initialized")?;
    let history = DbOperations::get_search_history(db, limit.unwrap_or(20))
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(history).unwrap_or_default())
}

/// Set MongoDB connection URI.
#[tauri::command]
async fn cmd_set_mongo_uri(
    uri: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut u = state.mongo_uri.lock().await;
    *u = uri;
    Ok(())
}

/// Get engine + DB status info.
#[tauri::command]
async fn cmd_get_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let engine_ready = state.engine.lock().await.is_some();
    let db_ready = state.db.lock().await.is_some();
    let vector_count = state.vector_store.len();
    Ok(serde_json::json!({
        "engine_ready": engine_ready,
        "db_ready": db_ready,
        "vector_count": vector_count,
    }))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn run_search(
    query: SearchQuery,
    text: Option<String>,
    image_path: Option<String>,
    state: &State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    let db_guard = state.db.lock().await;
    let db = db_guard.as_ref().ok_or("DB not initialized. Call cmd_init first.")?;

    let mut engine_guard = state.engine.lock().await;
    let engine = engine_guard.as_mut().ok_or("Engine not initialized. Call cmd_init first.")?;

    let results = SearchPipeline::run(&query, engine, &state.vector_store, db)
        .await
        .map_err(|e| e.to_string())?;

    // Save search history (fire and forget)
    let _ = DbOperations::save_search_history(db, text, image_path, None).await;

    Ok(results)
}

// ─── Main ────────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            cmd_init,
            cmd_scan_folder,
            cmd_search_text,
            cmd_search_image,
            cmd_search_combined,
            cmd_search_object,
            cmd_search_face,
            cmd_get_timeline,
            cmd_get_people,
            cmd_name_person,
            cmd_get_duplicates,
            cmd_get_search_history,
            cmd_set_mongo_uri,
            cmd_get_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() -> Result<()> {
    Logger::init("log/auraseek.log");
    run();
    Ok(())
}

// Needed for AppState to be Send + Sync
struct MongoDbWrapper(MongoDb);
unsafe impl Send for MongoDbWrapper {}
unsafe impl Sync for MongoDbWrapper {}
