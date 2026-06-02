use anyhow::Result;
use rusqlite::params;
use crate::infrastructure::database::SqliteDb;
use crate::infrastructure::database::models::{PersonDoc, Bbox};
use crate::core::models::{PersonGroup, BboxInfo};
use super::DbOperations;

impl DbOperations {
    pub fn upsert_person(db: &SqliteDb, person: PersonDoc) -> Result<()> {
        let conn = db.conn();
        use rusqlite::OptionalExtension;

        let existing: Option<(Option<f32>, Option<String>)> = conn.query_row(
            "SELECT conf, thumbnail FROM person WHERE face_id = ?1",
            params![person.face_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).optional()?;

        match existing {
            Some((old_conf, old_thumb)) => {
                let should_upgrade = match (person.conf, old_conf) {
                    (Some(_), None) => true,
                    (Some(new_c), Some(old_c)) => new_c >= old_c,
                    _ => false,
                };
                if should_upgrade {
                    let (bx, by, bw, bh) = match &person.face_bbox {
                        Some(b) => (Some(b.x), Some(b.y), Some(b.w), Some(b.h)),
                        None => (None, None, None, None),
                    };
                    conn.execute(
                        "UPDATE person SET
                            name = COALESCE(?2, name),
                            conf = ?3,
                            thumbnail = ?4,
                            face_bbox_x = ?5, face_bbox_y = ?6, face_bbox_w = ?7, face_bbox_h = ?8
                         WHERE face_id = ?1",
                        params![person.face_id, person.name, person.conf, person.thumbnail, bx, by, bw, bh],
                    )?;
                } else {
                    let new_is_face = person.thumbnail.as_ref().map(|t| t.starts_with("face_") || t.contains("_face_")).unwrap_or(false);
                    let old_is_face = old_thumb.as_ref().map(|t| t.starts_with("face_") || t.contains("_face_")).unwrap_or(false);
                    if new_is_face && !old_is_face {
                        conn.execute(
                            "UPDATE person SET thumbnail = ?2 WHERE face_id = ?1",
                            params![person.face_id, person.thumbnail],
                        )?;
                    }
                    if person.name.is_some() {
                        conn.execute(
                            "UPDATE person SET name = COALESCE(?2, name) WHERE face_id = ?1",
                            params![person.face_id, person.name],
                        )?;
                    }
                }
            }
            None => {
                let (bx, by, bw, bh) = match &person.face_bbox {
                    Some(b) => (Some(b.x), Some(b.y), Some(b.w), Some(b.h)),
                    None => (None, None, None, None),
                };
                conn.execute(
                    "INSERT INTO person (face_id, name, thumbnail, conf, face_bbox_x, face_bbox_y, face_bbox_w, face_bbox_h)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![person.face_id, person.name, person.thumbnail, person.conf, bx, by, bw, bh],
                )?;
            }
        }
        Ok(())
    }

    pub fn name_person(db: &SqliteDb, face_id: &str, name: &str) -> Result<()> {
        let conn = db.conn();
        conn.execute(
            "UPDATE person SET name = ?2 WHERE face_id = ?1",
            params![face_id, name],
        )?;
        conn.execute(
            "UPDATE media_faces SET name = ?2 WHERE person_id = (SELECT id FROM person WHERE face_id = ?1)",
            params![face_id, name],
        )?;
        Ok(())
    }

    pub fn get_people(db: &SqliteDb, source_dir: &str, cache_dir: &std::path::Path) -> Result<Vec<PersonGroup>> {
        let conn = db.conn();

        #[derive(Default, Clone)]
        struct Agg {
            photo_count: u32,
            cover_name: Option<String>,
            best_conf: Option<f32>,
            best_bbox: Option<Bbox>,
            best_cover_name: Option<String>,
            seen_media_sha: std::collections::HashSet<String>,
        }

        let mut stmt = conn.prepare(
            "SELECT m.file_name, m.file_sha256, p.face_id, mf.conf, mf.bbox_x, mf.bbox_y, mf.bbox_w, mf.bbox_h
             FROM media_faces mf
             JOIN media m ON m.id = mf.media_id
             JOIN person p ON mf.person_id = p.id
             WHERE m.deleted_at IS NULL AND m.is_hidden = 0"
        )?;
        let face_rows: Vec<(String, String, String, f32, f32, f32, f32, f32)> = stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
        })?.filter_map(|r| r.ok()).collect();

