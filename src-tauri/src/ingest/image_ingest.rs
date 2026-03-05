/// Image folder ingest pipeline.
/// Thread 1: scan files → insert media stubs (with sha256 dedup)
/// Thread 2: AI processing queue → update with objects/faces/embedding
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use mongodb::bson::DateTime as BsonDateTime;
use sha2::{Sha256, Digest};

use crate::db::{MongoDb, models::{MediaDoc, FileInfo, MediaMetadata, ObjectEntry, FaceEntry, Bbox}, DbOperations};
use crate::db::vector_store::{VectorStore, VectorEntry};
use crate::processor::AuraSeekEngine;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "bmp", "webp", "tiff", "tif", "heic", "avif"];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "mkv", "webm", "m4v", "flv", "wmv"];

/// Scan a source folder and ingest all images/videos.
/// Returns (total_found, newly_added, skipped_duplicates, errors).
pub async fn ingest_folder(
    source_dir: String,
    db: Arc<MongoDb>,
    engine: Arc<Mutex<Option<AuraSeekEngine>>>,
    vector_store: Arc<VectorStore>,
    progress_tx: Option<tauri::ipc::Channel<IngestProgress>>,
) -> Result<IngestSummary> {
    let source_path = Path::new(&source_dir);
    if !source_path.exists() {
        return Err(anyhow::anyhow!("Source directory not found: {}", source_dir));
    }

    // Collect all files
    let (image_files, video_files) = collect_files(source_path);
    let total = image_files.len() + video_files.len();

    let mut summary = IngestSummary {
        total_found: total,
        newly_added: 0,
        skipped_dup: 0,
        errors: 0,
    };

    // ── Thread 1 + 2: scan images ─────────────────────────────────────────────
    let (tx, mut rx) = mpsc::channel::<(PathBuf, ObjectId)>(64);

    // Clone for the scan task
    let db_scan = db.clone();
    let source_dir_clone = source_dir.clone();

    // Spawn scan task (Thread 1): file scan + insert media stubs
    let scan_task = tokio::spawn(async move {
        let mut newly_added = 0usize;
        let mut skipped = 0usize;
        let mut errors = 0usize;

        for path in image_files {
            match scan_single_file(&path, &db_scan, &source_dir_clone, "image").await {
                Ok(Some(media_id)) => {
                    newly_added += 1;
                    let _ = tx.send((path, media_id)).await;
                }
                Ok(None) => { skipped += 1; }
                Err(e) => { 
                    eprintln!("[AuraSeek] Lỗi khi quét ảnh {:?}: {}", path, e);
                    errors += 1; 
                }
            }
        }
        (newly_added, skipped, errors)
    });

    // Thread 2: AI processing + embedding
    let mut ai_processed = 0usize;
    while let Some((path, media_id)) = rx.recv().await {
        let path_str = path.to_string_lossy().to_string();
        let mut eng_guard = engine.lock().await;
        let eng = match eng_guard.as_mut() {
            Some(e) => e,
            None => { drop(eng_guard); continue; }
        };
        
        match eng.process_image(&path_str) {
            Ok(output) => {
                // Convert objects
                let objects: Vec<ObjectEntry> = output.objects.iter().map(|o| ObjectEntry {
                    class_name: o.class_name.clone(),
                    conf: o.conf,
                    bbox: Bbox {
                        x: o.bbox[0], y: o.bbox[1],
                        w: o.bbox[2] - o.bbox[0],
                        h: o.bbox[3] - o.bbox[1],
                    },
                    mask_area: o.mask_area,
                    mask_path: None,
                }).collect();

                // Convert faces
                let faces: Vec<FaceEntry> = output.faces.iter().map(|f| FaceEntry {
                    face_id: f.face_id.clone(),
                    name: f.name.clone(),
                    conf: f.conf,
                    bbox: Bbox {
                        x: f.bbox[0], y: f.bbox[1],
                        w: f.bbox[2] - f.bbox[0],
                        h: f.bbox[3] - f.bbox[1],
                    },
                }).collect();

                drop(eng_guard); // Release lock before DB calls

                // Update media doc with AI results
                let _ = DbOperations::update_media_ai(&db, media_id, objects, faces).await;

                // Save embedding and add to vector store
                if !output.vision_embedding.is_empty() {
                    let emb = output.vision_embedding.clone();
                    let _ = DbOperations::upsert_embedding(
                        &db, media_id, "image", None, None, emb.clone()
                    ).await;
                    vector_store.add(VectorEntry {
                        media_id,
                        source: "image".to_string(),
                        embedding: emb,
                    });
                }

                ai_processed += 1;
            }
            Err(e) => {
                drop(eng_guard);
                eprintln!("AI error for {}: {}", path_str, e);
            }
        }

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(IngestProgress {
                processed: ai_processed,
                total,
                current_file: path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string(),
            });
        }
    }

    let (newly_added, skipped, errors) = scan_task.await?;
    summary.newly_added = newly_added;
    summary.skipped_dup = skipped;
    summary.errors = errors;

    // Process videos
    for video_path in &video_files {
        let path_str = video_path.to_string_lossy().to_string();
        match scan_single_file(video_path, &db, &source_dir, "video").await {
            Ok(Some(media_id)) => {
                summary.newly_added += 1;
                // Video processing in background (scene extraction)
                let db2 = db.clone();
                let vs2 = vector_store.clone();
                let eng2 = engine.clone();
                tokio::spawn(async move {
                    let _ = crate::ingest::video_ingest::process_video_scenes(
                        &path_str, media_id, &db2, &eng2, &vs2
                    ).await;
                });
            }
            Ok(None) => {
                summary.skipped_dup += 1;
            }
            Err(e) => {
                eprintln!("[AuraSeek] Lỗi khi quét video {:?}: {}", video_path, e);
                summary.errors += 1;
            }
        }
    }

    Ok(summary)
}

