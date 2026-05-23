use anyhow::{anyhow, Result};
use ort::session::Session;
use ort::value::Value;
use opencv::{
    core::{Mat, Size, Vec3b, Point2f, Vector},
    imgcodecs::{imread, IMREAD_COLOR},
    prelude::*,
};
use super::db::FaceDb;
use crate::core::config::AppConfig;
use crate::infrastructure::ai::runtime::build_session;
use crate::log_info;

#[derive(Debug, Clone, serde::Serialize)]
pub struct FaceGroup {
    pub face_id:  String,
    pub name:     Option<String>,
    pub conf:     f32,
    pub bbox:     [f32; 4],
    #[serde(skip)]
    pub embedding: Vec<f32>,
}

struct FaceDetection {
    pub bbox:  [f32; 4], // [x, y, w, h]
    pub score: f32,
    pub landmarks: [[f32; 2]; 5],
}

pub struct FaceModel {
    scrfd_path: String,
    arcface_path: String,
    num_threads: usize,
    scrfd_session: Session,
    arcface_session: Session,
    score_threshold: f32,
    nms_iou_threshold: f32,
    top_k: usize,
}

impl FaceModel {
    pub fn new(scrfd_path: &str, arcface_path: &str, num_threads: usize) -> Result<Self> {
        let cfg = AppConfig::global();
        log_info!(
            "face model: scrfd={:<45} | arcface={:<45} | threads={}",
            scrfd_path,
            arcface_path,
            num_threads
        );

        let scrfd_session = build_session(scrfd_path, num_threads)?;
        let arcface_session = build_session(arcface_path, num_threads)?;

        Ok(Self {
            scrfd_path: scrfd_path.to_string(),
            arcface_path: arcface_path.to_string(),
            num_threads,
            scrfd_session,
            arcface_session,
            score_threshold: cfg.face_detection_threshold,
            nms_iou_threshold: cfg.face_nms_iou_threshold,
            top_k: cfg.face_top_k,
        })
    }

    pub fn set_score_threshold(&mut self, threshold: f32) {
        self.score_threshold = threshold;
    }

    fn rebuild_models(&mut self) -> Result<()> {
        self.scrfd_session = build_session(&self.scrfd_path, self.num_threads)?;
        self.arcface_session = build_session(&self.arcface_path, self.num_threads)?;
        Ok(())
    }

    fn run_scrfd_with_retry(&mut self, blob: Vec<f32>) -> Result<Vec<Vec<f32>>> {
        let run_once = |session: &mut Session, input: Vec<f32>| -> Result<Vec<Vec<f32>>> {
            let input_tensor = Value::from_array((
                vec![1usize, 3, 640, 640],
                input.into_boxed_slice(),
            ))?;
            let outputs = session.run(ort::inputs!["input.1" => input_tensor])?;
            let mut out = Vec::with_capacity(outputs.len());
            for i in 0..outputs.len() {
                let (_, data) = outputs[i].try_extract_tensor::<f32>()?;
                out.push(data.to_vec());
            }
            Ok(out)
        };

        match run_once(&mut self.scrfd_session, blob.clone()) {
            Ok(v) => Ok(v),
            Err(first_err) => {
                log_info!("scrfd detector recover: rebuilding session after error: {}", first_err);
                self.rebuild_models()?;
                run_once(&mut self.scrfd_session, blob)
            }
        }
    }

    fn run_arcface_with_retry(&mut self, blob: Vec<f32>) -> Result<Vec<f32>> {
        let run_once = |session: &mut Session, input: Vec<f32>| -> Result<Vec<f32>> {
            let input_tensor = Value::from_array((
                vec![1usize, 3, 112, 112],
                input.into_boxed_slice(),
            ))?;
            let outputs = session.run(ort::inputs!["input.1" => input_tensor])?;
            let (_, data) = outputs[0].try_extract_tensor::<f32>()?;
            Ok(data.to_vec())
        };

        match run_once(&mut self.arcface_session, blob.clone()) {
            Ok(v) => Ok(v),
            Err(first_err) => {
                log_info!("arcface embedder recover: rebuilding session after error: {}", first_err);
                self.rebuild_models()?;
                run_once(&mut self.arcface_session, blob)
            }
        }
    }

