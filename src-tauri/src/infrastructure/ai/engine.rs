use anyhow::Result;
use uuid::Uuid;

use super::text::{AuraModel, TextProcessor};
use super::vision::{
    FaceModel, FaceGroup,
    FaceDb, cosine_similarity,
    YoloModel, YoloProcessor,
    letterbox_640, preprocess_aura,
    DetectionRecord,
};
use crate::core::config::AppConfig;
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
        vision_path:  "assets/models/vision_visclir.onnx".into(),
        text_path:    "assets/models/text_visclir.onnx".into(),
        yolo_path:    "assets/models/yolo26n-seg.onnx".into(),
        yunet_path:   "assets/models/face_detection_yunet_2022mar.onnx".into(),
        sface_path:   "assets/models/face_recognition_sface_2021dec.onnx".into(),
        vocab_path:   "assets/tokenizer/vocab.txt".into(),
        bpe_path:     "assets/tokenizer/bpe.codes".into(),
        face_db_path: "assets/face_db".into(),
    }
}

pub fn config_from_model_dir(model_dir: &str) -> EngineConfig {
    EngineConfig {
        vision_path:  format!("{}/models/vision_visclir.onnx", model_dir),
        text_path:    format!("{}/models/text_visclir.onnx", model_dir),
        yolo_path:    format!("{}/models/yolo26n-seg.onnx", model_dir),
        yunet_path:   format!("{}/models/face_detection_yunet_2022mar.onnx", model_dir),
        sface_path:   format!("{}/models/face_recognition_sface_2021dec.onnx", model_dir),
        vocab_path:   format!("{}/tokenizer/vocab.txt", model_dir),
        bpe_path:     format!("{}/tokenizer/bpe.codes", model_dir),
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
            vision_path: base.join("models/vision_visclir.onnx").to_string_lossy().into_owned(),
            text_path: base.join("models/text_visclir.onnx").to_string_lossy().into_owned(),
            yolo_path: base.join("models/yolo26n-seg.onnx").to_string_lossy().into_owned(),
            yunet_path: base.join("models/face_detection_yunet_2022mar.onnx").to_string_lossy().into_owned(),
            sface_path: base.join("models/face_recognition_sface_2021dec.onnx").to_string_lossy().into_owned(),
            vocab_path: base.join("tokenizer/vocab.txt").to_string_lossy().into_owned(),
            bpe_path: base.join("tokenizer/bpe.codes").to_string_lossy().into_owned(),
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

        log_info!("loading ai models");
        let aura = AuraModel::new(&config.vision_path, &config.text_path)?;
        let text_proc = TextProcessor::new(&config.vocab_path, &config.bpe_path)?;
        let yolo = YoloModel::new(&config.yolo_path)?;
        
        let mut face = match FaceModel::new(&config.yunet_path, &config.sface_path) {
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

    /// Run AI pipeline on a single image and return structured output (no disk I/O).
    pub fn process_image(&mut self, img_path: &str) -> Result<EngineOutput> {
        // 1. Vision embedding
        let vision_emb = self.aura.encode_image(preprocess_aura(img_path)?, 256, 256)
            .unwrap_or_default();

        // 2. YOLO detection + segmentation
        let lb = letterbox_640(img_path)?;
        let raw = self.yolo.detect(lb.blob.clone())?;
        let objects = YoloProcessor::postprocess(&raw, &lb, self.yolo_confidence, self.yolo_iou);

        // 3. Face detection — prefer imread path (same behavior as debug pipeline),
        // then fallback to explicit imdecode for robustness.
        let mut faces = vec![];
        if let Some(ref mut fm) = self.face {
            fm.set_score_threshold(self.face_detection_threshold);
            match fm.detect_from_path(img_path, &self.face_db, self.face_identity_threshold) {
                Ok(detected) => faces = detected,
                Err(path_err) => {
                    log_warn!(
                        "face detect_from_path failed: {} | file={}",
                        path_err,
                        img_path
                    );
                    match std::fs::read(img_path) {
                        Ok(bytes) => {
                            let buf = opencv::core::Vector::<u8>::from_iter(bytes);
                            match imdecode(&buf, IMREAD_COLOR | IMREAD_IGNORE_ORIENTATION) {
                                Ok(frame) if !frame.empty() => {
                                    match fm.detect_from_mat(&frame, &self.face_db, self.face_identity_threshold) {
                                        Ok(detected) => faces = detected,
                                        Err(e) => log_warn!(
                                            "face detect_from_mat failed: {} | file={}",
                                            e,
                                            img_path
                                        ),
                                    }
                                }
                                Ok(_) => log_warn!("face detect: decoded empty Mat | file={}", img_path),
                                Err(e) => log_warn!("face detect: imdecode failed: {} | file={}", e, img_path),
                            }
                        }
                        Err(e) => log_warn!("face detect: read file failed: {} | file={}", e, img_path),
                    }
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
