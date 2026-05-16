use std::path::PathBuf;
use std::sync::OnceLock;

static GLOBAL_CONFIG: OnceLock<AppConfig> = OnceLock::new();

pub const MODEL_VISION_REL: &str = "models/vision_aura.onnx";
pub const MODEL_TEXT_REL: &str = "models/text_aura.onnx";
pub const MODEL_YOLO_REL: &str = "models/yolo26s-seg.onnx";
pub const MODEL_YUNET_REL: &str = "models/face_detection_yunet_2022mar.onnx";
pub const MODEL_SFACE_REL: &str = "models/face_recognition_sface_2021dec.onnx";
pub const TOKENIZER_VOCAB_REL: &str = "tokenizer/vocab.txt";
pub const TOKENIZER_BPE_REL: &str = "tokenizer/bpe.codes";
pub const FONT_DEJAVU_REL: &str = "fonts/DejaVuSans.ttf";

pub const MODEL_VISION_NAME: &str = "vision_aura.onnx";
pub const MODEL_TEXT_NAME: &str = "text_aura.onnx";
pub const MODEL_YOLO_NAME: &str = "yolo26s-seg.onnx";
pub const MODEL_YUNET_NAME: &str = "face_detection_yunet_2022mar.onnx";
pub const MODEL_SFACE_NAME: &str = "face_recognition_sface_2021dec.onnx";
pub const TOKENIZER_VOCAB_NAME: &str = "vocab.txt";
pub const TOKENIZER_BPE_NAME: &str = "bpe.codes";
pub const FONT_DEJAVU_NAME: &str = "DejaVuSans.ttf";

pub const AI_ASSETS_BASE_URL: &str = "https://github.com/ai-enthusiasm/auraseek/releases/download/v1.0.0";

pub const AI_ASSETS: &[(&str, &str)] = &[
    (MODEL_TEXT_NAME, MODEL_TEXT_REL),
    (MODEL_VISION_NAME, MODEL_VISION_REL),
    (MODEL_SFACE_NAME, MODEL_SFACE_REL),
    (MODEL_YUNET_NAME, MODEL_YUNET_REL),
    (MODEL_YOLO_NAME, MODEL_YOLO_REL),
    (TOKENIZER_BPE_NAME, TOKENIZER_BPE_REL),
    (TOKENIZER_VOCAB_NAME, TOKENIZER_VOCAB_REL),
    (FONT_DEJAVU_NAME, FONT_DEJAVU_REL),
];

#[derive(Debug, Clone)]
pub enum DevicePreference {
    Cpu,
    Cuda,
    Auto,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub log_path: PathBuf,
    pub model_dir: PathBuf,

    pub sqlite_path: PathBuf,
    pub qdrant_port: u16,
    pub qdrant_http_port: u16,
    pub qdrant_dashboard_enabled: bool,
    pub qdrant_storage_dir: PathBuf,
    pub qdrant_collection: String,

    pub face_detection_threshold: f32,
    pub face_identity_threshold: f32,
    /// YuNet post-NMS IoU overlap threshold (OpenCV-style suppression).
    pub face_nms_iou_threshold: f32,
    /// Max face candidates kept (by score) before NMS; must be ≥ 1.
    pub face_top_k: usize,
    pub yolo_confidence: f32,
    pub yolo_iou: f32,
    pub search_threshold: f32,
    pub search_limit: usize,
    pub max_batch_size: usize,

    /// `ffmpeg` scene filter: `select='gt(scene, THRESHOLD)'` (0.0–1.0).
    pub video_scene_threshold: f64,

    /// Minimum similarity for “near duplicate”: Qdrant `score_threshold` in duplicate finder,
    /// and cosine threshold when skipping redundant consecutive `video_frame` embeddings during ingest.
    pub duplicate_score_threshold: f32,
    /// Page size for Qdrant scroll when loading embeddings for duplicate detection.
    pub duplicate_scroll_page_size: usize,

    /// BPE text-query token cap (must match model max positions where applicable).
    pub text_query_max_len: usize,

    pub search_sql_limit_object_filter: usize,
    pub search_sql_limit_face_filter: usize,
    pub search_sql_limit_filter_only: usize,

    pub fs_watcher_debounce_ms: u64,
    /// Minimum free RAM % (`available_ram_percent`) required before watcher ingests a batch.
    pub fs_watcher_min_ram_percent: f32,

    pub device: DevicePreference,
    pub num_threads: usize,