        let mut agg: std::collections::HashMap<String, Agg> = std::collections::HashMap::new();
        for (file_name, sha256, face_id, conf, bx, by, bw, bh) in face_rows {
            let entry = agg.entry(face_id).or_default();
            if entry.seen_media_sha.insert(sha256) {
                entry.photo_count += 1;
            }
            if entry.cover_name.is_none() {
                entry.cover_name = Some(file_name.clone());
            }
            let should_replace = match entry.best_conf {
                None => true,
                Some(prev) => conf > prev,
            };
            if should_replace {
                entry.best_conf = Some(conf);
                entry.best_bbox = Some(Bbox { x: bx, y: by, w: bw, h: bh });
                entry.best_cover_name = Some(file_name);
            }
        }

        let mut person_stmt = conn.prepare(
            "SELECT face_id, name, thumbnail, conf, face_bbox_x, face_bbox_y, face_bbox_w, face_bbox_h FROM person"
        )?;
        struct PersonRow {
            face_id: String, name: Option<String>, thumbnail: Option<String>,
            conf: Option<f32>, face_bbox: Option<Bbox>,
        }
        let person_rows: Vec<PersonRow> = person_stmt.query_map([], |r| {
            let bx: Option<f32> = r.get(4)?;
            let by: Option<f32> = r.get(5)?;
            let bw: Option<f32> = r.get(6)?;
            let bh: Option<f32> = r.get(7)?;
            let bbox = match (bx, by, bw, bh) {
                (Some(x), Some(y), Some(w), Some(h)) => Some(Bbox { x, y, w, h }),
                _ => None,
            };
            Ok(PersonRow {
                face_id: r.get(0)?, name: r.get(1)?, thumbnail: r.get(2)?,
                conf: r.get(3)?, face_bbox: bbox,
            })
        })?.filter_map(|r| r.ok()).collect();

        let person_map: std::collections::HashMap<String, PersonRow> = person_rows
            .into_iter().map(|p| (p.face_id.clone(), p)).collect();

