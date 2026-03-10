/// Video processing pipeline
///
/// For each new video:
///   1. Probe FPS + total frames (ffprobe)
///   2. Scene detection (ffmpeg showinfo filter)
///   3. Extract 3 frames per scene (start / mid / end)
///   4. Run full AI pipeline (YOLO + face + embedding) on each frame
///   5. Aggregate objects + faces across all frames → update media record
///   6. Store per-frame embeddings (skip near-duplicates, cosine > 0.98)
///   7. Save first-frame thumbnail as `<stem>.thumb.jpg` in the same directory
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::db::{DbOperations, SurrealDb};
use crate::db::models::{ObjectEntry, FaceEntry, Bbox, PersonDoc};
use crate::processor::AuraSeekEngine;
use crate::{log_info, log_warn};

/// Scene-detection threshold for ffmpeg (0 = no change, 1 = full change).
const SCENE_THRESHOLD: f64 = 0.30;
/// Two frames whose vision embeddings have cosine similarity ≥ this are considered duplicates.
const DEDUP_THRESHOLD: f32 = 0.98;

/// Full video processing pipeline.
/// Returns the thumbnail filename (stem + ".thumb.jpg") if created, None otherwise.
pub async fn process_video(
    video_path: &str,
    media_id: &str,
    db: &Arc<Mutex<Option<SurrealDb>>>,
    engine: &Arc<Mutex<Option<AuraSeekEngine>>>,
) -> Result<Option<String>> {
    let (fps, total_frames) = probe_video(video_path)?;
    log_info!("🎥 Video probe: {} fps={:.2} frames={}", video_path, fps, total_frames);

    if total_frames == 0 {
        log_warn!("🎥 Could not determine frame count for {}", video_path);
        return Ok(None);
    }

    // ── 1. Scene detection ───────────────────────────────────────────────────
    let cuts = detect_scenes(video_path, fps)?;
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
    let tmp_dir = std::env::temp_dir().join(format!("auraseek_vid_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir)?;

    let mut frame_jobs: Vec<u64> = Vec::new();
    for (s_idx, (start, end)) in scenes.iter().enumerate() {
        let mid = (start + end) / 2;
        let mut seen = std::collections::HashSet::new();
        for fi in [*start, mid, *end] {
            if seen.insert(fi) {
                let out = tmp_dir.join(format!("s{}_f{}.jpg", s_idx, fi));
                if extract_frame(video_path, fi, fps, &out).is_ok() {
                    frame_jobs.push(fi);
                }
            }
        }
    }

    log_info!("🖼️  {} frames to process for {}", frame_jobs.len(), video_path);

    // ── 3. Full AI on every frame ─────────────────────────────────────────────
    // Accumulators for objects and faces across all frames
    let mut obj_map: HashMap<String, ObjectEntry> = HashMap::new(); // key = class_name
    let mut face_map: HashMap<String, FaceEntry>  = HashMap::new(); // key = face_id
    let mut stored_embeddings: Vec<Vec<f32>>       = Vec::new();
    let mut embed_count = 0usize;

    let stem = Path::new(video_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("vid");

    let scenes_clone = scenes.clone();

    for (job_idx, frame_idx) in frame_jobs.iter().enumerate() {
        // Compute the scene index for this frame job
        let s_idx = scenes_clone.iter().enumerate()
            .find(|(_, (s, e))| frame_idx >= s && frame_idx <= e)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let frame_path = tmp_dir.join(format!("s{}_f{}.jpg", s_idx, frame_idx));
        let frame_path_str = frame_path.to_string_lossy().to_string();

        let mut eng_guard = engine.lock().await;
        let eng = match eng_guard.as_mut() {
            Some(e) => e,
            None => { drop(eng_guard); continue; }
        };

        let result = eng.process_image(&frame_path_str);
        drop(eng_guard); // release lock immediately

        match result {
            Ok(output) => {
                let timestamp = *frame_idx as f64 / fps;
                log_info!(
                    "  🖼  Frame {} @ {:.2}s | obj={} face={} emb={}",
                    frame_idx, timestamp,
                    output.objects.len(), output.faces.len(), output.vision_embedding.len()
                );

                // ── Aggregate objects (keep highest-confidence entry per class) ──
                for o in &output.objects {
                    let entry = obj_map.entry(o.class_name.clone()).or_insert_with(|| ObjectEntry {
                        class_name: o.class_name.clone(),
                        conf:       0.0,
                        bbox:       Bbox { x: o.bbox[0], y: o.bbox[1], w: o.bbox[2]-o.bbox[0], h: o.bbox[3]-o.bbox[1] },
                        mask_area:  Some(o.mask_area),
                        mask_path:  None,
                        mask_rle:   Some(o.mask_rle.iter().map(|&(a,b)| [a,b]).collect()),
                    });
                    if o.conf > entry.conf {
                        entry.conf     = o.conf;
                        entry.bbox     = Bbox { x: o.bbox[0], y: o.bbox[1], w: o.bbox[2]-o.bbox[0], h: o.bbox[3]-o.bbox[1] };
                        entry.mask_rle = Some(o.mask_rle.iter().map(|&(a,b)| [a,b]).collect());
                        entry.mask_area= Some(o.mask_area);
                    }
                }

                // ── Aggregate faces (keep highest-confidence entry per face_id) ──
                for f in &output.faces {
                    let entry = face_map.entry(f.face_id.clone()).or_insert_with(|| FaceEntry {
                        face_id: f.face_id.clone(),
                        name:    f.name.clone(),
                        conf:    0.0,
                        bbox:    Bbox { x: f.bbox[0], y: f.bbox[1], w: f.bbox[2]-f.bbox[0], h: f.bbox[3]-f.bbox[1] },
                    });
                    if f.conf > entry.conf {
                        entry.conf = f.conf;
                        entry.name = f.name.clone();
                        entry.bbox = Bbox { x: f.bbox[0], y: f.bbox[1], w: f.bbox[2]-f.bbox[0], h: f.bbox[3]-f.bbox[1] };
                    }
                }

                // ── Embedding dedup + store ─────────────────────────────────────
                if !output.vision_embedding.is_empty() {
                    let is_dup = stored_embeddings.iter()
                        .any(|prev| cosine_similarity(prev, &output.vision_embedding) >= DEDUP_THRESHOLD);

                    if is_dup {
                        log_info!("  ⏭  Frame {} near-duplicate — embedding skipped", frame_idx);
                    } else {
                        let db_guard = db.lock().await;
                        if let Some(ref sdb) = *db_guard {
                            if let Err(e) = DbOperations::insert_embedding(
                                sdb, media_id, "video_frame",
                                Some(timestamp), Some(*frame_idx as u32),
                                output.vision_embedding.clone(),
                            ).await {
                                log_warn!("  ⚠️ insert_embedding frame {}: {}", frame_idx, e);
                            } else {
                                embed_count += 1;
                                stored_embeddings.push(output.vision_embedding);
                                log_info!("  ✅ Frame {} @ {:.2}s embedded", frame_idx, timestamp);
                            }
                        }
                    }
                }

                let _ = job_idx; // suppress unused warning
            }
            Err(e) => {
                log_warn!("  ⚠️ AI error frame {}: {}", frame_idx, e);
            }
        }
    }

    // ── 4. Store aggregated objects + faces in media record ───────────────────
    let obj_count  = obj_map.len();
    let face_count = face_map.len();
    let objects: Vec<ObjectEntry> = obj_map.into_values().collect();
    let faces: Vec<FaceEntry>     = face_map.into_values().collect();

    let detected_faces_for_person: Vec<(String, f32, Bbox, Option<String>)> = faces.iter()
        .map(|f| (f.face_id.clone(), f.conf, f.bbox.clone(), f.name.clone()))
        .collect();

    {
        let db_guard = db.lock().await;
        if let Some(ref sdb) = *db_guard {
            if let Err(e) = DbOperations::update_media_ai(sdb, media_id, objects, faces).await {
                log_warn!("⚠️ update_media_ai for video {}: {}", media_id, e);
            }
            for (fid, conf, bbox, name) in &detected_faces_for_person {
                if let Err(e) = DbOperations::upsert_person(sdb, PersonDoc {
                    face_id:   fid.clone(),
                    name:      name.clone(),
                    thumbnail: Some(format!("{}.thumb.jpg", stem)),
                    conf:      Some(*conf),
                    face_bbox: Some(bbox.clone()),
                }).await {
                    log_warn!("  ⚠️ upsert_person {} for video: {}", fid, e);
                }
            }
        }
    }

    // ── 5. Generate thumbnail from the first processed frame (less likely to be pure black) ──
    let thumb_name = format!("{}.thumb.jpg", stem);
    let thumb_path = Path::new(video_path)
        .parent()
        .unwrap_or(Path::new("."))
        .join(&thumb_name);

    // Prefer the first frame we actually extracted & processed; fall back to frame 0.
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

    // Cleanup temp frames
    let _ = std::fs::remove_dir_all(&tmp_dir);

    log_info!(
        "🎥 Video done: {} embeds, {} objects, {} faces | {}",
        embed_count, obj_count, face_count, video_path
    );

    Ok(thumb_result)
}

// ─── Private helpers ─────────────────────────────────────────────────────────

fn probe_video(video_path: &str) -> Result<(f64, u64)> {
    let fps_out = Command::new("ffprobe")
        .args(["-v","error","-select_streams","v:0","-show_entries",
               "stream=r_frame_rate","-of","default=noprint_wrappers=1:nokey=1", video_path])
        .output()?;
    let fps = parse_fraction(String::from_utf8_lossy(&fps_out.stdout).trim())
        .unwrap_or(30.0);

    let frames_out = Command::new("ffprobe")
        .args(["-v","error","-select_streams","v:0","-show_entries",
               "stream=nb_frames","-of","default=noprint_wrappers=1:nokey=1", video_path])
        .output()?;
    let frames_str = String::from_utf8_lossy(&frames_out.stdout);
    let total = frames_str.trim().parse::<u64>().unwrap_or_else(|_| {
        let dur_out = Command::new("ffprobe")
            .args(["-v","error","-show_entries","format=duration",
                   "-of","default=noprint_wrappers=1:nokey=1", video_path])
            .output()
            .unwrap_or_else(|_| std::process::Output {
                status: unsafe { std::mem::zeroed() }, stdout: vec![], stderr: vec![],
            });
        let dur: f64 = String::from_utf8_lossy(&dur_out.stdout).trim().parse().unwrap_or(0.0);
        (dur * fps) as u64
    });
    Ok((fps, total))
}

fn detect_scenes(video_path: &str, fps: f64) -> Result<Vec<u64>> {
    let filter = format!("select=gt(scene\\,{}),showinfo", SCENE_THRESHOLD);
    let output = Command::new("ffmpeg")
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

fn extract_frame(video_path: &str, frame_idx: u64, fps: f64, out: &Path) -> Result<()> {
    let ts  = format!("{:.6}", frame_idx as f64 / fps);
    let status = Command::new("ffmpeg")
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

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f32 = a.iter().zip(b).map(|(x,y)| x*y).sum();
    let na: f32  = a.iter().map(|x| x*x).sum::<f32>().sqrt();
    let nb: f32  = b.iter().map(|x| x*x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}
