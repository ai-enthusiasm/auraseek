/// nms, mask reconstruction, and rle encoding for yolo
use super::detector::{YoloDet, YoloRawResult};
use super::preprocess::LetterboxResult;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectionRecord {
    pub class_name: String,
    pub conf:       f32,
    pub bbox:       [f32; 4],
}

pub struct YoloProcessor;

impl YoloProcessor {
    pub fn postprocess(
        raw:        &YoloRawResult,
        lb:         &LetterboxResult,
        conf_thresh: f32,
        iou_thresh:  f32,
    ) -> Vec<DetectionRecord> {
        let dets = raw.raw_detections(conf_thresh);
        if dets.is_empty() { return vec![]; }

        let kept = Self::nms(&dets, iou_thresh);
        let (orig_h, orig_w) = lb.orig_size;

        kept.into_iter().map(|d| {
            let unpad = |v: f32, pad: u32, clip: u32| -> f32 {
                ((v - pad as f32) / lb.ratio).clamp(0.0, clip as f32)
            };
            let x1 = unpad(d.x1, lb.pad_left, orig_w);
            let y1 = unpad(d.y1, lb.pad_top,  orig_h);
            let x2 = unpad(d.x2, lb.pad_left, orig_w);
            let y2 = unpad(d.y2, lb.pad_top,  orig_h);

            DetectionRecord {
                class_name: d.class_name.clone(),
                conf:       d.conf,
                bbox:       [x1, y1, x2, y2],
            }
        }).collect()
    }

    pub fn apply_dual_threshold(
        dets: Vec<DetectionRecord>,
        score_high: f32,
        score_min: f32,
    ) -> Vec<DetectionRecord> {
        use std::collections::HashSet;
        let mut qualified_classes = HashSet::new();
        for d in &dets {
            if d.conf >= score_high {
                qualified_classes.insert(d.class_name.clone());
            }
        }
        dets.into_iter()
            .filter(|d| {
                d.conf >= score_high || (qualified_classes.contains(&d.class_name) && d.conf >= score_min)
            })
            .collect()
    }

    fn nms(dets: &[YoloDet], iou_thresh: f32) -> Vec<YoloDet> {
        let mut order: Vec<usize> = (0..dets.len()).collect();
        order.sort_by(|&a, &b| dets[b].conf.partial_cmp(&dets[a].conf).unwrap());

        let area = |d: &YoloDet| (d.x2 - d.x1).max(0.0) * (d.y2 - d.y1).max(0.0);
        let iou  = |a: &YoloDet, b: &YoloDet| -> f32 {
            let ix1   = a.x1.max(b.x1);
            let iy1   = a.y1.max(b.y1);
            let ix2   = a.x2.min(b.x2);
            let iy2   = a.y2.min(b.y2);
            let inter = (ix2 - ix1).max(0.0) * (iy2 - iy1).max(0.0);
            let union = area(a) + area(b) - inter;
            if union <= 0.0 { 0.0 } else { inter / union }
        };

        let mut suppressed = vec![false; dets.len()];
        let mut result     = Vec::new();

        for &i in &order {
            if suppressed[i] { continue; }
            result.push(dets[i].clone());
            for &j in &order {
                if !suppressed[j] && i != j && iou(&dets[i], &dets[j]) > iou_thresh {
                    suppressed[j] = true;
                }
            }
        }
        result
    }
}