    pub debug: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let data_dir = crate::platform::paths::fallback_data_dir();
        let log_path = PathBuf::from(crate::platform::paths::default_log_path());

        Self {
            model_dir: data_dir.clone(),
            sqlite_path: data_dir.join("auraseek.sqlite3"),
            qdrant_port: 6354,
            qdrant_http_port: 6353,
            qdrant_dashboard_enabled: false,
            qdrant_storage_dir: data_dir.join("qdrant_storage"),
            qdrant_collection: "media_embeddings".to_string(),
            data_dir,
            log_path,

            face_detection_threshold: 0.912,
            face_identity_threshold: 0.351,
            face_nms_iou_threshold: 0.8,
            face_top_k: 5000,
            yolo_confidence: 0.5,
            yolo_iou: 0.9,
            search_threshold: 0.386,
            search_limit: 10000,
            max_batch_size: 1,

            video_scene_threshold: 0.11,

            duplicate_score_threshold: 0.96,
            duplicate_scroll_page_size: 256,

            text_query_max_len: 64,

            search_sql_limit_object_filter: 100,
            search_sql_limit_face_filter: 100,
            search_sql_limit_filter_only: 200,

            fs_watcher_debounce_ms: 2000,
            fs_watcher_min_ram_percent: 10.0,

            device: DevicePreference::Auto,
            num_threads: 1,

            debug: false,
        }
    }
}

fn env_or<T: std::str::FromStr>(key: &str, fallback: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var(key).ok().map(PathBuf::from)
}

