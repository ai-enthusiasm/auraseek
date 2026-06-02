/// yolo model for object detection and segmentation
use anyhow::Result;
use std::collections::HashMap;
use ort::value::Value;
use ort::session::Session;
use crate::infrastructure::ai::runtime::build_session;
use crate::log_info;

pub struct YoloRawResult {
    pub det: Vec<f32>,
    pub n_det: usize,
    pub det_dim: usize,
    pub class_names: Vec<String>,
}

impl YoloRawResult {
    pub fn raw_detections(&self, conf_thresh: f32) -> Vec<YoloDet> {
        (0..self.n_det)
            .filter_map(|i| {
                let base = i * self.det_dim;
                let conf = self.det[base + 4];
                if conf < conf_thresh { return None; }
                let class_id   = self.det[base + 5] as usize;
                let class_name = self.class_names
                    .get(class_id)
                    .cloned()
                    .unwrap_or_else(|| format!("cls_{}", class_id));
                Some(YoloDet {
                    x1: self.det[base],
                    y1: self.det[base + 1],
                    x2: self.det[base + 2],
                    y2: self.det[base + 3],
                    conf,
                    class_id,
                    class_name,
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct YoloDet {
    pub x1:          f32,
    pub y1:          f32,
    pub x2:          f32,
    pub y2:          f32,
    pub conf:        f32,
    #[allow(dead_code)]
    pub class_id:    usize,
    pub class_name:  String,
}

pub struct YoloModel {
    session:     Session,
    class_names: Vec<String>,
}

impl YoloModel {
    pub fn new(path: &str, num_threads: usize) -> Result<Self> {
        let class_names = Self::load_class_names(path);
        log_info!("yolo: {} classes loaded", class_names.len());
        Ok(Self {
            session: build_session(path, num_threads)?,
            class_names,
        })
    }

    pub fn detect(
        &mut self,
        blob:   Vec<f32>,
    ) -> Result<YoloRawResult> {
        let img_tensor = Value::from_array((
            vec![1usize, 3, 640, 640],
            blob.into_boxed_slice(),
        ))?;
        let outputs = self.session.run(ort::inputs!["pixel_values" => img_tensor])?;

        let logits_val = &outputs["logits"];
        let pred_boxes_val = &outputs["pred_boxes"];
        let (logits_shape, logits_data) = logits_val.try_extract_tensor::<f32>()?;
        let (_pred_boxes_shape, pred_boxes_data) = pred_boxes_val.try_extract_tensor::<f32>()?;

        let n_det = logits_shape[1] as usize;
        let n_class = logits_shape[2] as usize;

        let mut det = Vec::with_capacity(n_det * 6);
        for i in 0..n_det {
            let cx = pred_boxes_data[i * 4];
            let cy = pred_boxes_data[i * 4 + 1];
            let w  = pred_boxes_data[i * 4 + 2];
            let h  = pred_boxes_data[i * 4 + 3];

            // Convert normalized [cx, cy, w, h] to absolute coordinates [x1, y1, x2, y2] relative to 640x640 space
            let x1 = (cx - w / 2.0) * 640.0;
            let y1 = (cy - h / 2.0) * 640.0;
            let x2 = (cx + w / 2.0) * 640.0;
            let y2 = (cy + h / 2.0) * 640.0;

            let mut max_score = 0.0f32;
            let mut max_class_id = 0;
            for class_id in 0..n_class {
                let logit = logits_data[i * n_class + class_id];
                let score = 1.0 / (1.0 + (-logit).exp());
                if score > max_score {
                    max_score = score;
                    max_class_id = class_id;
                }
            }

            det.push(x1);
            det.push(y1);
            det.push(x2);
            det.push(y2);
            det.push(max_score);
            det.push(max_class_id as f32);
        }

        Ok(YoloRawResult {
            det,
            n_det,
            det_dim: 6,
            class_names: self.class_names.clone(),
        })
    }

    fn load_class_names(path: &str) -> Vec<String> {
        let fallback = || {
            vec![
                "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat", "traffic light",
                "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat", "dog", "horse", "sheep", "cow",
                "elephant", "bear", "zebra", "giraffe", "backpack", "umbrella", "handbag", "tie", "suitcase", "frisbee",
                "skis", "snowboard", "sports ball", "kite", "baseball bat", "baseball glove", "skateboard", "surfboard", "tennis racket", "bottle",
                "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple", "sandwich", "orange",
                "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair", "couch", "potted plant", "bed",
                "dining table", "toilet", "tv", "laptop", "mouse", "remote", "keyboard", "cell phone", "microwave", "oven",
                "toaster", "sink", "refrigerator", "book", "clock", "vase", "scissors", "teddy bear", "hair drier", "toothbrush"
            ].into_iter().map(String::from).collect::<Vec<_>>()
        };
        let Ok(bytes) = std::fs::read(path) else { return fallback(); };
        let text = String::from_utf8_lossy(&bytes);

        let Some(start) = text.find("{0: '") else { return fallback(); };
        let slice = &text[start..];
        let Some(end) = slice.find('}') else { return fallback(); };
        let dict_str = &slice[..=end];

        let mut map: HashMap<usize, String> = HashMap::new();
        let mut remaining = dict_str;
        while let Some(colon_pos) = remaining.find(": '") {
            let before = remaining[..colon_pos].trim_start_matches(['{', ',', ' ']);
            if let Ok(idx) = before.trim().parse::<usize>() {
                let after = &remaining[colon_pos + 3..];
                if let Some(close) = after.find('\'') {
                    map.insert(idx, after[..close].to_string());
                    remaining = &after[close + 1..];
                    continue;
                }
            }
            break;
        }

        if map.is_empty() { return fallback(); }
        let max_id = *map.keys().max().unwrap();
        (0..=max_id)
            .map(|i| map.get(&i).cloned().unwrap_or_else(|| format!("cls_{}", i)))
            .collect()
    }
}

