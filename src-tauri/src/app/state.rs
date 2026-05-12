use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::core::models::SyncStatus;
use crate::infrastructure::ai::AuraSeekEngine;
use crate::infrastructure::database::SqliteDb;
use crate::infrastructure::fs::watcher;

pub struct AppState {
    pub engine:        Arc<Mutex<Option<AuraSeekEngine>>>,
    pub sqlite:        Arc<std::sync::Mutex<Option<SqliteDb>>>,
    pub qdrant_client: Arc<Mutex<Option<qdrant_client::Qdrant>>>,
    pub qdrant_child:  std::sync::Mutex<Option<std::process::Child>>,
    pub source_dir:    Mutex<String>,
    pub sync_status:   Arc<Mutex<SyncStatus>>,
    pub data_dir:      std::sync::Mutex<std::path::PathBuf>,
    pub watcher_handle: std::sync::Mutex<Option<watcher::FsWatcherHandle>>,
    pub stream_port:   std::sync::atomic::AtomicU16,
    pub qdrant_runtime_grpc_port: std::sync::atomic::AtomicU16,
    pub qdrant_runtime_http_port: std::sync::atomic::AtomicU16,
    pub abort_sync:    Arc<std::sync::atomic::AtomicBool>,
    pub library_reset_epoch: Arc<AtomicU64>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            engine:        Arc::new(Mutex::new(None)),
            sqlite:        Arc::new(std::sync::Mutex::new(None)),
            qdrant_client: Arc::new(Mutex::new(None)),
            qdrant_child:  std::sync::Mutex::new(None),
            source_dir:    Mutex::new(String::new()),
            sync_status:   Arc::new(Mutex::new(SyncStatus::default())),
            data_dir:      std::sync::Mutex::new(std::path::PathBuf::from(".")),
            watcher_handle: std::sync::Mutex::new(None),
            stream_port:   std::sync::atomic::AtomicU16::new(0),
            qdrant_runtime_grpc_port: std::sync::atomic::AtomicU16::new(0),
            qdrant_runtime_http_port: std::sync::atomic::AtomicU16::new(0),
            abort_sync:    Arc::new(std::sync::atomic::AtomicBool::new(false)),
            library_reset_epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    #[inline]
    pub fn bump_library_reset_epoch(&self) -> u64 {
        self.library_reset_epoch.fetch_add(1, Ordering::SeqCst) + 1
    }
}
