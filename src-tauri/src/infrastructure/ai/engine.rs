use anyhow::Result;
use uuid::Uuid;

use super::text::{AuraModel, TextProcessor};
use super::vision::{
    FaceModel, FaceGroup,
    FaceDb, cosine_similarity,
    YoloModel, YoloProcessor,
    letterbox_640_from_image, preprocess_aura_from_image,
    DetectionRecord,
};
use crate::core::config::{
    AppConfig, MODEL_VISION_REL, MODEL_TEXT_REL, MODEL_YOLO_REL, MODEL_SCRFD_REL, MODEL_ARCFACE_REL,
    TOKENIZER_VOCAB_REL, TOKENIZER_BPE_REL,
};
use crate::{log_info, log_warn};
use opencv::{
    imgcodecs::{imdecode, IMREAD_COLOR, IMREAD_IGNORE_ORIENTATION},
    prelude::*,
};

#[derive(Debug)]
pub struct EngineOutput {
    pub objects:          Vec<DetectionRecord>,
    pub faces:            Vec<FaceGroup>,
    pub vision_embedding: Vec<f32>,
    /// The decoded image from disk — reused by thumbnail generator to avoid re-read.
    pub decoded_image:    Option<image::DynamicImage>,
    pub dur_total:        u128,
}

fn default_config() -> EngineConfig {
    EngineConfig {
        vision_path:  format!("assets/{}", MODEL_VISION_REL),
        text_path:    format!("assets/{}", MODEL_TEXT_REL),
        yolo_path:    format!("assets/{}", MODEL_YOLO_REL),
        scrfd_path:   format!("assets/{}", MODEL_SCRFD_REL),
        arcface_path: format!("assets/{}", MODEL_ARCFACE_REL),
        vocab_path:   format!("assets/{}", TOKENIZER_VOCAB_REL),
        bpe_path:     format!("assets/{}", TOKENIZER_BPE_REL),
        face_db_path: "assets/face_db".into(),
    }
}

pub fn config_from_model_dir(model_dir: &str) -> EngineConfig {
    EngineConfig {
        vision_path:  format!("{}/{}", model_dir, MODEL_VISION_REL),
        text_path:    format!("{}/{}", model_dir, MODEL_TEXT_REL),
        yolo_path:    format!("{}/{}", model_dir, MODEL_YOLO_REL),
        scrfd_path:   format!("{}/{}", model_dir, MODEL_SCRFD_REL),
        arcface_path: format!("{}/{}", model_dir, MODEL_ARCFACE_REL),
        vocab_path:   format!("{}/{}", model_dir, TOKENIZER_VOCAB_REL),
        bpe_path:     format!("{}/{}", model_dir, TOKENIZER_BPE_REL),
        face_db_path: format!("{}/face_db", model_dir),
    }
}

pub struct EngineConfig {
    pub vision_path: String,
    pub text_path: String,
    pub yolo_path: String,
    pub scrfd_path: String,
    pub arcface_path: String,
    pub vocab_path: String,
    pub bpe_path: String,
    pub face_db_path: String,
}

impl EngineConfig {
    pub fn new_with_dir(base: &std::path::Path) -> Self {
        Self {
            vision_path: base.join(MODEL_VISION_REL).to_string_lossy().into_owned(),
            text_path: base.join(MODEL_TEXT_REL).to_string_lossy().into_owned(),
            yolo_path: base.join(MODEL_YOLO_REL).to_string_lossy().into_owned(),
            scrfd_path: base.join(MODEL_SCRFD_REL).to_string_lossy().into_owned(),
            arcface_path: base.join(MODEL_ARCFACE_REL).to_string_lossy().into_owned(),
            vocab_path: base.join(TOKENIZER_VOCAB_REL).to_string_lossy().into_owned(),
            bpe_path: base.join(TOKENIZER_BPE_REL).to_string_lossy().into_owned(),
            face_db_path: base.join("face_db").to_string_lossy().into_owned(),
        }
    }

    pub fn from_app_config(cfg: &AppConfig) -> Self {
        Self::new_with_dir(&cfg.model_dir)
    }
}

pub struct AuraSeekEngine {
    pub aura: AuraModel,
    #[allow(dead_code)]
    pub text_proc: TextProcessor,
    pub yolo: YoloModel,
    pub face: Option<FaceModel>,
    pub face_db: FaceDb,
    pub session_faces: Vec<(Vec<f32>, String)>,
    yolo_score_high: f32,
    yolo_score_min: f32,
    yolo_iou: f32,
    /// Minimum confidence to consider a crop as a face.
    pub face_detection_threshold: f32,
    /// Cosine threshold for face identity matching.
    pub face_identity_threshold: f32,
}

impl AuraSeekEngine {
    pub fn new_default() -> Result<Self> {
        Self::new(default_config())
    }

    pub fn new(config: EngineConfig) -> Result<Self> {
        let app_cfg = AppConfig::global();
        Self::new_with_threads(config, app_cfg.num_threads)
    }

