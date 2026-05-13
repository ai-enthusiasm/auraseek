/// face detection and recognition using opencv (yunet and sface)
use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use opencv::{
    core::{Mat, Ptr, Size, Vec3b},
    objdetect::FaceRecognizerSF,
    imgcodecs::{imread, IMREAD_COLOR},
    prelude::*,
};
use super::db::FaceDb;
use crate::infrastructure::ai::runtime::build_session;
use crate::log_info;

 
const NMS_THRESHOLD: f32 = 0.3;
const TOP_K: i32 = 5000;
// This YuNet checkpoint expects fixed NCHW input [1,3,120,160].
const YUNET_INPUT_H: usize = 120;
const YUNET_INPUT_W: usize = 160;
const YUNET_VARIANCE_0: f32 = 0.1;
const YUNET_VARIANCE_1: f32 = 0.2;
/// Default cosine similarity for face identity (keep in sync with `AppConfig::default().face_threshold`).
/// Runtime matching uses the `identity_threshold` argument on detect APIs and `AuraSeekEngine.face_threshold`.
#[allow(dead_code)]
pub const COSINE_THRESHOLD: f32 = 0.33;

#[derive(Debug, Clone, serde::Serialize)]
pub struct FaceGroup {
    pub face_id:  String,
    pub name:     Option<String>,
    pub conf:     f32,
    pub bbox:     [f32; 4],
    #[serde(skip)]
    pub embedding: Vec<f32>,
}

struct YuNetFace {
    pub row:   Mat,
    pub bbox:  [f32; 4],
    pub score: f32,
}

#[derive(Clone, Copy)]
struct Prior {
    cx: f32,
    cy: f32,
    sx: f32,
    sy: f32,
}

pub struct FaceModel {
    yunet_path: String,
    recognizer_path: String,
    num_threads: usize,
    backend: i32,
    target: i32,
    yunet_session: Session,
    detector_size: Size,
    recognizer:    Ptr<FaceRecognizerSF>,
    score_threshold: f32,
}

impl FaceModel {
    fn yunet_priors() -> Vec<Prior> {
        // Match YuNet anchor generation used by OpenCV FaceDetectorYN for 160x120 input.
        let steps = [8usize, 16, 32, 64];
        let min_sizes: [&[usize]; 4] = [
            &[10, 16, 24],
            &[32, 48],
            &[64, 96],
            &[128, 192, 256],
        ];
        let mut priors = Vec::new();
        for (idx, step) in steps.iter().enumerate() {
            let fm_w = YUNET_INPUT_W / step;
            let fm_h = YUNET_INPUT_H / step;
            for y in 0..fm_h {
                for x in 0..fm_w {
                    for ms in min_sizes[idx] {
                        let cx = (x as f32 + 0.5) * *step as f32 / YUNET_INPUT_W as f32;
                        let cy = (y as f32 + 0.5) * *step as f32 / YUNET_INPUT_H as f32;
                        let sx = *ms as f32 / YUNET_INPUT_W as f32;
                        let sy = *ms as f32 / YUNET_INPUT_H as f32;
                        priors.push(Prior { cx, cy, sx, sy });
                    }
                }
            }
        }
        priors
    }

    pub fn new(yunet_path: &str, sface_path: &str, num_threads: usize) -> Result<Self> {
        let size = Size::new(320, 320);
        
        #[cfg(target_os = "macos")]
        let (backend, target, provider_name) = (
            opencv::dnn::DNN_BACKEND_OPENCV,
            opencv::dnn::DNN_TARGET_CPU,
            "CPU (forced-macos)",
        );
        #[cfg(not(target_os = "macos"))]
        let (backend, target, provider_name) = if opencv::core::get_cuda_enabled_device_count()? > 0 {
            (opencv::dnn::DNN_BACKEND_CUDA, opencv::dnn::DNN_TARGET_CUDA, "CUDA")
        } else {
            (opencv::dnn::DNN_BACKEND_OPENCV, opencv::dnn::DNN_TARGET_CPU, "CPU")
        };

        log_info!("model: {:<45} | provider: {} | threads: {}", yunet_path, provider_name, num_threads);

        let yunet_session = build_session(yunet_path, num_threads)?;
        let recognizer = FaceRecognizerSF::create(
            sface_path, "", backend, target
        )?;

        Ok(Self {
            yunet_path: yunet_path.to_string(),
            recognizer_path: sface_path.to_string(),
            num_threads,
            backend,
            target,
            yunet_session,
            detector_size: size,
            recognizer,
            score_threshold: 0.93,
        })
    }

