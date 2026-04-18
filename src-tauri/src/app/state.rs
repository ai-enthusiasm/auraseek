use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::core::models::SyncStatus;
use crate::infrastructure::ai::AuraSeekEngine;
use crate::infrastructure::database::SurrealDb;
use crate::infrastructure::fs::watcher;

pub struct AppState {
    pub engine:        Arc<Mutex<Option<AuraSeekEngine>>>,
    pub db:            Arc<Mutex<Option<SurrealDb>>>,
    pub surreal_addr:  std::sync::Mutex<String>,
    pub surreal_user:  std::sync::Mutex<String>,
    pub surreal_pass:  std::sync::Mutex<String>,
    pub source_dir:    Mutex<String>,
    pub sync_status:   Arc<Mutex<SyncStatus>>,
    pub surreal_child: std::sync::Mutex<Option<std::process::Child>>,
    pub data_dir:      std::sync::Mutex<std::path::PathBuf>,
    pub watcher_handle: std::sync::Mutex<Option<watcher::FsWatcherHandle>>,
    pub stream_port:   std::sync::atomic::AtomicU16,
    pub abort_sync:    Arc<std::sync::atomic::AtomicBool>,
    /// Tăng mỗi lần reset thư viện để hủy ingest/scan cũ và không ghi lại `config_auraseek`.
    pub library_reset_epoch: Arc<AtomicU64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            engine:        Arc::new(Mutex::new(None)),
            db:            Arc::new(Mutex::new(None)),
            surreal_addr:  std::sync::Mutex::new(String::new()),
            surreal_user:  std::sync::Mutex::new("root".to_string()),
            surreal_pass:  std::sync::Mutex::new("root".to_string()),
            source_dir:    Mutex::new(String::new()),
            sync_status:   Arc::new(Mutex::new(SyncStatus::default())),
            surreal_child: std::sync::Mutex::new(None),
            data_dir:      std::sync::Mutex::new(std::path::PathBuf::from(".")),
            watcher_handle: std::sync::Mutex::new(None),
            stream_port:   std::sync::atomic::AtomicU16::new(0),
            abort_sync:    Arc::new(std::sync::atomic::AtomicBool::new(false)),
            library_reset_epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    #[inline]
    pub fn bump_library_reset_epoch(&self) -> u64 {
        self.library_reset_epoch.fetch_add(1, Ordering::SeqCst) + 1
    }
}
