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
) -> Option<(EngineOutput, u128)> {
    let mut eng_guard = engine.lock().await;
    let eng = eng_guard.as_mut()?;
    match eng.process_image(path_str, qdrant).await {
        Ok(output) => {
            let dur = output.dur_total;
            Some((output, dur))
        }
        Err(e) => {
            crate::log_warn!("🤖 AI error for {}: {}", path_str, e);
            None
        }
    }
}

/// Generate a small thumbnail (max 400×400px, JPEG quality 75%) for fast grid display.
/// Returns the absolute path of the created thumbnail file.
pub fn generate_thumbnail(
    source_path: &Path,
    cache_dir: &Path,
    media_id: &str,
) -> Result<String> {
    let img = image::open(source_path)?;
    generate_thumbnail_from_image(&img, cache_dir, media_id)
}

/// Generate thumbnail from an already-decoded DynamicImage — avoids re-reading from disk.
/// Uses Triangle filter (bilinear) for ~3-5x speed over Lanczos3; 400px is sufficient for grid cells.
pub fn generate_thumbnail_from_image(
    img: &image::DynamicImage,
    cache_dir: &Path,
    media_id: &str,
) -> Result<String> {
    std::fs::create_dir_all(cache_dir)?;

    // 400px max is enough for grid cells (typically displayed at ~200px).
    // Using fast downsampling via .thumbnail() which is ~10-20x faster than .resize() on large images.
    let thumb = img.thumbnail(400, 400);

    let thumb_path = cache_dir.join(format!("{}.thumb.jpg", media_id));

    // 75% JPEG quality reduces file size ~40% vs 85%, loads faster on FE
    let out_file = std::fs::File::create(&thumb_path)?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(out_file, 75);
    encoder.encode(
        thumb.to_rgb8().as_raw(),
        thumb.width(),
        thumb.height(),
        image::ColorType::Rgb8,
    )?;

    Ok(thumb_path.to_string_lossy().to_string())
}

pub fn generate_face_thumbnail(
    img: &image::DynamicImage,
    bbox: &Bbox,
    cache_dir: &Path,
    face_id: &str,
) -> Result<String> {
    std::fs::create_dir_all(cache_dir)?;
    let img_w = img.width();
    let img_h = img.height();

    let face_cx = bbox.x + bbox.w / 2.0;
    let face_cy = bbox.y + bbox.h / 2.0;
    let crop_size = (bbox.w.max(bbox.h) * 2.0) as u32;

    let mut crop_x = (face_cx - crop_size as f32 / 2.0) as i32;
    let mut crop_y = (face_cy - crop_size as f32 / 2.0) as i32;

    let clamped_size = crop_size.min(img_w).min(img_h);
    crop_x = crop_x.max(0).min((img_w - clamped_size) as i32);
    crop_y = crop_y.max(0).min((img_h - clamped_size) as i32);

    let cropped = img.crop_imm(crop_x as u32, crop_y as u32, clamped_size, clamped_size);
    let avatar = cropped.thumbnail(200, 200);

    let dest_path = cache_dir.join(format!("face_{}.jpg", face_id));
    let out_file = std::fs::File::create(&dest_path)?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(out_file, 75);
    encoder.encode(
        avatar.to_rgb8().as_raw(),
        avatar.width(),
        avatar.height(),
        image::ColorType::Rgb8,
    )?;

    Ok(dest_path.to_string_lossy().to_string())
}

