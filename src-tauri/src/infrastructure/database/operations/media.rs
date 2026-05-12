use anyhow::Result;
use rusqlite::params;
use crate::infrastructure::database::SqliteDb;
use crate::infrastructure::database::models::{
    FileInfo, MediaMetadata, ObjectEntry, FaceEntry,
};
use crate::core::models::TimelineGroup;
use super::{DbOperations, read_media_rows_from_query};

impl DbOperations {
    pub fn check_file_by_metadata(
        db: &SqliteDb,
        name: &str,
        size: u64,
        modified_at: Option<&str>,
    ) -> Result<Option<(String, bool)>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, processed FROM media
             WHERE file_name = ?1
               AND file_size = ?2
               AND (
                    (meta_modified_at IS NULL AND ?3 IS NULL)
                    OR meta_modified_at = ?3
               )
             LIMIT 1"
        )?;
        use rusqlite::OptionalExtension;
        let result = stmt.query_row(params![name, size as i64, modified_at], |r| {
            let id: String = r.get(0)?;
            let processed: bool = r.get::<_, i32>(1)? != 0;
            Ok((id, processed))
        }).optional()?;

        Ok(result)
    }

    pub fn check_exact_file(db: &SqliteDb, name: &str, sha256: &str) -> Result<Option<(String, bool)>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, processed FROM media
             WHERE file_name = ?1 AND file_sha256 = ?2
             LIMIT 1"
        )?;
        use rusqlite::OptionalExtension;
        let result = stmt.query_row(params![name, sha256], |r| {
            let id: String = r.get(0)?;
            let processed: bool = r.get::<_, i32>(1)? != 0;
            Ok((id, processed))
        }).optional()?;

        Ok(result)
    }

    pub fn find_media_by_name(db: &SqliteDb, name: &str) -> Result<Option<String>> {
        let conn = db.conn();
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT id FROM media WHERE file_name = ?1 LIMIT 1",
            params![name],
            |r| r.get(0),
        ).optional().map_err(Into::into)
    }

    pub fn insert_media(
        db: &SqliteDb,
        id: &str,
        media_type: &str,
        file: &FileInfo,
        metadata: &MediaMetadata,
    ) -> Result<String> {
        let conn = db.conn();
        conn.execute(
            "INSERT INTO media (id, media_type, file_name, file_size, file_sha256, file_phash,
                meta_width, meta_height, meta_duration, meta_fps, meta_created_at, meta_modified_at,
                processed, favorite, is_hidden, thumbnail)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12, 0, 0, 0, NULL)",
            params![
                id, media_type, file.name, file.size as i64, file.sha256, file.phash,
                metadata.width, metadata.height, metadata.duration, metadata.fps,
                metadata.created_at, metadata.modified_at,
            ],
        )?;
        Ok(id.to_string())
    }

    pub fn reset_media_file(
        db: &SqliteDb,
        media_id: &str,
        media_type: &str,
        file: &FileInfo,
        metadata: &MediaMetadata,
    ) -> Result<()> {
        let conn = db.conn();
        conn.execute("DELETE FROM media_objects WHERE media_id = ?1", params![media_id])?;
        conn.execute("DELETE FROM media_faces WHERE media_id = ?1", params![media_id])?;
        conn.execute(
            "UPDATE media
             SET media_type = ?2,
                 file_name = ?3,
                 file_size = ?4,
                 file_sha256 = ?5,
                 file_phash = ?6,
                 meta_width = ?7,
                 meta_height = ?8,
                 meta_duration = ?9,
                 meta_fps = ?10,
                 meta_created_at = ?11,
                 meta_modified_at = ?12,
                 processed = 0,
                 thumbnail = NULL
             WHERE id = ?1",
            params![
                media_id, media_type, file.name, file.size as i64, file.sha256, file.phash,
                metadata.width, metadata.height, metadata.duration, metadata.fps,
                metadata.created_at, metadata.modified_at,
            ],
        )?;
        Ok(())
    }

    pub fn update_media_ai(
        db: &SqliteDb,
        media_id: &str,
        objects: Vec<ObjectEntry>,
        faces: Vec<FaceEntry>,
        thumbnail: Option<String>,
    ) -> Result<()> {
        let conn = db.conn();

        conn.execute("DELETE FROM media_objects WHERE media_id = ?1", params![media_id])?;
        conn.execute("DELETE FROM media_faces WHERE media_id = ?1", params![media_id])?;

        for obj in &objects {
            let mask_rle_json: Option<String> = obj.mask_rle.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
            conn.execute(
                "INSERT INTO media_objects (media_id, class_name, conf, bbox_x, bbox_y, bbox_w, bbox_h, mask_area, mask_path, mask_rle)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    media_id, obj.class_name, obj.conf,
                    obj.bbox.x, obj.bbox.y, obj.bbox.w, obj.bbox.h,
                    obj.mask_area, obj.mask_path, mask_rle_json,
                ],
            )?;
        }

        for face in &faces {
            conn.execute(
                "INSERT INTO media_faces (media_id, face_id, name, conf, bbox_x, bbox_y, bbox_w, bbox_h)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    media_id, face.face_id, face.name, face.conf,
                    face.bbox.x, face.bbox.y, face.bbox.w, face.bbox.h,
                ],
            )?;
        }

        if let Some(ref thumb) = thumbnail {
            conn.execute(
                "UPDATE media SET processed = 1, thumbnail = ?2 WHERE id = ?1",
                params![media_id, thumb],
            )?;
        } else {
            conn.execute(
                "UPDATE media SET processed = 1 WHERE id = ?1",
                params![media_id],
            )?;
        }

        Ok(())
    }

    pub fn set_media_processed(db: &SqliteDb, media_id: &str, processed: bool) -> Result<()> {
        let conn = db.conn();
        conn.execute(
            "UPDATE media SET processed = ?2 WHERE id = ?1",
            params![media_id, if processed { 1 } else { 0 }],
        )?;
        Ok(())
    }

    pub fn toggle_favorite(db: &SqliteDb, media_id: &str) -> Result<bool> {
        let conn = db.conn();
        conn.execute(
            "UPDATE media SET favorite = 1 - favorite WHERE id = ?1",
            params![media_id],
        )?;
        let fav: bool = conn.query_row(
            "SELECT favorite FROM media WHERE id = ?1",
            params![media_id],
            |r| Ok(r.get::<_, i32>(0)? != 0),
        )?;
        Ok(fav)
    }

    pub fn prune_missing_media(db: &SqliteDb, source_dir: &str) -> Result<usize> {
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT id, file_name FROM media")?;
        let rows: Vec<(String, String)> = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?.filter_map(|r| r.ok()).collect();

        let base = std::path::Path::new(source_dir);
        let mut count = 0;
        for (id, name) in rows {
            if !base.join(&name).exists() {
                crate::log_info!("🗑️ Pruning missing file: {}", name);
                conn.execute("DELETE FROM media WHERE id = ?1", params![id])?;
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn clear_database(db: &SqliteDb) -> Result<()> {
        let conn = db.conn();
        conn.execute_batch(
            "DELETE FROM album_media;
             DELETE FROM custom_album;
             DELETE FROM search_history;
             DELETE FROM media_faces;
             DELETE FROM media_objects;
             DELETE FROM media;
             DELETE FROM person;
             DELETE FROM config_auraseek;"
        )?;
        Ok(())
    }

    pub fn get_timeline(db: &SqliteDb, limit: usize, source_dir: &str) -> Result<Vec<TimelineGroup>> {
        let conn = db.conn();
        let rows = read_media_rows_from_query(
            &conn,
            "SELECT * FROM media WHERE deleted_at IS NULL AND is_hidden = 0 AND processed = 1
             ORDER BY meta_created_at DESC LIMIT ?1",
            &[&(limit as i64)],
        )?;
        Self::group_rows_into_timeline(rows, source_dir)
    }

    pub fn get_distinct_objects(db: &SqliteDb) -> Result<Vec<String>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT class_name FROM media_objects
             WHERE media_id IN (SELECT id FROM media WHERE deleted_at IS NULL AND is_hidden = 0)
             ORDER BY class_name"
        )?;
        let names: Vec<String> = stmt.query_map([], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(names)
    }
}