        let base = source_dir.trim_end_matches('/');
        let mut rows: Vec<PersonGroup> = agg.into_iter().map(|(face_id, a)| {
            let person = person_map.get(&face_id);
            let cover_name = a.best_cover_name.clone().or(a.cover_name.clone());
            let cover_path = cover_name.as_ref().map(|n| format!("{}/{}", base, n));
            let thumb_raw = person
                .and_then(|p| p.thumbnail.clone())
                .or_else(|| cover_name.clone());
            let thumbnail = thumb_raw.map(|t| {
                if std::path::Path::new(&t).is_absolute() {
                    t
                } else if t.starts_with("face_") || t.contains("_face_") {
                    cache_dir.join("faces").join(t).to_string_lossy().to_string()
                } else {
                    std::path::Path::new(base).join(t).to_string_lossy().to_string()
                }
            });
            let conf = person.and_then(|p| p.conf).or(a.best_conf);
            let bbox = person
                .and_then(|p| p.face_bbox.clone())
                .or(a.best_bbox.clone());
            PersonGroup {
                face_id,
                name: person.and_then(|p| p.name.clone()),
                photo_count: a.photo_count,
                cover_path,
                thumbnail,
                conf,
                face_bbox: bbox.map(|b| BboxInfo { x: b.x, y: b.y, w: b.w, h: b.h }),
            }
        }).collect();
        rows.sort_by(|a, b| b.photo_count.cmp(&a.photo_count).then_with(|| a.face_id.cmp(&b.face_id)));
        // Only return persons that appear in 2+ distinct photos — single-photo entries are
        // likely false-positive detections and clutter the People view.
        rows.retain(|r| r.photo_count >= 2);
        Ok(rows)
    }

    pub fn merge_people(db: &SqliteDb, target_face_id: &str, source_face_id: &str) -> Result<()> {
        let conn = db.conn();
        use rusqlite::OptionalExtension;

        let source_name: Option<String> = conn.query_row(
            "SELECT name FROM person WHERE face_id = ?1",
            params![source_face_id],
            |r| r.get(0),
        ).optional()?.flatten();

        let target_name: Option<String> = conn.query_row(
            "SELECT name FROM person WHERE face_id = ?1",
            params![target_face_id],
            |r| r.get(0),
        ).optional()?.flatten();

        let final_name = target_name.or(source_name);

        let target_person_id: i64 = conn.query_row(
            "SELECT id FROM person WHERE face_id = ?1",
            params![target_face_id],
            |r| r.get(0),
        )?;
        let source_person_id: i64 = conn.query_row(
            "SELECT id FROM person WHERE face_id = ?1",
            params![source_face_id],
            |r| r.get(0),
        )?;

        conn.execute(
            "UPDATE media_faces SET person_id = ?1, name = ?2 WHERE person_id = ?3",
            params![target_person_id, final_name, source_person_id],
        )?;

        conn.execute("DELETE FROM person WHERE face_id = ?1", params![source_face_id])?;

        if let Some(ref n) = final_name {
            conn.execute(
                "UPDATE person SET name = ?2 WHERE face_id = ?1",
                params![target_face_id, n],
            )?;
        }
        Ok(())
    }

    pub fn delete_person(db: &SqliteDb, face_id: &str) -> Result<()> {
        let conn = db.conn();
        conn.execute("DELETE FROM media_faces WHERE person_id = (SELECT id FROM person WHERE face_id = ?1)", params![face_id])?;
        conn.execute("DELETE FROM person WHERE face_id = ?1", params![face_id])?;
        Ok(())
    }

    pub fn remove_face_from_person(db: &SqliteDb, media_id: &str, face_id: &str) -> Result<()> {
        let conn = db.conn();
        conn.execute(
            "DELETE FROM media_faces WHERE media_id = ?1 AND person_id = (SELECT id FROM person WHERE face_id = ?2)",
            params![media_id, face_id],
        )?;
        Ok(())
    }

    pub fn generate_missing_person_thumbnails(
        db: &SqliteDb,
        source_dir: &str,
        cache_dir: &std::path::Path,
    ) -> Result<usize> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT face_id, thumbnail FROM person"
        )?;
        struct PersonThumbInfo {
            face_id: String,
            thumbnail: Option<String>,
        }
        let persons: Vec<PersonThumbInfo> = stmt.query_map([], |r| {
            Ok(PersonThumbInfo {
                face_id: r.get(0)?,
                thumbnail: r.get(1)?,
            })
        })?.filter_map(|r| r.ok()).collect();

        let mut count = 0;

        for p in persons {
            let needs_generation = match &p.thumbnail {
                None => true,
                Some(t) => {
                    let path = if std::path::Path::new(t).is_absolute() {
                        std::path::PathBuf::from(t)
                    } else {
                        cache_dir.join("faces").join(t)
                    };
                    !t.contains("face_") || !path.exists()
                }
            };

            if needs_generation {
                use rusqlite::OptionalExtension;
                let best_face: Option<(String, f32, f32, f32, f32, f32)> = conn.query_row(
                    "SELECT m.file_name, mf.conf, mf.bbox_x, mf.bbox_y, mf.bbox_w, mf.bbox_h
                     FROM media_faces mf
                     JOIN media m ON m.id = mf.media_id
                     JOIN person p ON mf.person_id = p.id
                     WHERE p.face_id = ?1 AND m.deleted_at IS NULL AND m.is_hidden = 0
                     ORDER BY mf.conf DESC LIMIT 1",
                    params![p.face_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
                ).optional()?;

                if let Some((file_name, conf, bx, by, bw, bh)) = best_face {
                    let full_path = std::path::Path::new(source_dir).join(&file_name);
                    if full_path.exists() {
                        let bbox = Bbox { x: bx, y: by, w: bw, h: bh };
                        if let Ok(img) = image::open(&full_path) {
                            match crate::infrastructure::ingest::image_processor::generate_face_thumbnail(
                                &img,
                                &bbox,
                                &cache_dir.join("faces"),
                                &p.face_id,
                            ) {
                                Ok(face_thumb_path) => {
                                    let filename = std::path::Path::new(&face_thumb_path)
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or(&face_thumb_path);
                                    conn.execute(
                                        "UPDATE person SET thumbnail = ?2, conf = ?3,
                                                face_bbox_x = ?4, face_bbox_y = ?5, face_bbox_w = ?6, face_bbox_h = ?7
                                         WHERE face_id = ?1",
                                        params![p.face_id, filename, conf, bx, by, bw, bh],
                                    )?;
                                    count += 1;
                                    crate::log_info!("🖼️ Generated missing face thumbnail for person {}: {}", p.face_id, face_thumb_path);
                                }
                                Err(e) => {
                                    crate::log_warn!("⚠️ Failed to generate face thumbnail for person {}: {}", p.face_id, e);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(count)
    }
}