async fn scan_single_file(
    path: &Path,
    db: &MongoDb,
    source_dir: &str,
    media_type: &str,
) -> Result<Option<ObjectId>> {
    let sha256 = compute_sha256(path)?;

    // Dedup check
    if DbOperations::is_duplicate_sha256(db, &sha256).await? {
        return Ok(None);
    }

    let meta = std::fs::metadata(path)?;
    let size = meta.len();
    let name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    
    let path_str = path.to_string_lossy().to_string();

    // Get image dimensions
    let (width, height) = if media_type == "image" {
        get_image_dimensions(&path_str)
    } else {
        (None, None)
    };

    // File modification time
    let modified_at = meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| BsonDateTime::from_millis(d.as_millis() as i64));

    let doc = MediaDoc {
        id: None,
        media_type: media_type.to_string(),
        source: source_dir.to_string(),
        file: FileInfo {
            path: path_str,
            name,
            size,
            sha256,
            phash: None,
        },
        metadata: MediaMetadata {
            width,
            height,
            duration: None,
            fps: None,
            created_at: modified_at,
            modified_at,
        },
        objects: vec![],
        faces: vec![],
        processed: false,
    };

    let media_id = DbOperations::insert_media(db, doc).await?;
    Ok(Some(media_id))
}

fn collect_files(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut images = vec![];
    let mut videos = vec![];
    collect_files_recursive(dir, &mut images, &mut videos);
    (images, videos)
}

fn collect_files_recursive(dir: &Path, images: &mut Vec<PathBuf>, videos: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, images, videos);
        } else {
            let ext = path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                images.push(path);
            } else if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
                videos.push(path);
            }
        }
    }
    images.sort();
    videos.sort();
}

fn compute_sha256(path: &Path) -> Result<String> {
    let data = std::fs::read(path)?;
    let hash = Sha256::digest(&data);
    Ok(hex::encode(hash))
}

fn get_image_dimensions(path: &str) -> (Option<u32>, Option<u32>) {
    use image::io::Reader as ImageReader;
    if let Ok(reader) = ImageReader::open(path) {
        if let Ok(dims) = reader.into_dimensions() {
            return (Some(dims.0), Some(dims.1));
        }
    }
    (None, None)
}

// ─── Types ────────────────────────────────────────────────────────────────────

use mongodb::bson::oid::ObjectId;

#[derive(Debug, Clone, serde::Serialize)]
pub struct IngestProgress {
    pub processed:    usize,
    pub total:        usize,
    pub current_file: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IngestSummary {
    pub total_found: usize,
    pub newly_added: usize,
    pub skipped_dup: usize,
    pub errors:      usize,
}
