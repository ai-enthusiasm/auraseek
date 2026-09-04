use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

pub struct SqliteDb {
    conn: std::sync::Mutex<Connection>,
}

impl SqliteDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                crate::log_warn!("⚠️ Failed to create SQLite parent dir {}: {}", parent.display(), e);
            }
        }

        let open_and_init = |db_path: &Path| -> Result<Connection> {
            let conn = Connection::open(db_path)
                .with_context(|| format!("Failed to open SQLite database at {}", db_path.display()))?;

            if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=10000;") {
                crate::log_warn!("⚠️ WAL journal mode failed for {}: {}. Falling back to DELETE mode.", db_path.display(), e);
                conn.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=10000;")
                    .context("Failed to set SQLite PRAGMAs")?;
            }

            Ok(conn)
        };

        let conn = match open_and_init(path).and_then(|conn| {
            let db = Self { conn: std::sync::Mutex::new(conn) };
            db.ensure_schema()?;
            db.auto_migrate_columns()?;
            Ok(db.conn.into_inner().unwrap())
        }) {
            Ok(conn) => conn,
            Err(first_err) => {
                crate::log_warn!(
                    "⚠️ SQLite open/schema failed at {}: {:#}. Attempting automatic database recovery...",
                    path.display(),
                    first_err
                );

                if path.exists() {
                    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                    let corrupt_backup = path.with_extension(format!("sqlite3.corrupt.{}", timestamp));
                    crate::log_warn!("📦 Backing up unreadable/corrupt database file to {}", corrupt_backup.display());
                    let _ = std::fs::rename(path, &corrupt_backup);
                    let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
                    let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
                }

                let fresh_conn = open_and_init(path)?;
                let db = Self { conn: std::sync::Mutex::new(fresh_conn) };
                db.ensure_schema()?;
                db.auto_migrate_columns()?;
                db.conn.into_inner().unwrap()
            }
        };

        crate::log_info!("✅ SQLite database opened and ready: {}", path.display());
        let db = Self { conn: std::sync::Mutex::new(conn) };
        Ok(db)
    }

    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn auto_migrate_columns(&self) -> Result<()> {
        let conn = self.conn();
        let migrations = [
            ("media", "file_phash", "TEXT"),
            ("media", "frame_ts", "REAL"),
            ("media", "frame_idx", "INTEGER"),
            ("media", "meta_duration", "REAL"),
            ("media", "meta_fps", "REAL"),
            ("media", "meta_width", "INTEGER"),
            ("media", "meta_height", "INTEGER"),
            ("media", "processed", "INTEGER DEFAULT 0"),
            ("media", "favorite", "INTEGER DEFAULT 0"),
            ("media", "is_hidden", "INTEGER DEFAULT 0"),
            ("media", "deleted_at", "TEXT"),
            ("media", "thumbnail", "TEXT"),
            ("media_objects", "thumbnail", "TEXT"),
            ("person", "thumbnail", "TEXT"),
            ("person", "conf", "REAL"),
            ("person", "face_bbox_x", "REAL"),
            ("person", "face_bbox_y", "REAL"),
            ("person", "face_bbox_w", "REAL"),
            ("person", "face_bbox_h", "REAL"),
            ("media_faces", "name", "TEXT"),
            ("media_faces", "conf", "REAL"),
            ("media_faces", "bbox_x", "REAL"),
            ("media_faces", "bbox_y", "REAL"),
            ("media_faces", "bbox_w", "REAL"),
            ("media_faces", "bbox_h", "REAL"),
            ("search_history", "filter_object", "TEXT"),
            ("search_history", "filter_face", "TEXT"),
            ("search_history", "filter_month", "INTEGER"),
            ("search_history", "filter_year", "INTEGER"),
            ("search_history", "filter_media_type", "TEXT"),
            ("search_history", "deleted_at", "TEXT"),
        ];

        for (table, column, col_type) in migrations {
            let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, col_type);
            let _ = conn.execute(&sql, []);
        }

        Ok(())
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
            CREATE INDEX IF NOT EXISTS idx_media_name_sha256 ON media(file_name, file_sha256);
            CREATE INDEX IF NOT EXISTS idx_media_name_size_modified ON media(file_name, file_size, meta_modified_at);
            CREATE INDEX IF NOT EXISTS idx_media_processed ON media(processed);
            CREATE INDEX IF NOT EXISTS idx_media_processed_check
                ON media(file_name, file_size, meta_modified_at, processed, id);

            CREATE TABLE IF NOT EXISTS object_class (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL
            );

            CREATE TABLE IF NOT EXISTS media_objects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                media_id TEXT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
                class_id INTEGER NOT NULL REFERENCES object_class(id) ON DELETE CASCADE,
                conf REAL NOT NULL,
                bbox_x REAL, bbox_y REAL, bbox_w REAL, bbox_h REAL,
                thumbnail TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_obj_media ON media_objects(media_id);
            CREATE INDEX IF NOT EXISTS idx_obj_class ON media_objects(class_id);

            CREATE TABLE IF NOT EXISTS person (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                face_id TEXT UNIQUE NOT NULL,
                name TEXT,
                thumbnail TEXT,
                conf REAL,
                face_bbox_x REAL, face_bbox_y REAL, face_bbox_w REAL, face_bbox_h REAL,
                created_at TEXT DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS media_faces (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                media_id TEXT NOT NULL REFERENCES media(id) ON DELETE CASCADE,
                person_id INTEGER REFERENCES person(id) ON DELETE CASCADE,
                name TEXT,
                conf REAL NOT NULL,
                bbox_x REAL, bbox_y REAL, bbox_w REAL, bbox_h REAL
            );
            CREATE INDEX IF NOT EXISTS idx_face_media ON media_faces(media_id);
            CREATE INDEX IF NOT EXISTS idx_face_person_id ON media_faces(person_id);

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
