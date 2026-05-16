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
    AppConfig, MODEL_VISION_REL, MODEL_TEXT_REL, MODEL_YOLO_REL, MODEL_YUNET_REL, MODEL_SFACE_REL,
    TOKENIZER_VOCAB_REL, TOKENIZER_BPE_REL,
};
use crate::{log_info, log_warn};
use opencv::{
    imgcodecs::{imdecode, IMREAD_COLOR, IMREAD_IGNORE_ORIENTATION},
    prelude::*,
};

#[derive(Debug, Clone)]
pub struct EngineOutput {
    pub objects:          Vec<DetectionRecord>,
    pub faces:            Vec<FaceGroup>,
    pub vision_embedding: Vec<f32>,
}

fn default_config() -> EngineConfig {
    EngineConfig {
        vision_path:  format!("assets/{}", MODEL_VISION_REL),
        text_path:    format!("assets/{}", MODEL_TEXT_REL),
        yolo_path:    format!("assets/{}", MODEL_YOLO_REL),
        yunet_path:   format!("assets/{}", MODEL_YUNET_REL),
        sface_path:   format!("assets/{}", MODEL_SFACE_REL),
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
        yunet_path:   format!("{}/{}", model_dir, MODEL_YUNET_REL),
        sface_path:   format!("{}/{}", model_dir, MODEL_SFACE_REL),
        vocab_path:   format!("{}/{}", model_dir, TOKENIZER_VOCAB_REL),
        bpe_path:     format!("{}/{}", model_dir, TOKENIZER_BPE_REL),
        face_db_path: format!("{}/face_db", model_dir),
    }
}

pub struct EngineConfig {
    pub vision_path: String,
    pub text_path: String,
    pub yolo_path: String,
    pub yunet_path: String,
    pub sface_path: String,
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
            yunet_path: base.join(MODEL_YUNET_REL).to_string_lossy().into_owned(),
            sface_path: base.join(MODEL_SFACE_REL).to_string_lossy().into_owned(),
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
    yolo_confidence: f32,
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
        let num_threads = app_cfg.num_threads;

        log_info!("loading ai models | threads: {}", num_threads);
        let aura = AuraModel::new(&config.vision_path, &config.text_path, num_threads)?;
        let text_proc = TextProcessor::new(&config.vocab_path, &config.bpe_path)?;
        let yolo = YoloModel::new(&config.yolo_path, num_threads)?;
        
        let mut face = match FaceModel::new(&config.yunet_path, &config.sface_path, num_threads) {
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
            yolo_confidence: app_cfg.yolo_confidence,
            yolo_iou: app_cfg.yolo_iou,
            face_detection_threshold: app_cfg.face_detection_threshold,
            face_identity_threshold: app_cfg.face_identity_threshold,
        })
    }

    /// Run AI pipeline on a single image and return structured output (no disk I/O redundancy).
    pub fn process_image(&mut self, img_path: &str) -> Result<EngineOutput> {
        // Optimization: Read once from disk, decode once into 'image' crate, 
        // then use bytes directly for OpenCV to avoid 3 separate disk reads.
        let bytes = std::fs::read(img_path)?;
        let img = image::load_from_memory(&bytes)?;

        // 1. Vision embedding
        let vision_emb = self.aura.encode_image(preprocess_aura_from_image(&img), 256, 256)
            .unwrap_or_default();

        // 2. YOLO detection + segmentation
        let lb = letterbox_640_from_image(&img);
        let raw = self.yolo.detect(lb.blob.clone())?;
        let objects = YoloProcessor::postprocess(&raw, &lb, self.yolo_confidence, self.yolo_iou);

        // 3. Face detection
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
                        f.face_id = id;
                    } else {
                        let new_id = Uuid::new_v4().to_string();
                        f.face_id = new_id.clone();
                        self.session_faces.push((f.embedding.clone(), new_id));
                    }
                }
            }
        }

        Ok(EngineOutput { objects, faces, vision_embedding: vision_emb })
    }
}
