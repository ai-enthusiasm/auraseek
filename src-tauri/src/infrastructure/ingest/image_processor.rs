use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use sha2::{Sha256, Digest};

use crate::infrastructure::database::{SqliteDb, DbOperations};
use crate::infrastructure::database::models::{FileInfo, MediaMetadata, ObjectEntry, FaceEntry, Bbox, PersonDoc};
use crate::infrastructure::ai::AuraSeekEngine;
use crate::infrastructure::ai::engine::EngineOutput;
use crate::infrastructure::ai::vision::coco_label_vi;

pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "bmp", "webp", "tiff", "tif", "heic", "avif"];
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "mkv", "webm", "m4v", "flv", "wmv"];

pub async fn analyze_image_raw(
    path_str: &str,
    engine: &Arc<Mutex<Option<AuraSeekEngine>>>,
    qdrant: Option<&qdrant_client::Qdrant>,
) -> Option<EngineOutput> {
    let mut eng_guard = engine.lock().await;
    let eng = eng_guard.as_mut()?;
    match eng.process_image(path_str, qdrant).await {
        Ok(output) => Some(output),
        Err(e) => {
            crate::log_warn!("🤖 AI error for {}: {}", path_str, e);
            None
        }
    }
}

/// Generate a small thumbnail (max 800×800px, JPEG quality ~85%) for fast grid display.
/// Returns the absolute path of the created thumbnail file.
pub fn generate_thumbnail(
    source_path: &Path,
    cache_dir: &Path,
    media_id: &str,
) -> Result<String> {
    use image::imageops::FilterType;

    std::fs::create_dir_all(cache_dir)?;

    let img = image::open(source_path)?;
    // Use 800x800 for sharpness on Retina/High-DPI screens, and Lanczos3 filter for high-quality downsampling
    let thumb = img.resize(800, 800, FilterType::Lanczos3);

    let thumb_path = cache_dir.join(format!("{}.thumb.jpg", media_id));

    // Save as JPEG with 85% quality to avoid artifacting
    let out_file = std::fs::File::create(&thumb_path)?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(out_file, 85);
    encoder.encode(
        thumb.to_rgb8().as_raw(),
        thumb.width(),
        thumb.height(),
        image::ColorType::Rgb8,
    )?;

    Ok(thumb_path.to_string_lossy().to_string())
}

pub async fn process_image_file(
    path_str: &str,
    media_id: &str,
    file_name_only: &str,
    sqlite: &Arc<std::sync::Mutex<Option<SqliteDb>>>,
    qdrant: &Arc<Mutex<Option<qdrant_client::Qdrant>>>,
    engine: &Arc<Mutex<Option<AuraSeekEngine>>>,
    thumb_cache_dir: Option<&Path>,
) {
    let t0 = std::time::Instant::now();
    let qdrant_guard = qdrant.lock().await;
    let qdrant_client = qdrant_guard.as_ref();

    let output = match analyze_image_raw(path_str, engine, qdrant_client).await {
        Some(o) => o,
        None => return,
    };

    crate::log_info!(
        "  ✅ Done in {}ms | objects={} faces={} embed_dims={}",
        t0.elapsed().as_millis(),
        output.objects.len(), output.faces.len(), output.vision_embedding.len()
    );

    let objects = convert_objects(&output);
    let faces = convert_faces(&output);
    let detected_faces = extract_person_data(&faces);

    // ── Generate thumbnail for grid view ──────────────────────────────────
    let thumb_path: Option<String> = thumb_cache_dir.and_then(|cache_dir| {
        let source = Path::new(path_str);
        match generate_thumbnail(source, cache_dir, media_id) {
            Ok(p) => {
                crate::log_info!("  🖼️ Thumbnail saved: {}", p);
                Some(p)
            }
            Err(e) => {
                crate::log_warn!("  ⚠️ Thumbnail generation failed for {}: {}", path_str, e);
                None
            }
        }
    });

    {
        let guard = sqlite.lock().unwrap();
        if let Some(ref db) = *guard {
            if let Err(e) = DbOperations::update_media_ai(db, media_id, objects, faces, thumb_path) {
                crate::log_warn!("⚠️ update_media_ai failed for {}: {}", media_id, e);
            }
            for (fid, conf, bbox) in &detected_faces {
                crate::log_info!("  👤 Upserting person face_id={} conf={:.3}", fid, conf);
                if let Err(e) = DbOperations::upsert_person(db, PersonDoc {
                    face_id: fid.clone(),
                    name: None,
                    thumbnail: Some(file_name_only.to_string()),
                    conf: Some(*conf),
                    face_bbox: Some(bbox.clone()),
                }) {
                    crate::log_warn!("⚠️ upsert_person failed for {}: {}", fid, e);
                }
            }
        }
    }

    let mut embedding_ok = output.vision_embedding.is_empty();
    if !output.vision_embedding.is_empty() {
        let config = crate::core::config::AppConfig::global();
        let collection = &config.qdrant_collection;
        if let Some(client) = qdrant_client {
            let mut deleted_old = true;
            if let Err(e) = DbOperations::delete_embeddings_for_media(client, collection, media_id).await {
                crate::log_warn!("⚠️ delete_embeddings_for_media failed for {}: {:#}", media_id, e);
                deleted_old = false;
            }
            // Delete old face embeddings for this media if reprocessing
            let _ = DbOperations::delete_face_embeddings_for_media(client, crate::core::config::QDRANT_FACE_COLLECTION, media_id).await;

            if deleted_old {
                if let Err(e) = DbOperations::insert_embedding(
                    client, collection, media_id, "image", None, None, output.vision_embedding
                ).await {
                    crate::log_warn!("⚠️ insert_embedding failed for {}: {:#}", media_id, e);
                } else {
                    embedding_ok = true;
                }

                // Insert new face embeddings to Qdrant face collection
                for f in &output.faces {
                    if !f.embedding.is_empty() {
                        if let Err(e) = DbOperations::insert_face_embedding(
                            client,
                            crate::core::config::QDRANT_FACE_COLLECTION,
                            media_id,
                            &f.face_id,
                            f.bbox,
                            f.embedding.clone(),
                        ).await {
                            crate::log_warn!("⚠️ insert_face_embedding failed for face_id={} in {}: {:#}", f.face_id, media_id, e);
                        } else {
                            crate::log_info!("  👤 Saved face embedding to Qdrant: face_id={}", f.face_id);
                        }
                    }
                }
            }
        } else {
            crate::log_warn!("⚠️ Qdrant client unavailable; media {} will be reprocessed later", media_id);
        }
    }

    if !embedding_ok {
        let guard = sqlite.lock().unwrap();
        if let Some(ref db) = *guard {
            if let Err(e) = DbOperations::set_media_processed(db, media_id, false) {
                crate::log_warn!("⚠️ failed to mark media {} as unprocessed after embedding error: {}", media_id, e);
            }
        }
    }
}

