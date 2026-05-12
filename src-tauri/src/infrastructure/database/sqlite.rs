use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub struct SqliteDb {
    conn: std::sync::Mutex<Connection>,
}

impl SqliteDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create SQLite parent dir: {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open SQLite database at {}", path.display()))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")
            .context("Failed to set SQLite PRAGMAs")?;

        crate::log_info!("✅ SQLite opened: {}", path.display());

        let db = Self { conn: std::sync::Mutex::new(conn) };
        db.ensure_schema()?;
        Ok(db)
    }

    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }

    fn ensure_schema(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS media (
                id TEXT PRIMARY KEY,
                media_type TEXT NOT NULL,
                file_name TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                file_sha256 TEXT NOT NULL,
                file_phash TEXT,
                frame_ts REAL,
                frame_idx INTEGER,
                meta_duration REAL,
                meta_fps REAL,
                meta_width INTEGER,
                meta_height INTEGER,
                meta_created_at TEXT,
                meta_modified_at TEXT,
                processed INTEGER DEFAULT 0,
                favorite INTEGER DEFAULT 0,
                is_hidden INTEGER DEFAULT 0,
                deleted_at TEXT,
                thumbnail TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_media_sha256 ON media(file_sha256);
            CREATE INDEX IF NOT EXISTS idx_media_created ON media(meta_created_at);
            CREATE INDEX IF NOT EXISTS idx_media_name ON media(file_name);
            CREATE INDEX IF NOT EXISTS idx_media_name_sha256 ON media(file_name, file_sha256);
            CREATE INDEX IF NOT EXISTS idx_media_name_size_modified ON media(file_name, file_size, meta_modified_at);
            CREATE INDEX IF NOT EXISTS idx_media_processed ON media(processed);

            CREATE TABLE IF NOT EXISTS media_objects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                media_id TEXT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
                class_name TEXT NOT NULL,
                conf REAL NOT NULL,
                bbox_x REAL, bbox_y REAL, bbox_w REAL, bbox_h REAL,
                thumbnail TEXT,
                mask_area INTEGER,
                mask_path TEXT,
                mask_rle TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_obj_media ON media_objects(media_id);
            CREATE INDEX IF NOT EXISTS idx_obj_class ON media_objects(class_name);

            CREATE TABLE IF NOT EXISTS media_faces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                media_id TEXT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
                face_id TEXT NOT NULL,
                name TEXT,
                conf REAL NOT NULL,
                bbox_x REAL, bbox_y REAL, bbox_w REAL, bbox_h REAL
            );
            CREATE INDEX IF NOT EXISTS idx_face_media ON media_faces(media_id);
            CREATE INDEX IF NOT EXISTS idx_face_id ON media_faces(face_id);

            CREATE TABLE IF NOT EXISTS person (
                id TEXT PRIMARY KEY,
                face_id TEXT UNIQUE NOT NULL,
                name TEXT,
                thumbnail TEXT,
                conf REAL,
                face_bbox_x REAL, face_bbox_y REAL, face_bbox_w REAL, face_bbox_h REAL,
                created_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS config_auraseek (
                id TEXT PRIMARY KEY DEFAULT 'main',
                source_dir TEXT NOT NULL,
                updated_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS search_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                query TEXT,
                image_path TEXT,
                filter_object TEXT,
                filter_face TEXT,
                filter_month INTEGER,
                filter_year INTEGER,
                filter_media_type TEXT,
                created_at TEXT DEFAULT (datetime('now')),
                deleted_at TEXT
            );

            CREATE TABLE IF NOT EXISTS custom_album (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS album_media (
                album_id TEXT NOT NULL REFERENCES custom_album(id) ON DELETE CASCADE,
                media_id TEXT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
                PRIMARY KEY (album_id, media_id)
            );
            "
        ).context("Failed to create SQLite schema")?;

        crate::log_info!("📋 SQLite schema ready");
        Ok(())
    }
}
