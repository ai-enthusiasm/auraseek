use serde::{Deserialize, Serialize};

// ────────────────────── Core document types ──────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name:   String,
    pub size:   u64,
    pub sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phash:  Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub width:       Option<u32>,
    pub height:      Option<u32>,
    pub duration:    Option<f64>,
    pub fps:         Option<f64>,
    pub created_at:  Option<String>,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bbox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectEntry {
    pub class_name: String,
    pub conf:       f32,
    pub bbox:       Bbox,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask_area:  Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask_path:  Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask_rle:   Option<Vec<[u32; 2]>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceEntry {
    pub face_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name:    Option<String>,
    pub conf:    f32,
    pub bbox:    Bbox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRow {
    pub id:         String,
    pub media_type: String,
    pub file:       FileInfo,
    pub metadata:   MediaMetadata,
    pub objects:    Vec<ObjectEntry>,
    pub faces:      Vec<FaceEntry>,
    pub processed:  bool,
    #[serde(default)]
    pub favorite:   bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail:  Option<String>,
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub is_hidden:  bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonDoc {
    pub face_id:   String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name:      Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conf:      Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_bbox: Option<Bbox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilters {
    pub object:     Option<String>,
    pub face:       Option<String>,
    pub month:      Option<u32>,
    pub year:       Option<i32>,
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHistoryRow {
    pub id:         i64,
    pub query:      Option<String>,
    pub image_path: Option<String>,
    pub created_at: Option<String>,
}

#[allow(unused_imports)]
pub use crate::core::models::{
    BboxInfo, DetectedObject, DetectedFace, SearchResult, SearchResultMeta,
    TimelineGroup, TimelineItem, PersonGroup, DuplicateGroup, DuplicateItem, CustomAlbum,
};
