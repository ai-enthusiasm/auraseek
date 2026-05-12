use anyhow::Result;
use rusqlite::params;
use crate::infrastructure::database::SqliteDb;
use crate::core::models::TimelineGroup;
use super::{DbOperations, read_media_rows_from_query};

impl DbOperations {
    pub fn move_to_trash(db: &SqliteDb, source_dir: &str, media_id: &str) -> Result<()> {
        let conn = db.conn();
        use rusqlite::OptionalExtension;

        let name: Option<String> = conn.query_row(
            "SELECT file_name FROM media WHERE id = ?1",
            params![media_id],
            |r| r.get(0),
        ).optional()?;

        if let Some(ref name) = name {
            let src_path = std::path::Path::new(source_dir).join(name);
            let trash_dir = std::path::Path::new(source_dir).join(".trash");
            if !trash_dir.exists() { let _ = std::fs::create_dir_all(&trash_dir); }
            let dst_path = trash_dir.join(name);
            if src_path.exists() { let _ = std::fs::rename(&src_path, &dst_path); }
        }

        conn.execute(
            "UPDATE media SET deleted_at = datetime('now') WHERE id = ?1",
            params![media_id],
        )?;
        Ok(())
    }

    pub fn restore_from_trash(db: &SqliteDb, source_dir: &str, media_id: &str) -> Result<()> {
        let conn = db.conn();
        use rusqlite::OptionalExtension;

        let name: Option<String> = conn.query_row(
            "SELECT file_name FROM media WHERE id = ?1",
            params![media_id],
            |r| r.get(0),
        ).optional()?;

        if let Some(ref name) = name {
            let trash_path = std::path::Path::new(source_dir).join(".trash").join(name);
            let dst_path = std::path::Path::new(source_dir).join(name);
            if trash_path.exists() { let _ = std::fs::rename(&trash_path, &dst_path); }
        }

        conn.execute(
            "UPDATE media SET deleted_at = NULL WHERE id = ?1",
            params![media_id],
        )?;
        Ok(())
    }

    pub fn get_trash(db: &SqliteDb, source_dir: &str) -> Result<Vec<TimelineGroup>> {
        let conn = db.conn();
        let rows = read_media_rows_from_query(
            &conn,
            "SELECT * FROM media WHERE deleted_at IS NOT NULL ORDER BY deleted_at ASC",
            &[],
        )?;
        Self::group_rows_into_timeline(rows, source_dir)
    }

    pub fn empty_trash(db: &SqliteDb, source_dir: &str) -> Result<()> {
        let conn = db.conn();

        let mut stmt = conn.prepare(
            "SELECT file_name FROM media WHERE deleted_at IS NOT NULL"
        )?;
        let names: Vec<String> = stmt.query_map([], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for name in names {
            let path = std::path::Path::new(source_dir).join(".trash").join(&name);
            if path.exists() { let _ = std::fs::remove_file(&path); }
        }

        conn.execute("DELETE FROM media WHERE deleted_at IS NOT NULL", [])?;
        Ok(())
    }

    pub fn auto_purge_trash(db: &SqliteDb, source_dir: &str) -> Result<()> {
        let conn = db.conn();

        let mut stmt = conn.prepare(
            "SELECT file_name FROM media WHERE deleted_at IS NOT NULL AND deleted_at < datetime('now', '-30 days')"
        )?;
        let names: Vec<String> = stmt.query_map([], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        for name in names {
            let path = std::path::Path::new(source_dir).join(".trash").join(&name);
            if path.exists() { let _ = std::fs::remove_file(&path); }
        }

        conn.execute(
            "DELETE FROM media WHERE deleted_at IS NOT NULL AND deleted_at < datetime('now', '-30 days')",
            [],
        )?;
        Ok(())
    }

    pub fn hard_delete_trash_item(db: &SqliteDb, source_dir: &str, media_id: &str) -> Result<()> {
        let conn = db.conn();
        use rusqlite::OptionalExtension;

        let name: Option<String> = conn.query_row(
            "SELECT file_name FROM media WHERE id = ?1 AND deleted_at IS NOT NULL",
            params![media_id],
            |r| r.get(0),
        ).optional()?;

        if let Some(ref name) = name {
            let path = std::path::Path::new(source_dir).join(".trash").join(name);
            if path.exists() { let _ = std::fs::remove_file(&path); }
        }

        conn.execute("DELETE FROM media WHERE id = ?1", params![media_id])?;
        Ok(())
    }

    pub fn hide_photo(db: &SqliteDb, media_id: &str) -> Result<()> {
        let conn = db.conn();
        conn.execute(
            "UPDATE media SET is_hidden = 1 WHERE id = ?1",
            params![media_id],
        )?;
        Ok(())
    }

    pub fn unhide_photo(db: &SqliteDb, media_id: &str) -> Result<()> {
        let conn = db.conn();
        conn.execute(
            "UPDATE media SET is_hidden = 0 WHERE id = ?1",
            params![media_id],
        )?;
        Ok(())
    }

    pub fn get_hidden_photos(db: &SqliteDb, source_dir: &str) -> Result<Vec<TimelineGroup>> {
        let conn = db.conn();
        let rows = read_media_rows_from_query(
            &conn,
            "SELECT * FROM media WHERE is_hidden = 1 AND deleted_at IS NULL ORDER BY meta_created_at DESC",
            &[],
        )?;
        Self::group_rows_into_timeline(rows, source_dir)
    }
}