pub fn convert_objects(output: &EngineOutput) -> Vec<ObjectEntry> {
    output.objects.iter().map(|o| ObjectEntry {
        class_name: coco_label_vi(&o.class_name).to_string(),
        conf: o.conf,
        bbox: Bbox {
            x: o.bbox[0], y: o.bbox[1],
            w: o.bbox[2] - o.bbox[0],
            h: o.bbox[3] - o.bbox[1],
        },
        mask_area: Some(o.mask_area),
        mask_path: None,
        mask_rle: Some(o.mask_rle.iter().map(|&(a, b)| [a, b]).collect()),
    }).collect()
}

pub fn convert_faces(output: &EngineOutput) -> Vec<FaceEntry> {
    output.faces.iter().map(|f| FaceEntry {
        face_id: f.face_id.clone(),
        name: f.name.clone(),
        conf: f.conf,
        bbox: Bbox {
            x: f.bbox[0], y: f.bbox[1],
            w: f.bbox[2] - f.bbox[0],
            h: f.bbox[3] - f.bbox[1],
        },
    }).collect()
}

pub fn extract_person_data(faces: &[FaceEntry]) -> Vec<(String, f32, Bbox)> {
    faces.iter().map(|f| (
        f.face_id.clone(), f.conf,
        Bbox { x: f.bbox.x, y: f.bbox.y, w: f.bbox.w, h: f.bbox.h },
    )).collect()
}

pub fn scan_single_file(
    path: &Path,
    db: &SqliteDb,
    _source_dir: &str,
    media_type: &str,
) -> Result<Option<String>> {
    let meta = std::fs::metadata(path)?;
    let size = meta.len();
    let name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let modified_at = file_modified_at(&meta);

    if let Ok(Some((media_id, processed))) = DbOperations::check_file_by_metadata(
        db,
        &name,
        size,
        modified_at.as_deref(),
    ) {
        if processed {
            return Ok(None);
        } else {
            return Ok(Some(media_id));
        }
    }

    let sha256 = compute_sha256(path)?;

    if let Ok(Some((media_id, processed))) = DbOperations::check_exact_file(db, &name, &sha256) {
        if processed {
            return Ok(None);
        } else {
            return Ok(Some(media_id));
        }
    }

    let path_str = path.to_string_lossy().to_string();

    let (width, height) = if media_type == "image" {
        get_image_dimensions(&path_str)
    } else {
        (None, None)
    };

    let file_info = FileInfo { name, size, sha256, phash: None };
    let metadata = MediaMetadata {
        width, height,
        duration: None, fps: None,
        created_at: modified_at.clone(),
        modified_at,
    };

    if let Some(media_id) = DbOperations::find_media_by_name(db, &file_info.name)? {
        DbOperations::reset_media_file(db, &media_id, media_type, &file_info, &metadata)?;
        return Ok(Some(media_id));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let media_id = DbOperations::insert_media(db, &id, media_type, &file_info, &metadata)?;
    Ok(Some(media_id))
}

pub fn collect_files(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
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
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if IMAGE_EXTENSIONS.contains(&ext.as_str()) && !fname.ends_with(".thumb.jpg") && !fname.ends_with(".debug.jpg") {
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
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 256]; // 256KB buffer
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 { break; }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn file_modified_at(meta: &std::fs::Metadata) -> Option<String> {
    meta.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| {
            chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        })
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