fn ensure_dir_or_fallback(path: PathBuf, fallback: PathBuf, label: &str) -> PathBuf {
    if std::fs::create_dir_all(&path).is_ok() {
        return path;
    }

    eprintln!(
        "[config] {} is not writable/creatable: {}. Falling back to {}",
        label,
        path.display(),
        fallback.display()
    );
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

fn ensure_file_parent_or_fallback(path: PathBuf, fallback: PathBuf, label: &str) -> PathBuf {
    let parent_ok = path
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .is_ok();
    if parent_ok {
        return path;
    }

    eprintln!(
        "[config] {} parent is not writable/creatable: {}. Falling back to {}",
        label,
        path.display(),
        fallback.display()
    );
    if let Some(parent) = fallback.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    fallback
}

fn clamp_f32(val: f32, min: f32, max: f32, name: &str) -> f32 {
    if val < min || val > max {
        let clamped = val.clamp(min, max);
        eprintln!(
            "[config] {} value {:.4} out of range [{:.2}, {:.2}], clamped to {:.4}",
            name, val, min, max, clamped
        );
        clamped
    } else {
        val
    }
}

fn clamp_f64(val: f64, min: f64, max: f64, name: &str) -> f64 {
    if val < min || val > max {
        let clamped = val.clamp(min, max);
        eprintln!(
            "[config] {} value {:.4} out of range [{:.2}, {:.2}], clamped to {:.4}",
            name, val, min, max, clamped
        );
        clamped
    } else {
        val
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        let defaults = Self::default();

        let data_dir = ensure_dir_or_fallback(
            env_path("AURASEEK_DATA_DIR").unwrap_or_else(|| defaults.data_dir.clone()),
            defaults.data_dir.clone(),
            "AURASEEK_DATA_DIR",
        );
        let log_path = ensure_file_parent_or_fallback(
            env_path("AURASEEK_LOG_PATH").unwrap_or_else(|| defaults.log_path.clone()),
            defaults.log_path.clone(),
            "AURASEEK_LOG_PATH",
        );
        let model_dir = ensure_dir_or_fallback(
            env_path("AURASEEK_MODEL_DIR").unwrap_or_else(|| data_dir.clone()),
            data_dir.clone(),
            "AURASEEK_MODEL_DIR",
        );

        let sqlite_path = env_path("AURASEEK_SQLITE_PATH")
            .unwrap_or_else(|| data_dir.join("auraseek.sqlite3"));
        let qdrant_port = env_or("AURASEEK_QDRANT_PORT", defaults.qdrant_port);
        let qdrant_http_port = env_or("AURASEEK_QDRANT_HTTP_PORT", defaults.qdrant_http_port);
        let qdrant_dashboard_enabled = env_or(
            "AURASEEK_QDRANT_DASHBOARD_ENABLED",
            defaults.qdrant_dashboard_enabled,
        );
        let qdrant_storage_dir = ensure_dir_or_fallback(
            env_path("AURASEEK_QDRANT_STORAGE_DIR")
                .unwrap_or_else(|| data_dir.join("qdrant_storage")),
            data_dir.join("qdrant_storage"),
            "AURASEEK_QDRANT_STORAGE_DIR",
        );
        let qdrant_collection = std::env::var("AURASEEK_QDRANT_COLLECTION")
            .unwrap_or_else(|_| defaults.qdrant_collection.clone());

        let face_detection_threshold = clamp_f32(
            env_or("AURASEEK_FACE_DETECTION_THRESHOLD", defaults.face_detection_threshold),
            0.0, 1.0, "AURASEEK_FACE_DETECTION_THRESHOLD",
        );
        // Legacy alias: AURASEEK_FACE_THRESHOLD (docs / older .env) if IDENTITY not set.
        let face_identity_threshold = {
            let primary = std::env::var("AURASEEK_FACE_IDENTITY_THRESHOLD")
                .ok()
                .and_then(|v| v.parse::<f32>().ok());
            let legacy = std::env::var("AURASEEK_FACE_THRESHOLD")
                .ok()
                .and_then(|v| v.parse::<f32>().ok());
            let raw = primary.or(legacy).unwrap_or(defaults.face_identity_threshold);
            clamp_f32(raw, 0.0, 1.0, "AURASEEK_FACE_IDENTITY_THRESHOLD")
        };
        let face_nms_iou_threshold = clamp_f32(
            env_or("AURASEEK_FACE_NMS_IOU_THRESHOLD", defaults.face_nms_iou_threshold),
            0.0, 1.0, "AURASEEK_FACE_NMS_IOU_THRESHOLD",
        );
        let face_top_k = env_or("AURASEEK_FACE_TOP_K", defaults.face_top_k).max(1);
        let yolo_confidence = clamp_f32(
            env_or("AURASEEK_YOLO_CONFIDENCE", defaults.yolo_confidence),
            0.0, 1.0, "AURASEEK_YOLO_CONFIDENCE",
        );
        let yolo_iou = clamp_f32(
            env_or("AURASEEK_YOLO_IOU", defaults.yolo_iou),
            0.0, 1.0, "AURASEEK_YOLO_IOU",
        );
        let max_batch_size = env_or("AURASEEK_MAX_BATCH_SIZE", defaults.max_batch_size)
            .max(1);

        let device = match std::env::var("AURASEEK_DEVICE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "cpu" => DevicePreference::Cpu,
            "cuda" => DevicePreference::Cuda,
            _ => DevicePreference::Auto,
        };

        let num_threads = env_or("AURASEEK_NUM_THREADS", defaults.num_threads)
            .max(1);

        let search_threshold = clamp_f32(
            env_or("AURASEEK_SEARCH_THRESHOLD", defaults.search_threshold),
            0.0, 1.0, "AURASEEK_SEARCH_THRESHOLD",
        );
        let search_limit = env_or("AURASEEK_SEARCH_LIMIT", defaults.search_limit)
            .max(1);

        let video_scene_threshold = clamp_f64(
            env_or("AURASEEK_VIDEO_SCENE_THRESHOLD", defaults.video_scene_threshold),
            0.0,
            1.0,
            "AURASEEK_VIDEO_SCENE_THRESHOLD",
        );

        let duplicate_score_threshold = {
            let primary = std::env::var("AURASEEK_DUPLICATE_SCORE_THRESHOLD")
                .ok()
                .and_then(|v| v.parse::<f32>().ok());
            // Legacy: same semantics were split into a separate video-only env.
            let legacy_video = std::env::var("AURASEEK_VIDEO_FRAME_DEDUP_COSINE_THRESHOLD")
                .ok()
                .and_then(|v| v.parse::<f32>().ok());
            let raw = primary
                .or(legacy_video)
                .unwrap_or(defaults.duplicate_score_threshold);
            clamp_f32(raw, 0.0, 1.0, "AURASEEK_DUPLICATE_SCORE_THRESHOLD")
        };
        let duplicate_scroll_page_size =
            env_or("AURASEEK_DUPLICATE_SCROLL_PAGE_SIZE", defaults.duplicate_scroll_page_size).max(1);

        let text_query_max_len = env_or("AURASEEK_TEXT_QUERY_MAX_LEN", defaults.text_query_max_len).max(8);

        let search_sql_limit_object_filter = env_or(
            "AURASEEK_SEARCH_SQL_LIMIT_OBJECT_FILTER",
            defaults.search_sql_limit_object_filter,
        )
        .max(1);
        let search_sql_limit_face_filter = env_or(
            "AURASEEK_SEARCH_SQL_LIMIT_FACE_FILTER",
            defaults.search_sql_limit_face_filter,
        )
        .max(1);
        let search_sql_limit_filter_only = env_or(
            "AURASEEK_SEARCH_SQL_LIMIT_FILTER_ONLY",
            defaults.search_sql_limit_filter_only,
        )
        .max(1);

        let fs_watcher_debounce_ms = env_or("AURASEEK_FS_WATCHER_DEBOUNCE_MS", defaults.fs_watcher_debounce_ms).max(1);
        let fs_watcher_min_ram_percent = clamp_f32(
            env_or("AURASEEK_FS_WATCHER_MIN_RAM_PERCENT", defaults.fs_watcher_min_ram_percent),
            0.0,
            100.0,
            "AURASEEK_FS_WATCHER_MIN_RAM_PERCENT",
        );

        let debug = env_or("AURASEEK_DEBUG", defaults.debug);

        Self {
            data_dir,
            log_path,
            model_dir,
            sqlite_path,
            qdrant_port,
            qdrant_http_port,
            qdrant_dashboard_enabled,
            qdrant_storage_dir,
            qdrant_collection,
            face_detection_threshold,
            face_identity_threshold,
            face_nms_iou_threshold,
            face_top_k,
            yolo_confidence,
            yolo_iou,
            search_threshold,
            search_limit,
            max_batch_size,
            video_scene_threshold,
            duplicate_score_threshold,
            duplicate_scroll_page_size,
            text_query_max_len,
            search_sql_limit_object_filter,
            search_sql_limit_face_filter,
            search_sql_limit_filter_only,
            fs_watcher_debounce_ms,
            fs_watcher_min_ram_percent,
            device,
            num_threads,
            debug,
        }
    }

    pub fn global() -> &'static AppConfig {
        GLOBAL_CONFIG.get_or_init(|| Self::from_env())
    }

    pub fn init(config: AppConfig) {
        let _ = GLOBAL_CONFIG.set(config);
    }

    pub fn log_summary(&self) {
        crate::log_info!("⚙️  Configuration Summary:");
        crate::log_info!("   Data Dir:      {}", self.data_dir.display());
        crate::log_info!("   Model Dir:     {}", self.model_dir.display());
        crate::log_info!("   SQLite:        {}", self.sqlite_path.display());
        crate::log_info!("   Qdrant gRPC:   {}", self.qdrant_port);
        crate::log_info!("   Qdrant HTTP:   {}", self.qdrant_http_port);
        crate::log_info!("   Qdrant UI:     {}", if self.qdrant_dashboard_enabled { "enabled" } else { "disabled" });
        crate::log_info!("   Qdrant store:  {}", self.qdrant_storage_dir.display());
        crate::log_info!("   Device:        {:?}", self.device);
        crate::log_info!("   Threads:       {}", self.num_threads);
        crate::log_info!(
            "   Face (Det/Id/NMS/topK): {:.4} / {:.4} / {:.4} / {}",
            self.face_detection_threshold,
            self.face_identity_threshold,
            self.face_nms_iou_threshold,
            self.face_top_k
        );
        crate::log_info!("   Search:        {:.4} (limit: {})", self.search_threshold, self.search_limit);
        crate::log_info!("   YOLO (Conf/IOU): {:.4} / {:.4}", self.yolo_confidence, self.yolo_iou);
        crate::log_info!("   Max Batch:     {}", self.max_batch_size);
        crate::log_info!(
            "   Video scene: {:.4} | near-dup similarity {:.4} (video dedup + Qdrant) | scroll {}",
            self.video_scene_threshold,
            self.duplicate_score_threshold,
            self.duplicate_scroll_page_size
        );
        crate::log_info!("   Text query max len: {}", self.text_query_max_len);
        crate::log_info!(
            "   SQL limits (obj/face/filter): {}/{}/{}",
            self.search_sql_limit_object_filter,
            self.search_sql_limit_face_filter,
            self.search_sql_limit_filter_only
        );
        crate::log_info!(
            "   FS watcher:    debounce {} ms | min RAM {:.1}%",
            self.fs_watcher_debounce_ms,
            self.fs_watcher_min_ram_percent
        );
        crate::log_info!("   Debug Mode:    {}", self.debug);
    }
}
