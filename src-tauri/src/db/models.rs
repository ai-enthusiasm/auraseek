use mongodb::bson::{oid::ObjectId, DateTime as BsonDateTime};
use serde::{Deserialize, Serialize};

// ─── Bbox ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Bbox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

// ─── Media ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectEntry {
    pub class_name: String,
    pub conf:       f32,
    pub bbox:       Bbox,
    pub mask_area:  u32,
    pub mask_path:  Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceEntry {
    pub face_id: String,
    pub name:    Option<String>,
    pub conf:    f32,
    pub bbox:    Bbox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path:   String,
    pub name:   String,
    pub size:   u64,
    pub sha256: String,
    pub phash:  Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub width:       Option<u32>,
    pub height:      Option<u32>,
    pub duration:    Option<f64>,
    pub fps:         Option<f64>,
    pub created_at:  Option<BsonDateTime>,
    pub modified_at: Option<BsonDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id:       Option<ObjectId>,
    #[serde(rename = "type")]
    pub media_type: String,
    pub source:   String,
    pub file:     FileInfo,
    pub metadata: MediaMetadata,
    #[serde(default)]
    pub objects:  Vec<ObjectEntry>,
    #[serde(default)]
    pub faces:    Vec<FaceEntry>,
    pub processed: bool,
}

// ─── Person ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id:         Option<ObjectId>,
    pub face_id:    String,
    pub bbox:       Bbox,
    pub media_id:   ObjectId,
    pub name:       Option<String>,
    pub created_at: BsonDateTime,
    /// thumbnail crop (base64 PNG)
    pub thumbnail:  Option<String>,
}

// ─── SearchHistory ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilters {
    pub object: Option<String>,
    pub face:   Option<String>,
    pub month:  Option<u32>,
    pub year:   Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHistoryDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id:                Option<ObjectId>,
    pub query:             Option<String>,
    pub image_search_path: Option<String>,
    pub filters:           Option<SearchFilters>,
    pub created_at:        BsonDateTime,
}

// ─── VectorEmbedding ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameInfo {
    pub timestamp:   Option<f64>,
    pub frame_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorEmbeddingDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id:         Option<ObjectId>,
    pub media_id:   ObjectId,
    pub source:     String,   // "image" | "video_frame"
    pub frame:      FrameInfo,
    pub embedding:  Vec<f32>,
    pub created_at: BsonDateTime,
}

// ─── API types (returned to frontend) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub media_id:        String,
    pub similarity_score: f32,
    pub file_path:       String,
    pub media_type:      String,
    pub metadata:        SearchResultMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultMeta {
    pub width:      Option<u32>,
    pub height:     Option<u32>,
    pub created_at: Option<String>,
    pub objects:    Vec<String>,
    pub faces:      Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineGroup {
    pub label:    String,
    pub year:     i32,
    pub month:    u32,
    pub day:      Option<u32>,
    pub items:    Vec<TimelineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineItem {
    pub media_id:   String,
    pub file_path:  String,
    pub media_type: String,
    pub width:      Option<u32>,
    pub height:     Option<u32>,
    pub created_at: Option<String>,
    pub objects:    Vec<String>,
    pub faces:      Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonGroup {
    pub face_id:       String,
    pub name:          Option<String>,
    pub photo_count:   u32,
    pub cover_path:    Option<String>,
    pub thumbnail:     Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub sha256:  String,
    pub items:   Vec<DuplicateItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateItem {
    pub media_id:  String,
    pub file_path: String,
    pub size:      u64,
}