    pub fn set_score_threshold(&mut self, threshold: f32) {
        self.score_threshold = threshold;
    }

    fn rebuild_models(&mut self) -> Result<()> {
        self.yunet_session = build_session(&self.yunet_path, self.num_threads)?;
        self.recognizer = FaceRecognizerSF::create(
            &self.recognizer_path,
            "",
            self.backend,
            self.target,
        )?;
        Ok(())
    }

    fn run_yunet_with_retry(&mut self, blob: Vec<f32>, pad_h: usize, pad_w: usize) -> Result<Vec<Vec<f32>>> {
        let run_once = |session: &mut Session, input: Vec<f32>| -> Result<Vec<Vec<f32>>> {
            let input_tensor = Value::from_array((
                vec![1usize, 3, pad_h, pad_w],
                input.into_boxed_slice(),
            ))?;
            let outputs = session.run(ort::inputs!["input" => input_tensor])?;
            let mut out = Vec::with_capacity(outputs.len());
            for i in 0..outputs.len() {
                let (_, data) = outputs[i].try_extract_tensor::<f32>()?;
                out.push(data.to_vec());
            }
            Ok(out)
        };

        match run_once(&mut self.yunet_session, blob.clone()) {
            Ok(v) => Ok(v),
            Err(first_err) => {
                log_info!("face detector recover: rebuilding detector after error: {}", first_err);
                self.rebuild_models()?;
                run_once(&mut self.yunet_session, blob)
            }
        }
    }

