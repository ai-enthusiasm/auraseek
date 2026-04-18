use std::path::PathBuf;
use std::sync::OnceLock;

static GLOBAL_CONFIG: OnceLock<AppConfig> = OnceLock::new();

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
    pub surreal_bin: Option<PathBuf>,

    pub face_detection_threshold: f32,
    pub face_identity_threshold: f32,
    pub yolo_confidence: f32,
    pub yolo_iou: f32,
    pub search_threshold: f32,
    pub search_limit: usize,
    pub max_batch_size: usize,

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
            data_dir,
            log_path,
            surreal_bin: None,

            face_detection_threshold: 0.93,
            face_identity_threshold: 0.33,
            yolo_confidence: 0.25,
            yolo_iou: 0.45,
            search_threshold: 0.256,
            search_limit: 10000,
            max_batch_size: 32,

            device: DevicePreference::Auto,
            num_threads: num_cpus::get().max(1),

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
        let surreal_bin = env_path("AURASEEK_SURREAL_BIN")
            .or(defaults.surreal_bin);

        let face_detection_threshold = clamp_f32(
            env_or("AURASEEK_FACE_DETECTION_THRESHOLD", defaults.face_detection_threshold),
            0.0, 1.0, "AURASEEK_FACE_DETECTION_THRESHOLD",
        );
        let face_identity_threshold = clamp_f32(
            env_or("AURASEEK_FACE_IDENTITY_THRESHOLD", defaults.face_identity_threshold),
            0.0, 1.0, "AURASEEK_FACE_IDENTITY_THRESHOLD",
        );
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

        let debug = env_or("AURASEEK_DEBUG", defaults.debug);

        Self {
            data_dir,
            log_path,
            model_dir,
            surreal_bin,
            face_detection_threshold,
            face_identity_threshold,
            yolo_confidence,
            yolo_iou,
            search_threshold,
            search_limit,
            max_batch_size,
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
        crate::log_info!("   Device:        {:?}", self.device);
        crate::log_info!("   Threads:       {}", self.num_threads);
        crate::log_info!("   Face (Det/Id): {:.4} / {:.4}", self.face_detection_threshold, self.face_identity_threshold);
        crate::log_info!("   Search:        {:.4} (limit: {})", self.search_threshold, self.search_limit);
        crate::log_info!("   YOLO (Conf/IOU): {:.4} / {:.4}", self.yolo_confidence, self.yolo_iou);
        crate::log_info!("   Max Batch:     {}", self.max_batch_size);
        crate::log_info!("   Debug Mode:    {}", self.debug);
    }
}