pub fn crop_and_save_face_from_file(
    frame_path: &Path,
    bbox: &Bbox,
) -> Result<()> {
    let img = image::open(frame_path)?;
    let img_w = img.width();
    let img_h = img.height();

    let face_cx = bbox.x + bbox.w / 2.0;
    let face_cy = bbox.y + bbox.h / 2.0;
    let crop_size = (bbox.w.max(bbox.h) * 2.0) as u32;

    let mut crop_x = (face_cx - crop_size as f32 / 2.0) as i32;
    let mut crop_y = (face_cy - crop_size as f32 / 2.0) as i32;

    let clamped_size = crop_size.min(img_w).min(img_h);
    crop_x = crop_x.max(0).min((img_w - clamped_size) as i32);
    crop_y = crop_y.max(0).min((img_h - clamped_size) as i32);

    let cropped = img.crop_imm(crop_x as u32, crop_y as u32, clamped_size, clamped_size);
    let avatar = cropped.thumbnail(200, 200);

    let out_file = std::fs::File::create(frame_path)?;
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(out_file, 75);
    encoder.encode(
        avatar.to_rgb8().as_raw(),
        avatar.width(),
        avatar.height(),
        image::ColorType::Rgb8,
    )?;

    Ok(())
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
    let qdrant_client_owned = {
        let guard = qdrant.lock().await;
        guard.clone()
    };
    let qdrant_client = qdrant_client_owned.as_ref();

    let (output, dur_ai) = match analyze_image_raw(path_str, engine, qdrant_client).await {
        Some(pair) => pair,
        None => return,
    };

    let objects = convert_objects(&output);
    let faces = convert_faces(&output);
    let detected_faces = extract_person_data(&faces);

    // ── Generate thumbnail for grid view ──────────────────────────────────
    // Reuse the decoded DynamicImage from the AI engine if available,
    // otherwise fall back to reading from disk (legacy path).
    let t_thumb = std::time::Instant::now();
    let mut face_thumbnails: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let thumb_path: Option<String> = thumb_cache_dir.and_then(|cache_dir| {
        let photos_dir = cache_dir.join("photos");
        let faces_dir = cache_dir.join("faces");
        let result = if let Some(ref decoded_img) = output.decoded_image {
            for (fid, _, bbox) in &detected_faces {
                match generate_face_thumbnail(decoded_img, bbox, &faces_dir, fid) {
                    Ok(p) => {
                        face_thumbnails.insert(fid.clone(), p);
                    }
                    Err(e) => {
                        crate::log_warn!("  ⚠️ Face thumbnail failed for face_id={} in {}: {}", fid, path_str, e);
                    }
                }
            }
            generate_thumbnail_from_image(decoded_img, &photos_dir, media_id)
        } else {
            let source = Path::new(path_str);
            generate_thumbnail(source, &photos_dir, media_id)
        };
        match result {
            Ok(p) => Some(p),
            Err(e) => {
                crate::log_warn!("  ⚠️ Thumbnail generation failed for {}: {}", path_str, e);
                None
            }
        }
    });
    let dur_thumb = t_thumb.elapsed().as_millis();

    // Capture original image dimensions from the decoded image (before thumbnail scaling)
    let img_width  = output.decoded_image.as_ref().map(|img| img.width());
    let img_height = output.decoded_image.as_ref().map(|img| img.height());

    let thumb_path_rel = thumb_path.as_ref().and_then(|p| {
        if let Some(cache_dir) = thumb_cache_dir {
            std::path::Path::new(p)
                .strip_prefix(cache_dir)
                .ok()
                .map(|rel| rel.to_string_lossy().to_string())
        } else {
            None
        }
    }).or(thumb_path.clone());

    let t_sql = std::time::Instant::now();
    {
        let guard = sqlite.lock().unwrap();
        if let Some(ref db) = *guard {
            if let Err(e) = DbOperations::update_media_ai(db, media_id, objects, faces, thumb_path_rel, img_width, img_height) {
                crate::log_warn!("⚠️ update_media_ai failed for {}: {}", media_id, e);
            }
            for (fid, conf, bbox) in &detected_faces {
                let face_thumb = face_thumbnails.get(fid).cloned();
                let face_thumb_rel = face_thumb.as_ref().and_then(|p| {
                    std::path::Path::new(p).file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string())
                });
                if let Err(e) = DbOperations::upsert_person(db, PersonDoc {
                    face_id: fid.clone(),
                    name: None,
                    thumbnail: face_thumb_rel.or_else(|| Some(file_name_only.to_string())),
                    conf: Some(*conf),
                    face_bbox: Some(bbox.clone()),
                }) {
                    crate::log_warn!("⚠️ upsert_person failed for {}: {}", fid, e);
                }
            }
        }
    }
    let dur_sql = t_sql.elapsed().as_millis();

    let t_qdrant = std::time::Instant::now();
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
    let dur_qdrant = t_qdrant.elapsed().as_millis();

    let dur_active = dur_ai + dur_thumb + dur_sql + dur_qdrant;
    let dur_queue = t0.elapsed().as_millis().saturating_sub(dur_active);

    crate::log_info!(
        "✅ Processed {} | Total: {}ms | AI: {}ms | Thumb: {}ms | SQLite: {}ms | Qdrant: {}ms | Queue: {}ms (objs: {}, faces: {})",
        file_name_only,
        dur_active,
        dur_ai,
        dur_thumb,
        dur_sql,
        dur_qdrant,
        dur_queue,
        output.objects.len(),
        output.faces.len()
    );
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
    sqlite: &Arc<std::sync::Mutex<Option<SqliteDb>>>,
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

    // Fast path: check by metadata (name + size + mtime) — uses covering index
    let meta_check = {
        let guard = sqlite.lock().unwrap();
        let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not connected"))?;
        DbOperations::check_file_by_metadata(
            db,
            &name,
            size,
            modified_at.as_deref(),
        )?
    };
    if let Some((media_id, processed)) = meta_check {
        if processed {
            return Ok(None);
        } else {
            return Ok(Some(media_id));
        }
    }

    // Slower path: use partial hash first (reads only 128KB vs full file)
    let partial = compute_partial_hash(path)?;

    let exact_check = {
        let guard = sqlite.lock().unwrap();
        let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not connected"))?;
        DbOperations::check_exact_file(db, &name, &partial)?
    };
    if let Some((media_id, processed)) = exact_check {
        if processed {
            return Ok(None);
        } else {
            return Ok(Some(media_id));
        }
    }

    // Full SHA-256 only needed for truly new files
    let sha256 = compute_sha256(path)?;

    // Skip reading image dimensions here — they'll be captured during AI inference.
    // This avoids an extra image decode per file during scan phase.

    let file_info = FileInfo { name, size, sha256, phash: None };
    let metadata = MediaMetadata {
        width: None, height: None,
        duration: None, fps: None,
        created_at: modified_at.clone(),
        modified_at,
    };

    let guard = sqlite.lock().unwrap();
    let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not connected"))?;
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

/// Compute a fast partial hash: first 64KB + last 64KB + file size.
/// ~50x faster than full SHA-256 for large files, sufficient to detect content changes.
fn compute_partial_hash(path: &Path) -> Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut hasher = Sha256::new();

    // Include file size in hash to differentiate files with same head/tail
    hasher.update(&len.to_le_bytes());

    // Read first 64KB
    let mut buf = [0u8; 65536];
    let n = file.read(&mut buf)?;
    hasher.update(&buf[..n]);

    // Read last 64KB (if file is large enough to have a distinct tail)
    if len > 131072 {
        file.seek(SeekFrom::End(-65536))?;
        let n = file.read(&mut buf)?;
        hasher.update(&buf[..n]);
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

/// Public wrapper for `file_modified_at` — used by ingest pipeline for in-memory processed set.
pub fn file_modified_at_public(meta: &std::fs::Metadata) -> Option<String> {
    file_modified_at(meta)
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