    fn detect_faces_raw(&mut self, frame: &Mat) -> Result<Vec<YuNetFace>> {
        if frame.empty() { return Ok(vec![]); }
        let size = frame.size()?;
        let (w, h) = (size.width as usize, size.height as usize);
        if w < 20 || h < 20 { return Ok(vec![]); }

        let ratio_x = w as f32 / YUNET_INPUT_W as f32;
        let ratio_y = h as f32 / YUNET_INPUT_H as f32;
        self.detector_size = Size::new(w as i32, h as i32);

        let mut resized = Mat::default();
        opencv::imgproc::resize(
            frame,
            &mut resized,
            Size::new(YUNET_INPUT_W as i32, YUNET_INPUT_H as i32),
            0.0,
            0.0,
            opencv::imgproc::INTER_LINEAR,
        )?;

        let area = YUNET_INPUT_W * YUNET_INPUT_H;
        let mut blob = vec![0f32; 3 * area];
        for y in 0..YUNET_INPUT_H {
            for x in 0..YUNET_INPUT_W {
                let px: Vec3b = *resized.at_2d::<Vec3b>(y as i32, x as i32)?;
                let idx = y * YUNET_INPUT_W + x;
                // blobFromImage with default settings keeps BGR channel order.
                blob[idx] = px[0] as f32;
                blob[idx + area] = px[1] as f32;
                blob[idx + 2 * area] = px[2] as f32;
            }
        }

        let outputs = self.run_yunet_with_retry(blob, YUNET_INPUT_H, YUNET_INPUT_W)?;
        let mut raw_faces = Vec::new();
        if outputs.len() >= 12 {
            let strides = [8usize, 16, 32];
            for (i, stride) in strides.iter().enumerate() {
                let cols = YUNET_INPUT_W / stride;
                let rows = YUNET_INPUT_H / stride;
                let cls = &outputs[i];
                let obj = &outputs[i + 3];
                let bbox = &outputs[i + 6];
                let kps = &outputs[i + 9];

                for r in 0..rows {
                    for c in 0..cols {
                        let idx = r * cols + c;
                        let cls_score = cls[idx].clamp(0.0, 1.0);
                        let obj_score = obj[idx].clamp(0.0, 1.0);
                        let score = (cls_score * obj_score).sqrt();
                        if score < self.score_threshold {
                            continue;
                        }

                        let cx = (c as f32 + bbox[idx * 4]) * *stride as f32;
                        let cy = (r as f32 + bbox[idx * 4 + 1]) * *stride as f32;
                        let bw = bbox[idx * 4 + 2].exp() * *stride as f32;
                        let bh = bbox[idx * 4 + 3].exp() * *stride as f32;
                        let x1 = cx - bw / 2.0;
                        let y1 = cy - bh / 2.0;
                        let x1o = x1 * ratio_x;
                        let y1o = y1 * ratio_y;
                        let bwo = bw * ratio_x;
                        let bho = bh * ratio_y;

                        let mut row = Mat::zeros(1, 15, opencv::core::CV_32FC1)?.to_mat()?;
                        *row.at_2d_mut::<f32>(0, 0)? = x1o;
                        *row.at_2d_mut::<f32>(0, 1)? = y1o;
                        *row.at_2d_mut::<f32>(0, 2)? = bwo;
                        *row.at_2d_mut::<f32>(0, 3)? = bho;
                        for n in 0..5usize {
                            *row.at_2d_mut::<f32>(0, (4 + 2 * n) as i32)? =
                                ((kps[idx * 10 + 2 * n] + c as f32) * *stride as f32) * ratio_x;
                            *row.at_2d_mut::<f32>(0, (4 + 2 * n + 1) as i32)? =
                                ((kps[idx * 10 + 2 * n + 1] + r as f32) * *stride as f32) * ratio_y;
                        }
                        *row.at_2d_mut::<f32>(0, 14)? = score;

                        raw_faces.push(YuNetFace {
                            row,
                            bbox: [x1o, y1o, bwo, bho],
                            score,
                        });
                    }
                }
            }
        } else {
            // Some YuNet exports return a compact [N, 15]-like tensor (often with 3 outputs total).
            // Decode from the first output and keep behavior equivalent to OpenCV FaceDetectorYN rows.
            let det = &outputs[0];
            log_info!(
                "yunet compact output layout: outputs={}, first_len={}, second_len={}, third_len={}",
                outputs.len(),
                det.len(),
                outputs.get(1).map(|v| v.len()).unwrap_or(0),
                outputs.get(2).map(|v| v.len()).unwrap_or(0)
            );
            if outputs.len() == 3
                && det.len() % 14 == 0
                && outputs[2].len() == det.len() / 14
                && (outputs[1].len() == det.len() / 14 || outputs[1].len() == 2 * (det.len() / 14))
            {
                // Layout variant A: loc [N,14], cls [N], iou [N]
                // Layout variant B: loc [N,14], cls [N,2], iou [N]
                let n = det.len() / 14;
                let cls = &outputs[1];
                let obj = &outputs[2];
                let priors = Self::yunet_priors();
                if priors.len() != n {
                    return Err(anyhow!(
                        "yunet prior mismatch: priors={} outputs_n={}",
                        priors.len(),
                        n
                    ));
                }
                log_info!(
                    "yunet compact decode: n={}, cls_len={}, obj_len={}, priors={}",
                    n,
                    cls.len(),
                    obj.len(),
                    priors.len()
                );
                for i in 0..n {
                    let base = i * 14;
                    let cls_score = if cls.len() == n {
                        cls[i].clamp(0.0, 1.0)
                    } else {
                        // [bg, face] pair per anchor. Treat values as logits if outside [0,1].
                        let bg = cls[2 * i];
                        let face = cls[2 * i + 1];
                        if (0.0..=1.0).contains(&bg) && (0.0..=1.0).contains(&face) {
                            face
                        } else {
                            let m = bg.max(face);
                            let eb = (bg - m).exp();
                            let ef = (face - m).exp();
                            ef / (eb + ef + 1e-6)
                        }
                    };
                    let obj_score = obj[i].clamp(0.0, 1.0);
                    let score = (cls_score * obj_score).sqrt();
                    if score < self.score_threshold {
                        continue;
                    }

                    let p = priors[i];
                    let cx = p.cx + det[base] * YUNET_VARIANCE_0 * p.sx;
                    let cy = p.cy + det[base + 1] * YUNET_VARIANCE_0 * p.sy;
                    let bw = p.sx * (det[base + 2] * YUNET_VARIANCE_1).exp();
                    let bh = p.sy * (det[base + 3] * YUNET_VARIANCE_1).exp();
                    let x1 = (cx - bw / 2.0) * YUNET_INPUT_W as f32;
                    let y1 = (cy - bh / 2.0) * YUNET_INPUT_H as f32;
                    let bw_px = bw * YUNET_INPUT_W as f32;
                    let bh_px = bh * YUNET_INPUT_H as f32;
                    let x1o = x1 * ratio_x;
                    let y1o = y1 * ratio_y;
                    let bwo = bw_px * ratio_x;
                    let bho = bh_px * ratio_y;

                    let mut row = Mat::zeros(1, 15, opencv::core::CV_32FC1)?.to_mat()?;
                    *row.at_2d_mut::<f32>(0, 0)? = x1o;
                    *row.at_2d_mut::<f32>(0, 1)? = y1o;
                    *row.at_2d_mut::<f32>(0, 2)? = bwo;
                    *row.at_2d_mut::<f32>(0, 3)? = bho;
                    for p in 0..5usize {
                        let lmx = (priors[i].cx + det[base + 4 + 2 * p] * YUNET_VARIANCE_0 * priors[i].sx)
                            * YUNET_INPUT_W as f32
                            * ratio_x;
                        let lmy = (priors[i].cy + det[base + 4 + 2 * p + 1] * YUNET_VARIANCE_0 * priors[i].sy)
                            * YUNET_INPUT_H as f32
                            * ratio_y;
                        *row.at_2d_mut::<f32>(0, (4 + 2 * p) as i32)? = lmx;
                        *row.at_2d_mut::<f32>(0, (4 + 2 * p + 1) as i32)? = lmy;
                    }
                    *row.at_2d_mut::<f32>(0, 14)? = score;
                    raw_faces.push(YuNetFace {
                        row,
                        bbox: [x1o, y1o, bwo, bho],
                        score,
                    });
                }
            } else if det.len() % 15 == 0 {
                for chunk in det.chunks_exact(15) {
                    let score = chunk[14];
                    if score < self.score_threshold {
                        continue;
                    }
                    let x1o = chunk[0] * ratio_x;
                    let y1o = chunk[1] * ratio_y;
                    let bwo = chunk[2] * ratio_x;
                    let bho = chunk[3] * ratio_y;

                    let mut row = Mat::zeros(1, 15, opencv::core::CV_32FC1)?.to_mat()?;
                    *row.at_2d_mut::<f32>(0, 0)? = x1o;
                    *row.at_2d_mut::<f32>(0, 1)? = y1o;
                    *row.at_2d_mut::<f32>(0, 2)? = bwo;
                    *row.at_2d_mut::<f32>(0, 3)? = bho;
                    for n in 0..5usize {
                        *row.at_2d_mut::<f32>(0, (4 + 2 * n) as i32)? = chunk[4 + 2 * n] * ratio_x;
                        *row.at_2d_mut::<f32>(0, (4 + 2 * n + 1) as i32)? = chunk[4 + 2 * n + 1] * ratio_y;
                    }
                    *row.at_2d_mut::<f32>(0, 14)? = score;
                    raw_faces.push(YuNetFace {
                        row,
                        bbox: [x1o, y1o, bwo, bho],
                        score,
                    });
                }
            } else {
                return Err(anyhow!(
                    "unsupported YuNet output layout: {} outputs, lens=({},{},{})",
                    outputs.len(),
                    outputs[0].len(),
                    outputs.get(1).map(|v| v.len()).unwrap_or(0),
                    outputs.get(2).map(|v| v.len()).unwrap_or(0)
                ));
            }
        }

        raw_faces.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        let mut kept: Vec<YuNetFace> = Vec::new();
        for f in raw_faces {
            let mut overlap = false;
            for k in &kept {
                if calc_iou(&f.bbox, &k.bbox) > 0.3 {
                    overlap = true;
                    break;
                }
            }
            if !overlap { kept.push(f); }
        }
        Ok(kept)
    }