    fn detect_faces_raw(&mut self, frame: &Mat) -> Result<Vec<FaceDetection>> {
        if frame.empty() { return Ok(vec![]); }
        let size = frame.size()?;
        let (w_img, h_img) = (size.width as f32, size.height as f32);
        if w_img < 20.0 || h_img < 20.0 { return Ok(vec![]); }

        let scale_x = w_img / 640.0;
        let scale_y = h_img / 640.0;

        let mut resized = Mat::default();
        opencv::imgproc::resize(
            frame,
            &mut resized,
            Size::new(640, 640),
            0.0,
            0.0,
            opencv::imgproc::INTER_LINEAR,
        )?;

        let area = 640 * 640;
        let mut blob = vec![0f32; 3 * area];
        for y in 0..640 {
            for x in 0..640 {
                let px: Vec3b = *resized.at_2d::<Vec3b>(y as i32, x as i32)?;
                let idx = y * 640 + x;
                // BGR -> RGB and normalization
                blob[idx] = (px[2] as f32 - 127.5) / 128.0;          // R
                blob[idx + area] = (px[1] as f32 - 127.5) / 128.0;   // G
                blob[idx + 2 * area] = (px[0] as f32 - 127.5) / 128.0; // B
            }
        }

        let outputs = self.run_scrfd_with_retry(blob)?;
        if outputs.len() != 9 {
            return Err(anyhow!("SCRFD model outputs size mismatch: expected 9, got {}", outputs.len()));
        }

        let strides = [8usize, 16, 32];
        let fmc = 3;
        let num_anchors = 2;
        let mut candidate_faces = Vec::new();

        for (idx, &stride) in strides.iter().enumerate() {
            let fh = 640 / stride;
            let fw = 640 / stride;
            let expected_len = fh * fw * num_anchors;

            let scores_out = &outputs[idx];
            let bbox_preds_out = &outputs[idx + fmc];
            let kps_preds_out = &outputs[idx + 2 * fmc];

            if scores_out.len() != expected_len {
                return Err(anyhow!(
                    "SCRFD output score size mismatch for stride {}: expected {}, got {}",
                    stride,
                    expected_len,
                    scores_out.len()
                ));
            }

            for anchor_idx in 0..expected_len {
                let score = scores_out[anchor_idx];
                if score < self.score_threshold {
                    continue;
                }

                // Generate anchor center
                let pos_idx = anchor_idx / num_anchors;
                let grid_y = pos_idx / fw;
                let grid_x = pos_idx % fw;
                let cx = (grid_x as f32) * (stride as f32);
                let cy = (grid_y as f32) * (stride as f32);

                // Decode bounding box
                let bbox_base = anchor_idx * 4;
                let dl = bbox_preds_out[bbox_base] * (stride as f32);
                let dt = bbox_preds_out[bbox_base + 1] * (stride as f32);
                let dr = bbox_preds_out[bbox_base + 2] * (stride as f32);
                let db = bbox_preds_out[bbox_base + 3] * (stride as f32);

                let x1 = cx - dl;
                let y1 = cy - dt;
                let x2 = cx + dr;
                let y2 = cy + db;

                // Scale bounding box back to original coordinates
                let x1_orig = x1 * scale_x;
                let y1_orig = y1 * scale_y;
                let x2_orig = x2 * scale_x;
                let y2_orig = y2 * scale_y;
                let w_orig = x2_orig - x1_orig;
                let h_orig = y2_orig - y1_orig;

                // Decode landmarks
                let kps_base = anchor_idx * 10;
                let mut landmarks = [[0.0f32; 2]; 5];
                for n in 0..5 {
                    let kps_dx = kps_preds_out[kps_base + 2 * n] * (stride as f32);
                    let kps_dy = kps_preds_out[kps_base + 2 * n + 1] * (stride as f32);
                    landmarks[n][0] = (cx + kps_dx) * scale_x;
                    landmarks[n][1] = (cy + kps_dy) * scale_y;
                }

                candidate_faces.push(FaceDetection {
                    bbox: [x1_orig, y1_orig, w_orig, h_orig],
                    score,
                    landmarks,
                });
            }
        }

        // Apply NMS
        candidate_faces.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        if candidate_faces.len() > self.top_k {
            candidate_faces.truncate(self.top_k);
        }

        let mut kept: Vec<FaceDetection> = Vec::new();
        for face in candidate_faces {
            let mut overlap = false;
            for k in &kept {
                if calc_iou(&face.bbox, &k.bbox) > self.nms_iou_threshold {
                    overlap = true;
                    break;
                }
            }
            if !overlap {
                kept.push(face);
            }
        }

        // Quality/profile filtering
        let mut final_faces = Vec::new();
        for face in kept {
            // Filter 1: Check eye overlap/closeness (< 5px Euclidean distance)
            let eye_dx = face.landmarks[0][0] - face.landmarks[1][0];
            let eye_dy = face.landmarks[0][1] - face.landmarks[1][1];
            let eye_dist = (eye_dx * eye_dx + eye_dy * eye_dy).sqrt();
            if eye_dist < 5.0 {
                continue;
            }

            // Filter 2: Check if landmarks are outside frame bounds
            let mut out_of_bounds = false;
            for kp in face.landmarks.iter() {
                if kp[0] < 0.0 || kp[0] >= w_img || kp[1] < 0.0 || kp[1] >= h_img {
                    out_of_bounds = true;
                    break;
                }
            }
            if out_of_bounds {
                continue;
            }

            final_faces.push(face);
        }

        Ok(final_faces)
    }

