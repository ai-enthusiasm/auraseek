/// Video processing pipeline
///
/// For each new video:
///   1. Probe FPS + total frames (ffprobe)
///   2. Scene detection (ffmpeg showinfo filter)
///   3. Extract 3 frames per scene (start / mid / end)
///   4. Run full AI pipeline (YOLO + face + embedding) on each frame
///   5. Aggregate objects + faces across all frames → update media record
///   6. Store per-frame embeddings (skip consecutive near-duplicates when cos ≥ `duplicate_score_threshold`)
///   7. Save first-frame thumbnail as `<stem>.thumb.jpg` in the same directory
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::platform::process::hidden_command;
use crate::infrastructure::database::{SqliteDb, DbOperations};
use crate::infrastructure::database::models::{ObjectEntry, FaceEntry, Bbox, PersonDoc};
use crate::infrastructure::ai::AuraSeekEngine;
use crate::infrastructure::ai::vision::{coco_label_vi, cosine_similarity};
use crate::{log_info, log_warn};

/// Two frames whose vision embeddings have cosine similarity ≥ `AppConfig::duplicate_score_threshold`
/// are treated as duplicates for ingest (same field as Qdrant duplicate `score_threshold`).

/// Full video processing pipeline.
/// If `thumb_cache_dir` is Some, thumbnails (video + face) are written there instead of next to the video;
/// person.thumbnail is then stored as full path. Otherwise thumbs are written next to the video (legacy).
/// Returns the thumbnail filename or path if created, None otherwise.
pub async fn process_video(
    video_path: &str,
    media_id: &str,
    sqlite: &Arc<std::sync::Mutex<Option<SqliteDb>>>,
    qdrant: &Arc<Mutex<Option<qdrant_client::Qdrant>>>,
    engine: &Arc<Mutex<Option<AuraSeekEngine>>>,
    thumb_cache_dir: Option<&std::path::Path>,
) -> Result<Option<String>> {
    let (fps, total_frames) = probe_video(video_path)?;
    log_info!("🎥 Video probe: {} fps={:.2} frames={}", video_path, fps, total_frames);

    if total_frames == 0 {
        log_warn!("🎥 Could not determine frame count for {}", video_path);
        return Ok(None);
    }

    // ── 1. Scene detection ───────────────────────────────────────────────────
    let cuts = {
        let scene = crate::core::config::AppConfig::global().video_scene_threshold;
        detect_scenes(video_path, fps, scene)?
    };
    log_info!("🎬 {} scenes detected", cuts.len() + 1);

    // Build (start, end) frame ranges from cut points
    let mut scenes: Vec<(u64, u64)> = Vec::new();
    let mut prev = 0u64;
    for cut in &cuts {
        scenes.push((prev, cut.saturating_sub(1)));
        prev = *cut;
    }
    scenes.push((prev, total_frames.saturating_sub(1)));

    // ── 2. Extract frames ─────────────────────────────────────────────────────
    let stem = Path::new(video_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("vid");

    let tmp_dir = std::env::temp_dir().join(format!("auraseek_frames_{}_{}", stem, std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir)?;

    let mut frame_jobs: Vec<u64> = Vec::new();
    for (s_idx, (start, end)) in scenes.iter().enumerate() {
        let len = end.saturating_sub(*start);
        
        let frame_candidates = vec![
            start + (len as f64 * 0.2) as u64,
            start + (len as f64 * 0.5) as u64,
            start + (len as f64 * 0.8) as u64,
        ];

        let mut seen = std::collections::HashSet::new();
        let mut scene_frames = Vec::new();
        for fi in frame_candidates {
            if seen.insert(fi) {
                let out = tmp_dir.join(format!("s{}_f{}.debug.jpg", s_idx, fi));
                if extract_frame(video_path, fi, fps, &out).is_ok() {
                    let (is_good, luma) = is_good_brightness(&out);
                    scene_frames.push((fi, is_good, luma, out));
                }
            }
        }

        let mut keep = Vec::new();
        for &(fi, is_good, _, _) in &scene_frames {
            if is_good {
                keep.push(fi);
            }
        }

        if keep.is_empty() && !scene_frames.is_empty() {
            let mut best_idx = 0;
            let mut best_diff = f64::MAX;
            for (i, (_, _, luma, _)) in scene_frames.iter().enumerate() {
                let diff = (luma - 128.0).abs();
                if diff < best_diff {
                    best_diff = diff;
                    best_idx = i;
                }
            }
            keep.push(scene_frames[best_idx].0);
        }

        for (fi, _, _, out_path) in &scene_frames {
            if !keep.contains(fi) {
                let _ = std::fs::remove_file(out_path);
            }
        }

        frame_jobs.extend(keep);
    }

    log_info!("🖼️  {} frames to process for {}", frame_jobs.len(), video_path);

    // ── 3. AI on every frame, aggregate under the video's media_id ─────────
    let mut obj_map: HashMap<String, ObjectEntry>  = HashMap::new();
    let mut face_map: HashMap<String, FaceEntry>   = HashMap::new();
    let mut face_frame_map: HashMap<String, u64>   = HashMap::new();
    let mut embed_count = 0usize;
    let mut last_stored_emb: Option<Vec<f32>> = None;

    let scenes_clone = scenes.clone();
    let config = crate::core::config::AppConfig::global();
    let collection = &config.qdrant_collection;
    let mut qdrant_ready_for_embeddings = false;
    let mut embedding_error = false;

    {
        let qdrant_guard = qdrant.lock().await;
        if let Some(ref client) = *qdrant_guard {
            if let Err(e) = DbOperations::delete_embeddings_for_media(client, collection, media_id).await {
                log_warn!("  ⚠️ delete_embeddings_for_media for video {}: {:#}", media_id, e);
                embedding_error = true;
            } else {
                qdrant_ready_for_embeddings = true;
            }
        } else {
            log_warn!("  ⚠️ Qdrant client unavailable for video {}; media will be reprocessed later", media_id);
            embedding_error = true;
        }
    }

    for frame_idx in &frame_jobs {
        let s_idx = scenes_clone.iter().enumerate()
            .find(|(_, (s, e))| frame_idx >= s && frame_idx <= e)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let frame_path = tmp_dir.join(format!("s{}_f{}.debug.jpg", s_idx, frame_idx));
        let frame_path_str = frame_path.to_string_lossy().to_string();

        let output = match super::image_processor::analyze_image_raw(&frame_path_str, engine).await {
            Some(o) => o,
            None => continue,
        };

        let timestamp = *frame_idx as f64 / fps;
        log_info!(
            "  🖼  Frame {} @ {:.2}s | obj={} face={} emb={}",
            frame_idx, timestamp,
            output.objects.len(), output.faces.len(), output.vision_embedding.len()
        );

        for o in &output.objects {
            let class_name = coco_label_vi(&o.class_name).to_string();
            let entry = obj_map.entry(class_name.clone()).or_insert_with(|| ObjectEntry {
                class_name,
                conf:       0.0,
                bbox:       Bbox { x: o.bbox[0], y: o.bbox[1], w: o.bbox[2]-o.bbox[0], h: o.bbox[3]-o.bbox[1] },
                mask_area:  Some(o.mask_area),
                mask_path:  None,
                mask_rle:   Some(o.mask_rle.iter().map(|&(a,b)| [a,b]).collect()),
            });
            if o.conf > entry.conf {
                entry.conf      = o.conf;
                entry.bbox      = Bbox { x: o.bbox[0], y: o.bbox[1], w: o.bbox[2]-o.bbox[0], h: o.bbox[3]-o.bbox[1] };
                entry.mask_rle  = Some(o.mask_rle.iter().map(|&(a,b)| [a,b]).collect());
                entry.mask_area = Some(o.mask_area);
            }
        }

        for f in &output.faces {
            let mut is_best = false;
            let entry = face_map.entry(f.face_id.clone()).or_insert_with(|| {
                is_best = true;
                FaceEntry {
                    face_id: f.face_id.clone(),
                    name:    f.name.clone(),
                    conf:    0.0,
                    bbox:    Bbox { x: f.bbox[0], y: f.bbox[1], w: f.bbox[2]-f.bbox[0], h: f.bbox[3]-f.bbox[1] },
                }
            });
            if f.conf > entry.conf {
                entry.conf = f.conf;
                entry.name = f.name.clone();
                entry.bbox = Bbox { x: f.bbox[0], y: f.bbox[1], w: f.bbox[2]-f.bbox[0], h: f.bbox[3]-f.bbox[1] };
                is_best = true;
            }
            if is_best { face_frame_map.insert(f.face_id.clone(), *frame_idx); }
        }

        if qdrant_ready_for_embeddings && !output.vision_embedding.is_empty() {
            let dedup_thr = config.duplicate_score_threshold;
            let skip_dedup = last_stored_emb.as_ref().is_some_and(|prev| {
                cosine_similarity(prev, &output.vision_embedding) >= dedup_thr
            });
            if skip_dedup {
                log_info!(
                    "  ⏭️  Frame {} @ {:.2}s skipped (embedding dedup, cos ≥ {:.4})",
                    frame_idx, timestamp, dedup_thr
                );
                continue;
            }

            let qdrant_guard = qdrant.lock().await;
            if let Some(ref client) = *qdrant_guard {
                if let Err(e) = DbOperations::insert_embedding(
                    client, collection, media_id, "video_frame",
                    Some(timestamp), Some(*frame_idx as u32),
                    output.vision_embedding.clone(),
                ).await {
                    log_warn!("  ⚠️ insert_embedding frame {}: {:#}", frame_idx, e);
                    embedding_error = true;
                } else {
                    embed_count += 1;
                    last_stored_emb = Some(output.vision_embedding.clone());
                    log_info!("  ✅ Frame {} @ {:.2}s embedded", frame_idx, timestamp);
                }
            } else {
                embedding_error = true;
            }
        }
    }

    // ── 4. Store aggregated objects + faces under video's media_id ─────────
    let obj_count  = obj_map.len();
    let face_count = face_map.len();
    let objects: Vec<ObjectEntry> = obj_map.into_values().collect();
    let faces: Vec<FaceEntry>     = face_map.into_values().collect();

    let detected_faces_for_person: Vec<(String, f32, Bbox, Option<String>, u64)> = faces.iter()
        .map(|f| (
            f.face_id.clone(), f.conf, f.bbox.clone(), f.name.clone(),
            *face_frame_map.get(&f.face_id).unwrap_or(&0)
        ))
        .collect();

    // ── 5. Generate thumbnail from the first processed frame ──
    let video_parent = Path::new(video_path).parent().unwrap_or(Path::new("."));
    if let Some(cache_dir) = thumb_cache_dir {
        let _ = std::fs::create_dir_all(cache_dir);
    }
    
    let thumb_name = format!("{}.thumb.jpg", stem);
    let thumb_path: std::path::PathBuf = thumb_cache_dir
        .map(|d| d.join(&thumb_name))
        .unwrap_or_else(|| video_parent.join(&thumb_name));

    let thumb_frame_idx: u64 = frame_jobs.first().copied().unwrap_or(0);

    let thumb_result = if extract_frame(video_path, thumb_frame_idx, fps, &thumb_path).is_ok() {
        log_info!(
            "🖼️  Thumbnail saved (frame {}): {}",
            thumb_frame_idx,
            thumb_path.display()
        );
        Some(thumb_name)
    } else {
        log_warn!("⚠️ Could not generate thumbnail for {}", video_path);
        None
    };
    
    let thumb_value_for_db = thumb_cache_dir
        .map(|_| thumb_path.to_string_lossy().to_string())
        .or(thumb_result.clone());

    {
        let guard = sqlite.lock().unwrap();
        if let Some(ref db) = *guard {
            if let Err(e) = DbOperations::update_media_ai(db, media_id, objects, faces, thumb_value_for_db) {
                log_warn!("⚠️ update_media_ai for video {}: {}", media_id, e);
            }
            if embedding_error {
                if let Err(e) = DbOperations::set_media_processed(db, media_id, false) {
                    log_warn!("⚠️ failed to mark video {} as unprocessed after embedding error: {}", media_id, e);
                }
            }
            for (fid, conf, bbox, name, fi) in &detected_faces_for_person {
                let face_thumb_name = format!("{}_face_{}.thumb.jpg", stem, fid);
                let face_thumb_path: std::path::PathBuf = thumb_cache_dir
                    .map(|d| d.join(&face_thumb_name))
                    .unwrap_or_else(|| video_parent.join(&face_thumb_name));
                if !face_thumb_path.exists() {
                    let _ = extract_frame(video_path, *fi, fps, &face_thumb_path);
                }
                let face_thumb_value = thumb_cache_dir
                    .map(|_| face_thumb_path.to_string_lossy().to_string())
                    .unwrap_or(face_thumb_name);
                if let Err(e) = DbOperations::upsert_person(db, PersonDoc {
                    face_id:   fid.clone(),
                    name:      name.clone(),
                    thumbnail: Some(face_thumb_value),
                    conf:      Some(*conf),
                    face_bbox: Some(bbox.clone()),
                }) {
                    log_warn!("  ⚠️ upsert_person {} for video: {}", fid, e);
                }
            }
        }
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);

    log_info!(
        "🎥 Video done: {} embeds, {} objects, {} faces | {}",
        embed_count, obj_count, face_count, video_path
    );

    Ok(thumb_result)
}

// ─── Private helpers ─────────────────────────────────────────────────────────

pub(crate) fn probe_video(video_path: &str) -> Result<(f64, u64)> {
    let fps_out = hidden_command("ffprobe")
        .args(["-v","error","-select_streams","v:0","-show_entries",
               "stream=r_frame_rate","-of","default=noprint_wrappers=1:nokey=1", video_path])
        .output()?;
    let fps = parse_fraction(String::from_utf8_lossy(&fps_out.stdout).trim())
        .unwrap_or(30.0);

    let mut parsed_frames = 0u64;
    let frames_out = hidden_command("ffprobe")
        .args(["-v","error","-select_streams","v:0","-show_entries",
               "stream=nb_frames","-of","default=noprint_wrappers=1:nokey=1", video_path])
        .output()?;
    let frames_str = String::from_utf8_lossy(&frames_out.stdout);
    if let Ok(n) = frames_str.trim().parse::<u64>() {
        parsed_frames = n;
    }

    let dur_out = hidden_command("ffprobe")
        .args(["-v","error","-show_entries","format=duration",
               "-of","default=noprint_wrappers=1:nokey=1", video_path])
        .output()
        .unwrap_or_else(|_| std::process::Output {
            status: unsafe { std::mem::zeroed() }, stdout: vec![], stderr: vec![],
        });
    let dur: f64 = String::from_utf8_lossy(&dur_out.stdout).trim().parse().unwrap_or(0.0);
    let dur_frames = (dur * fps) as u64;

    let total = parsed_frames.max(dur_frames);
    Ok((fps, total))
}

pub(crate) fn detect_scenes(video_path: &str, fps: f64, scene_threshold: f64) -> Result<Vec<u64>> {
    let filter = format!("select='gt(scene,{})',showinfo", scene_threshold);
    let output = hidden_command("ffmpeg")
        .args(["-i", video_path, "-vf", &filter, "-vsync","vfr","-f","null","-"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut cuts: Vec<u64> = Vec::new();
    for line in stderr.lines() {
        if !line.contains("pts_time") || !line.contains("Parsed_showinfo") { continue; }
        if let Some(t) = parse_pts_time(line) {
            cuts.push((t * fps).round() as u64);
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    Ok(cuts)
}

pub(crate) fn extract_frame(video_path: &str, frame_idx: u64, fps: f64, out: &Path) -> Result<()> {
    let ts  = format!("{:.6}", frame_idx as f64 / fps);
    let status = hidden_command("ffmpeg")
        .args(["-y","-ss",&ts,"-i",video_path,"-vframes","1","-q:v","3",
               &out.to_string_lossy()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?;
    if status.success() && out.exists() { Ok(()) }
    else { Err(anyhow::anyhow!("ffmpeg failed frame {}", frame_idx)) }
}

fn parse_fraction(s: &str) -> Option<f64> {
    if let Some((n, d)) = s.split_once('/') {
        let a: f64 = n.trim().parse().ok()?;
        let b: f64 = d.trim().parse().ok()?;
        if b == 0.0 { None } else { Some(a / b) }
    } else { s.trim().parse().ok() }
}

fn parse_pts_time(line: &str) -> Option<f64> {
    let key = "pts_time:";
    let pos  = line.find(key)?;
    let rest = &line[pos + key.len()..];
    let end  = rest.find(|c: char| !c.is_ascii_digit() && c != '.').unwrap_or(rest.len());
    rest[..end].parse().ok()
}

pub(crate) fn is_good_brightness(path: &Path) -> (bool, f64) {
    if let Ok(img) = image::open(path) {
        let img = img.to_rgba8();
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 { return (false, 128.0); }
        
        let mut total_luma: u64 = 0;
        for pixel in img.pixels() {
            let r = pixel[0] as u64;
            let g = pixel[1] as u64;
            let b = pixel[2] as u64;
            let luma = (r * 299 + g * 587 + b * 114) / 1000;
            total_luma += luma;
        }
        
        let avg_luma = total_luma as f64 / (w * h) as f64;
        let is_good = avg_luma >= 25.0 && avg_luma <= 240.0;
        return (is_good, avg_luma);
    }
    (true, 128.0)
}
