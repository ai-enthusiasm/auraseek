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
               AND meta_modified_at IS ?3
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
        img_width: Option<u32>,
        img_height: Option<u32>,
    ) -> Result<()> {
        let mut conn = db.conn();
        let tx = conn.transaction()?;

        tx.execute("DELETE FROM media_objects WHERE media_id = ?1", params![media_id])?;
        tx.execute("DELETE FROM media_faces WHERE media_id = ?1", params![media_id])?;

        for obj in &objects {
            // First, look up or insert class_name into object_class table
            let class_id: i64 = match tx.query_row(
                "SELECT id FROM object_class WHERE name = ?1",
                params![obj.class_name],
                |r| r.get(0),
            ) {
                Ok(id) => id,
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    tx.execute(
                        "INSERT INTO object_class (name) VALUES (?1)",
                        params![obj.class_name],
                    )?;
                    tx.last_insert_rowid()
                }
                Err(e) => return Err(e.into()),
            };

            let mask_blob: Option<Vec<u8>> = if crate::core::config::AppConfig::global().enable_mask_rle {
                obj.mask_rle.as_ref().map(|pairs| {
                    let mut bytes = Vec::with_capacity(pairs.len() * 8);
                    for pair in pairs {
                        bytes.extend_from_slice(&pair[0].to_le_bytes());
                        bytes.extend_from_slice(&pair[1].to_le_bytes());
                    }
                    bytes
                })
            } else {
                None
            };

            tx.execute(
                "INSERT INTO media_objects (media_id, class_id, conf, bbox_x, bbox_y, bbox_w, bbox_h, mask_area, mask_path, mask_rle)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    media_id, class_id, obj.conf,
                    obj.bbox.x, obj.bbox.y, obj.bbox.w, obj.bbox.h,
                    obj.mask_area, obj.mask_path, mask_blob,
                ],
            )?;
        }

        for face in &faces {
            // Lookup person_id from person table using face_id
            let person_id: i64 = match tx.query_row(
                "SELECT id FROM person WHERE face_id = ?1",
                params![face.face_id],
                |r| r.get(0),
            ) {
                Ok(id) => id,
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    tx.execute(
                        "INSERT INTO person (face_id, name, conf, face_bbox_x, face_bbox_y, face_bbox_w, face_bbox_h)
                         VALUES (?1,?2,?3,?4,?5,?6,?7)",
                        params![
                            face.face_id, face.name, face.conf,
                            face.bbox.x, face.bbox.y, face.bbox.w, face.bbox.h,
                        ],
                    )?;
                    tx.last_insert_rowid()
                }
                Err(e) => return Err(e.into()),
            };

            tx.execute(
                "INSERT INTO media_faces (media_id, person_id, name, conf, bbox_x, bbox_y, bbox_w, bbox_h)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    media_id, person_id, face.name, face.conf,
                    face.bbox.x, face.bbox.y, face.bbox.w, face.bbox.h,
                ],
            )?;
        }

        // Build SET clause: always update processed; optionally update thumbnail and dimensions
        match (thumbnail.as_ref(), img_width, img_height) {
            (Some(thumb), Some(w), Some(h)) => {
                tx.execute(
                    "UPDATE media SET processed = 1, thumbnail = ?2, meta_width = ?3, meta_height = ?4 WHERE id = ?1",
                    params![media_id, thumb, w, h],
                )?;
            }
            (Some(thumb), _, _) => {
                tx.execute(
                    "UPDATE media SET processed = 1, thumbnail = ?2 WHERE id = ?1",
                    params![media_id, thumb],
                )?;
            }
            (None, Some(w), Some(h)) => {
                tx.execute(
                    "UPDATE media SET processed = 1, meta_width = ?2, meta_height = ?3 WHERE id = ?1",
                    params![media_id, w, h],
                )?;
            }
            (None, _, _) => {
                tx.execute(
                    "UPDATE media SET processed = 1 WHERE id = ?1",
                    params![media_id],
                )?;
            }
        }

        tx.commit()?;
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

    /// Lightweight paginated timeline query — skips JOINing objects/faces tables.
    /// Returns only the fields needed for grid display + total count.
    pub fn get_timeline_page(
        db: &SqliteDb,
        offset: usize,
        limit: usize,
        source_dir: &str,
    ) -> Result<(Vec<crate::core::models::TimelinePageItem>, usize)> {
        let conn = db.conn();
        let base = source_dir.trim_end_matches('/');

        let total: usize = conn.query_row(
            "SELECT COUNT(*) FROM media WHERE deleted_at IS NULL AND is_hidden = 0 AND processed = 1",
            [],
            |r| r.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, media_type, file_name, meta_width, meta_height,
                    meta_created_at, favorite, thumbnail
             FROM media
             WHERE deleted_at IS NULL AND is_hidden = 0 AND processed = 1
             ORDER BY meta_created_at DESC
             LIMIT ?1 OFFSET ?2"
        )?;

        let items: Vec<crate::core::models::TimelinePageItem> = stmt
            .query_map(params![limit as i64, offset as i64], |r| {
                let file_name: String = r.get(2)?;
                let thumb: Option<String> = r.get(7)?;
                let resolved_thumb = thumb.map(|t| super::resolve_thumbnail_path(&t));
                let file_path = std::path::Path::new(base).join(&file_name).to_string_lossy().to_string();
                Ok(crate::core::models::TimelinePageItem {
                    media_id:       r.get(0)?,
                    file_path,
                    media_type:     r.get(1)?,
                    width:          r.get(3)?,
                    height:         r.get(4)?,
                    created_at:     r.get(5)?,
                    favorite:       r.get::<_, i32>(6)? != 0,
                    thumbnail_path: resolved_thumb,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok((items, total))
    }


    pub fn get_distinct_objects(db: &SqliteDb) -> Result<Vec<String>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT oc.name FROM media_objects mo
             JOIN object_class oc ON mo.class_id = oc.id
             WHERE mo.media_id IN (SELECT id FROM media WHERE deleted_at IS NULL AND is_hidden = 0)
             ORDER BY oc.name"
        )?;
        let names: Vec<String> = stmt.query_map([], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(names)
    }
}