    fn align_face_affine(&self, frame: &Mat, kps: &[[f32; 2]; 5]) -> Result<Mat> {
        let ref_kps = [
            [38.2946f32, 51.6963f32],  // left eye
            [73.5318f32, 51.5014f32],  // right eye
            [56.0252f32, 71.7366f32],  // nose
            [41.5493f32, 92.3655f32],  // left mouth corner
            [70.7299f32, 92.2041f32],  // right mouth corner
        ];

        let mut from_pts = Vector::<Point2f>::new();
        for kp in kps.iter() {
            from_pts.push(Point2f::new(kp[0], kp[1]));
        }

        let mut to_pts = Vector::<Point2f>::new();
        for ref_kp in ref_kps.iter() {
            to_pts.push(Point2f::new(ref_kp[0], ref_kp[1]));
        }

        let mut inliers = Mat::default();
        let mut m = opencv::calib3d::estimate_affine_partial_2d(
            &from_pts,
            &to_pts,
            &mut inliers,
            opencv::calib3d::LMEDS,
            3.0,
            2000,
            0.99,
            10,
        )?;

        if m.empty() {
            let mut src_3 = Vector::<Point2f>::new();
            src_3.push(from_pts.get(0)?);
            src_3.push(from_pts.get(1)?);
            src_3.push(from_pts.get(2)?);

            let mut dst_3 = Vector::<Point2f>::new();
            dst_3.push(to_pts.get(0)?);
            dst_3.push(to_pts.get(1)?);
            dst_3.push(to_pts.get(2)?);

            m = opencv::imgproc::get_affine_transform(&src_3, &dst_3)?;
        }

        let mut aligned = Mat::default();
        opencv::imgproc::warp_affine(
            frame,
            &mut aligned,
            &m,
            Size::new(112, 112),
            opencv::imgproc::INTER_LINEAR,
            opencv::core::BORDER_CONSTANT,
            opencv::core::Scalar::all(0.0),
        )?;

        Ok(aligned)
    }

    fn embed_aligned_face(&mut self, aligned: &Mat) -> Result<Vec<f32>> {
        let area = 112 * 112;
        let mut blob = vec![0f32; 3 * area];
        for y in 0..112 {
            for x in 0..112 {
                let px: Vec3b = *aligned.at_2d::<Vec3b>(y as i32, x as i32)?;
                let idx = y * 112 + x;
                // BGR -> RGB and normalization
                blob[idx] = (px[2] as f32 - 127.5) / 128.0;          // R
                blob[idx + area] = (px[1] as f32 - 127.5) / 128.0;   // G
                blob[idx + 2 * area] = (px[0] as f32 - 127.5) / 128.0; // B
            }
        }

        let raw_embedding = self.run_arcface_with_retry(blob)?;

        // L2 normalize
        let norm = raw_embedding.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-8);
        let normalized: Vec<f32> = raw_embedding.into_iter().map(|x| x / norm).collect();

        Ok(normalized)
    }

    pub fn detect_from_mat(
        &mut self,
        frame: &Mat,
        db: &FaceDb,
        identity_threshold: f32,
    ) -> Result<Vec<FaceGroup>> {
        let kept = self.detect_faces_raw(frame)?;

        let mut groups = Vec::new();
        for face in kept.into_iter() {
            let aligned = self.align_face_affine(frame, &face.landmarks)?;
            let embedding = self.embed_aligned_face(&aligned)?;

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
                    face.bbox[1] + face.bbox[3],
                ],
                embedding,
            });
        }
        Ok(groups)
    }

    pub fn detect_from_path(
        &mut self,
        path: &str,
        db: &FaceDb,
        identity_threshold: f32,
    ) -> Result<Vec<FaceGroup>> {
        let frame = imread(path, IMREAD_COLOR)?;
        self.detect_from_mat(&frame, db, identity_threshold)
    }

    pub fn detect_from_mat_with_aligned(
        &mut self,
        frame: &Mat,
        db: &FaceDb,
        identity_threshold: f32,
    ) -> Result<Vec<(FaceGroup, Mat)>> {
        let kept = self.detect_faces_raw(frame)?;
        let mut out = Vec::new();
        for face in kept {
            let aligned = self.align_face_affine(frame, &face.landmarks)?;
            let embedding = self.embed_aligned_face(&aligned)?;
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
            let aligned = self.align_face_affine(&frame, &face.landmarks)?;
            let embedding = self.embed_aligned_face(&aligned)?;
            features.push(embedding);
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
