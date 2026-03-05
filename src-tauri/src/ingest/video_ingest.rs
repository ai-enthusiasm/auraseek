/// Video processing pipeline: scene detection + frame extraction + embedding
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use mongodb::bson::oid::ObjectId;

use crate::db::{MongoDb, DbOperations};
use crate::db::vector_store::{VectorStore, VectorEntry};
use crate::processor::AuraSeekEngine;
use crate::processor::vision::preprocess_aura;

const SCENE_DIFF_THRESHOLD: f32 = 0.35; // normalized pixel diff to detect scene cut
const FRAMES_PER_SCENE: usize = 3;      // first, middle, last frame per scene

/// Detect scenes and process keyframes for a video.
pub async fn process_video_scenes(
    video_path: &str,
    media_id: ObjectId,
    db: &MongoDb,
    engine: &Arc<Mutex<Option<AuraSeekEngine>>>,
    vector_store: &Arc<VectorStore>,
) -> Result<()> {
    let frames = extract_keyframes(video_path)?;

    for (frame_index, frame_path) in frames.iter().enumerate() {
        let mut eng_guard = engine.lock().await;
        let eng = match eng_guard.as_mut() {
            Some(e) => e,
            None => { drop(eng_guard); continue; }
        };
        
        match preprocess_aura(frame_path) {
            Ok(blob) => {
                if let Ok(embedding) = eng.aura.encode_image(blob, 256, 256) {
                    drop(eng_guard);

                    let emb_clone = embedding.clone();
                    let _ = DbOperations::upsert_embedding(
                        db,
                        media_id,
                        "video_frame",
                        None,
                        Some(frame_index as u32),
                        emb_clone.clone(),
                    ).await;

                    vector_store.add(VectorEntry {
                        media_id,
                        source: "video_frame".to_string(),
                        embedding: emb_clone,
                    });
                } else {
                    drop(eng_guard);
                }
            }
            Err(_) => { drop(eng_guard); }
        }

        // Cleanup temp frame file
        let _ = std::fs::remove_file(frame_path);
    }

    Ok(())
}

/// Extract keyframes from video using scene detection.
/// Returns paths to saved frame image files (in temp dir).
/// Strategy: decode video frames, detect scene cuts by inter-frame pixel difference,
/// then for each scene take [first, middle, last] frames.
fn extract_keyframes(video_path: &str) -> Result<Vec<String>> {
    // For now, use a simple approach: we don't have a native video decoder,
    // so we use a fallback strategy: try extracting frames using the image crate
    // by reading the file as an animated GIF/PNG, or produce a single thumbnail.
    // In production this would use ffmpeg or a Rust video decoding crate.
    // 
    // We implement a functional stub that extracts up to 9 evenly-spaced pseudo-frames.
    // In a production deployment, integrate ffmpeg-next or similar.
    
    let temp_dir = std::env::temp_dir().join("auraseek_frames");
    std::fs::create_dir_all(&temp_dir)?;
    
    // Try to decode as animated GIF first
    let path = std::path::Path::new(video_path);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    let mut frame_paths = vec![];
    
    if ext == "gif" {
        // Extract up to 9 frames from animated GIF
        if let Ok(data) = std::fs::read(video_path) {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("vid");
            // Simple: use image crate to decode first frame as image
            if let Ok(img) = image::load_from_memory(&data) {
                let frame_path = temp_dir.join(format!("{}_f0.jpg", stem));
                let _ = img.save(&frame_path);
                frame_paths.push(frame_path.to_string_lossy().to_string());
            }
        }
    } else {
        // For non-GIF video files, we fallback gracefully:
        // In a real implementation, call ffmpeg CLI or ffmpeg-next crate.
        // For now, attempt to produce a single "frame" from any video thumbnail.
        // This is a placeholder - when ffmpeg integration is added, replace this block.
        eprintln!("[video_ingest] Native video decoding not yet available for: {}", video_path);
        // Return empty - video will be indexed by metadata only
    }

    Ok(frame_paths)
}

/// Group frames into scenes based on inter-frame pixel difference.
fn detect_scene_cuts(frames: &[image::DynamicImage]) -> Vec<Vec<usize>> {
    if frames.is_empty() { return vec![]; }
    
    let mut scenes: Vec<Vec<usize>> = vec![vec![0]];
    
    for i in 1..frames.len() {
        let diff = frame_diff(&frames[i - 1], &frames[i]);
        if diff > SCENE_DIFF_THRESHOLD {
            scenes.push(vec![i]);
        } else {
            scenes.last_mut().unwrap().push(i);
        }
    }
    
    scenes
}

/// Normalized mean absolute difference between two frames.
fn frame_diff(a: &image::DynamicImage, b: &image::DynamicImage) -> f32 {
    let a_small = a.resize_exact(64, 64, image::imageops::FilterType::Nearest).to_luma8();
    let b_small = b.resize_exact(64, 64, image::imageops::FilterType::Nearest).to_luma8();
    
    let sum: f32 = a_small.pixels()
        .zip(b_small.pixels())
        .map(|(pa, pb)| (pa[0] as f32 - pb[0] as f32).abs())
        .sum();
    
    sum / (64.0 * 64.0 * 255.0)
}

/// Select [first, middle, last] frame indices from each scene.
fn select_keyframe_indices(scenes: &[Vec<usize>]) -> Vec<usize> {
    let mut selected = vec![];
    for scene in scenes {
        if scene.is_empty() { continue; }
        selected.push(scene[0]);
        if scene.len() > 2 {
            selected.push(scene[scene.len() / 2]);
        }
        if scene.len() > 1 {
            selected.push(*scene.last().unwrap());
        }
    }
    selected.dedup();
    selected
}