    /// Detect faces from an already-loaded Mat (e.g. a person crop).
    ///
    /// To avoid OpenCV DNN shape errors on some extreme aspect ratios, we
    /// always resize to the fixed detector input size that was configured
    /// when the model was created (320x320). Dynamic resizing of the
    /// detector network has been removed for stability.
    ///
    /// Returns FaceGroups with bbox in the coordinate space of the input frame.
    /// `identity_threshold`: cosine similarity for matching `face_db` (must match
    /// the threshold used when clustering unknown faces in the engine session).
    pub fn detect_from_mat(
        &mut self,
        frame: &Mat,
        db: &FaceDb,
        identity_threshold: f32,
    ) -> Result<Vec<FaceGroup>> {
        let kept = self.detect_faces_raw(frame)?;

        let mut groups = Vec::new();
        for face in kept.into_iter() {
            let mut aligned = Mat::default();
            self.recognizer.align_crop(frame, &face.row, &mut aligned)?;
            
            let mut feature = Mat::default();
            self.recognizer.feature(&aligned, &mut feature)?;
            let embedding = mat_to_vec_f32(&feature)?;

            let (name, face_id) = match db.query_id(&embedding, identity_threshold) {
                Some((n, id)) => (Some(n), id),
                None => (None, "unknown_placeholder".to_string()),
            };

            groups.push(FaceGroup {
                face_id,
                name,
                conf: face.score,
                bbox: [
                    face.bbox[0], 
                    face.bbox[1], 
                    face.bbox[0] + face.bbox[2], 
                    face.bbox[1] + face.bbox[3]
                ],
                embedding,
            });
        }
        Ok(groups)
    }

