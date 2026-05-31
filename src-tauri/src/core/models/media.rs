use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BboxInfo {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedObject {
    pub class_name: String,
    pub conf:       f32,
    pub bbox:       BboxInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedFace {
    pub face_id: String,
    pub name:    Option<String>,
    pub conf:    f32,
    pub bbox:    BboxInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub media_id:          String,
    pub similarity_score:  f32,
    pub file_path:         String,
    pub media_type:        String,
    pub metadata:          SearchResultMeta,
    pub detected_objects:  Vec<DetectedObject>,
    pub detected_faces:    Vec<DetectedFace>,
    pub width:             Option<u32>,
    pub height:            Option<u32>,
    pub thumbnail_path:    Option<String>,
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
    pub label:  String,
    pub year:   i32,
    pub month:  u32,
    pub day:    Option<u32>,
    pub items:  Vec<TimelineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineItem {
    pub media_id:          String,
    pub file_path:         String,
    pub media_type:        String,
    pub width:             Option<u32>,
    pub height:            Option<u32>,
    pub created_at:        Option<String>,
    pub objects:           Vec<String>,
    pub faces:             Vec<String>,
    pub face_ids:          Vec<String>,
    pub favorite:          bool,
    pub deleted_at:        Option<String>,
    pub is_hidden:         bool,
    pub detected_objects:  Vec<DetectedObject>,
    pub detected_faces:    Vec<DetectedFace>,
    pub thumbnail_path:    Option<String>,
}

/// Lightweight item for paginated grid view — no objects/faces detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePageItem {
    pub media_id:       String,
    pub file_path:      String,
    pub media_type:     String,
    pub width:          Option<u32>,
    pub height:         Option<u32>,
    pub created_at:     Option<String>,
    pub favorite:       bool,
    pub thumbnail_path: Option<String>,
}

/// Paginated timeline response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelinePageResponse {
    pub items:  Vec<TimelinePageItem>,
    pub total:  usize,
    pub offset: usize,
    pub limit:  usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub group_id: String,
    pub reason:   String,
    pub items:    Vec<DuplicateItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateItem {
    pub media_id:       String,
    pub file_path:      String,
    pub size:           u64,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAlbum {
    pub id:        String,
    pub title:     String,
    pub count:     u32,
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSummary {
    pub total_found: usize,
    pub newly_added: usize,
    pub skipped_dup: usize,
    pub errors:      usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestProgress {
    pub processed:    usize,
    pub total:        usize,
    pub current_file: String,
}
