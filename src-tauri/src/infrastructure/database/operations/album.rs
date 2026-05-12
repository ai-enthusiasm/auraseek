use anyhow::Result;
use rusqlite::params;
use crate::infrastructure::database::SqliteDb;
use crate::core::models::{TimelineGroup, CustomAlbum};
use super::{DbOperations, read_media_rows_from_query};

impl DbOperations {
    pub fn create_album(db: &SqliteDb, title: String) -> Result<String> {
        crate::log_info!("🔨 [DB] Creating album: {}", title);
        let id = uuid::Uuid::new_v4().to_string();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO custom_album (id, title) VALUES (?1, ?2)",
            params![id, title],
        )?;
        crate::log_info!("✅ [DB] Album created successfully: {}", id);
        Ok(id)
    }

    pub fn get_albums(db: &SqliteDb, source_dir: &str) -> Result<Vec<CustomAlbum>> {
        crate::log_info!("🔍 [DB] Fetching all albums...");
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT a.id, a.title,
                    (SELECT COUNT(*) FROM album_media am WHERE am.album_id = a.id) AS cnt,
                    (SELECT m.file_name FROM album_media am2 JOIN media m ON m.id = am2.media_id WHERE am2.album_id = a.id LIMIT 1) AS cover_name,
                    (SELECT m.thumbnail FROM album_media am3 JOIN media m ON m.id = am3.media_id WHERE am3.album_id = a.id LIMIT 1) AS cover_thumb
             FROM custom_album a ORDER BY a.created_at DESC"
        )?;
        let base = source_dir.trim_end_matches('/');
        let albums: Vec<CustomAlbum> = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let title: String = r.get(1)?;
            let count: i64 = r.get(2)?;
            let cover_name: Option<String> = r.get(3)?;
            let cover_thumb: Option<String> = r.get(4)?;
            Ok((id, title, count as u32, cover_name, cover_thumb))
        })?.filter_map(|r| r.ok()).map(|(id, title, count, cover_name, cover_thumb)| {
            let cover_url = if let Some(ref t) = cover_thumb {
                Some(if std::path::Path::new(t).is_absolute() { t.clone() } else { format!("{}/{}", base, t) })
            } else {
                cover_name.as_ref().map(|n| format!("{}/{}", base, n))
            };
            crate::log_info!("  📷 Album '{}' count={} cover={:?}", title, count, cover_url);
            CustomAlbum { id, title, count, cover_url }
        }).collect();

        crate::log_info!("📂 [DB] Found {} manual albums", albums.len());
        Ok(albums)
    }

    pub fn add_to_album(db: &SqliteDb, album_id: &str, media_ids: Vec<String>) -> Result<()> {
        crate::log_info!("➕ [DB] Adding {} files to album: {}", media_ids.len(), album_id);
        let conn = db.conn();
        for mid in &media_ids {
            conn.execute(
                "INSERT OR IGNORE INTO album_media (album_id, media_id) VALUES (?1, ?2)",
                params![album_id, mid],
            )?;
        }
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM album_media WHERE album_id = ?1",
            params![album_id],
            |r| r.get(0),
        )?;
        crate::log_info!("📋 [DB] Verified Album now has {} items", total);
        Ok(())
    }

    pub fn remove_from_album(db: &SqliteDb, album_id: &str, media_ids: Vec<String>) -> Result<()> {
        let conn = db.conn();
        for mid in &media_ids {
            conn.execute(
                "DELETE FROM album_media WHERE album_id = ?1 AND media_id = ?2",
                params![album_id, mid],
            )?;
        }
        Ok(())
    }

    pub fn delete_album(db: &SqliteDb, album_id: &str) -> Result<()> {
        let conn = db.conn();
        conn.execute("DELETE FROM custom_album WHERE id = ?1", params![album_id])?;
        Ok(())
    }

    pub fn get_album_photos(db: &SqliteDb, album_id: &str, source_dir: &str) -> Result<Vec<TimelineGroup>> {
        let conn = db.conn();
        let rows = read_media_rows_from_query(
            &conn,
            "SELECT m.* FROM media m
             JOIN album_media am ON am.media_id = m.id
             WHERE am.album_id = ?1 AND m.deleted_at IS NULL AND m.is_hidden = 0
             ORDER BY m.meta_created_at DESC",
            &[&album_id],
        )?;
        Self::group_rows_into_timeline(rows, source_dir)
    }
}