    /// Convenience: load image from path and detect faces on the full image.
    pub fn detect_from_path(
        &mut self,
        path: &str,
        db: &FaceDb,
        identity_threshold: f32,
    ) -> Result<Vec<FaceGroup>> {
        let frame = imread(path, IMREAD_COLOR)?;
        self.detect_from_mat(&frame, db, identity_threshold)
    }

    /// Same as `detect_from_mat` but also returns the aligned Mat crop (112×112)
    /// for each detected face, so callers can save it for debugging.
    pub fn detect_from_mat_with_aligned(
        &mut self,
        frame: &Mat,
        db: &FaceDb,
        identity_threshold: f32,
    ) -> Result<Vec<(FaceGroup, Mat)>> {
        let kept = self.detect_faces_raw(frame)?;
        let mut out = Vec::new();
        for face in kept {
            let mut aligned = Mat::default();
            self.recognizer.align_crop(frame, &face.row, &mut aligned)?;
            let mut feature = Mat::default();
            self.recognizer.feature(&aligned, &mut feature)?;
            let embedding = mat_to_vec_f32(&feature)?;
            let (name, face_id) = match db.query_id(&embedding, identity_threshold) {
                Some((n, id)) => (Some(n), id),
                None => (None, "unknown_placeholder".to_string()),
            };
            out.push((FaceGroup {
                face_id,
                name,
                conf: face.score,
                bbox: [
                    face.bbox[0],
                    face.bbox[1],
                    face.bbox[0] + face.bbox[2],
                    face.bbox[1] + face.bbox[3],
                ],
                embedding,
            }, aligned));
        }
        Ok(out)
    }

    pub fn extract_feature_for_db(&mut self, img_path: &str) -> Result<Vec<Vec<f32>>> {
        let frame = imread(img_path, IMREAD_COLOR)?;
        if frame.empty() { return Ok(vec![]); }
        let kept = self.detect_faces_raw(&frame)?;
        let mut features = Vec::new();
        for face in kept {
            let mut aligned = Mat::default();
            self.recognizer.align_crop(&frame, &face.row, &mut aligned)?;
            let mut feature = Mat::default();
            self.recognizer.feature(&aligned, &mut feature)?;
            features.push(mat_to_vec_f32(&feature)?);
        }
        Ok(features)
    }
}

pub fn calc_iou(b1: &[f32; 4], b2: &[f32; 4]) -> f32 {
    let inter_x = b1[0].max(b2[0]);
    let inter_y = b1[1].max(b2[1]);
    let inter_w = (b1[0] + b1[2]).min(b2[0] + b2[2]) - inter_x;
    let inter_h = (b1[1] + b1[3]).min(b2[1] + b2[3]) - inter_y;

    if inter_w <= 0.0 || inter_h <= 0.0 { return 0.0; }
    let inter_area = inter_w * inter_h;
    let union_area = b1[2] * b1[3] + b2[2] * b2[3] - inter_area;
    inter_area / union_area
}

pub fn mat_to_vec_f32(m: &Mat) -> Result<Vec<f32>> {
    let mut v = Vec::with_capacity(m.cols() as usize);
    for j in 0..m.cols() {
        v.push(*m.at_2d::<f32>(0, j)?);
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
    Ok(v.into_iter().map(|x| x / norm).collect())
}