    pub fn new_with_threads(config: EngineConfig, num_threads: usize) -> Result<Self> {
        let app_cfg = AppConfig::global();

        log_info!("loading ai models | threads: {}", num_threads);
        let aura = AuraModel::new(&config.vision_path, &config.text_path, num_threads)?;
        let text_proc = TextProcessor::new(&config.vocab_path, &config.bpe_path)?;
        let yolo = YoloModel::new(&config.yolo_path, num_threads)?;
        
        let mut face = match FaceModel::new(&config.scrfd_path, &config.arcface_path, num_threads) {
            Ok(m) => Some(m),
            Err(e) => {
                log_warn!("face model failed to load: {}", e);
                None
            }
        };

        let face_db = if let Some(ref mut fm) = face {
            FaceDb::build(&config.face_db_path, fm).unwrap_or_else(|_| FaceDb::empty())
        } else {
            FaceDb::empty()
        };

        Ok(Self {
            aura,
            text_proc,
            yolo,
            face,
            face_db,
            session_faces: Vec::new(),
            yolo_score_high: app_cfg.yolo_score_high,
            yolo_score_min: app_cfg.yolo_score_min,
            yolo_iou: app_cfg.yolo_iou,
            face_detection_threshold: app_cfg.face_detection_threshold,
            face_identity_threshold: app_cfg.face_identity_threshold,
        })
    }

    /// Run AI pipeline on a single image and return structured output (no disk I/O redundancy).
    pub async fn process_image(
        &mut self,
        img_path: &str,
        qdrant: Option<&qdrant_client::Qdrant>,
    ) -> Result<EngineOutput> {
        let t_total = std::time::Instant::now();

        // 1. Image loading & decoding
        let t_load = std::time::Instant::now();
        let bytes = std::fs::read(img_path)?;
        let img = image::load_from_memory(&bytes)?;
        let dur_load = t_load.elapsed().as_millis();

        // 2. Vision embedding
        let t_embed = std::time::Instant::now();
        let vision_emb = self.aura.encode_image(preprocess_aura_from_image(&img), 256, 256)
            .unwrap_or_default();
        let dur_embed = t_embed.elapsed().as_millis();

        // 3. YOLO detection + segmentation
        let t_yolo = std::time::Instant::now();
        let lb = letterbox_640_from_image(&img);
        let raw = self.yolo.detect(lb.blob.clone())?;
        let raw_objects = YoloProcessor::postprocess(&raw, &lb, self.yolo_score_min, self.yolo_iou);
        let objects = YoloProcessor::apply_dual_threshold(raw_objects, self.yolo_score_high, self.yolo_score_min);
        let dur_yolo = t_yolo.elapsed().as_millis();

        // 4. Face detection
        let t_face = std::time::Instant::now();
        let mut faces = vec![];
        if let Some(ref mut fm) = self.face {
            fm.set_score_threshold(self.face_detection_threshold);
            
            // Reuse pre-read bytes to build an OpenCV Mat without disk I/O
            let buf = opencv::core::Vector::<u8>::from_iter(bytes);
            match imdecode(&buf, IMREAD_COLOR | IMREAD_IGNORE_ORIENTATION) {
                Ok(frame) if !frame.empty() => {
                    match fm.detect_from_mat(&frame, &self.face_db, self.face_identity_threshold) {
                        Ok(detected) => faces = detected,
                        Err(e) => log_warn!("face detect_from_mat failed: {} | file={}", e, img_path),
                    }
                }
                _ => {
                    // Final fallback to path if imdecode fails for some reason
                    let _ = fm.detect_from_path(img_path, &self.face_db, self.face_identity_threshold)
                        .map(|detected| faces = detected);
                }
            }

            if faces.is_empty() {
                log_info!("face detect: no face found | file={}", img_path);
            }

            // Session face matching for unknown faces
            for f in faces.iter_mut() {
                if f.face_id == "unknown_placeholder" {
                    let mut matched_id = None;

                    // 1. Check Qdrant first if available
                    if let Some(client) = qdrant {
                        match crate::infrastructure::database::DbOperations::vector_search_face(
                            client,
                            crate::core::config::QDRANT_FACE_COLLECTION,
                            &f.embedding,
                            self.face_identity_threshold,
                            1,
                        ).await {
                            Ok(hits) => {
                                if let Some((face_id, score)) = hits.first() {
                                    log_info!("  👤 Qdrant face match found: {} (score: {:.3})", face_id, score);
                                    matched_id = Some(face_id.clone());
                                }
                            }
                            Err(e) => {
                                log_warn!("  ⚠️ Qdrant face search failed: {:#}", e);
                            }
                        }
                    }

                    // 2. Check local session cache if not matched in Qdrant
                    if matched_id.is_none() {
                        let mut best_score = self.face_identity_threshold;
                        let mut cached_id = None;
                        for (cached_emb, id) in &self.session_faces {
                            let score = cosine_similarity(&f.embedding, cached_emb);
                            if score > best_score {
                                best_score = score;
                                cached_id = Some(id.clone());
                            }
                        }
                        if let Some(id) = cached_id {
                            log_info!("  👤 Session face match found: {}", id);
                            matched_id = Some(id);
                        }
                    }

                    // 3. Fallback to new ID
                    if let Some(id) = matched_id {
                        f.face_id = id;
                    } else {
                        let new_id = Uuid::new_v4().to_string();
                        log_info!("  👤 New face detected, assigning face_id={}", new_id);
                        f.face_id = new_id.clone();
                        self.session_faces.push((f.embedding.clone(), new_id));
                    }
                }
            }
        }
        let dur_face = t_face.elapsed().as_millis();

        let filename = img_path.split('/').last().unwrap_or(img_path);
        let dur_total = t_total.elapsed().as_millis();
        crate::log_info!(
            "⏱️  AI Details for {} | Load: {}ms | CLIP: {}ms | YOLO: {}ms | Face: {}ms | AI Total: {}ms",
            filename,
            dur_load,
            dur_embed,
            dur_yolo,
            dur_face,
            dur_total
        );

        Ok(EngineOutput { objects, faces, vision_embedding: vision_emb, decoded_image: Some(img), dur_total })
    }
}
