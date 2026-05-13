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

        let existing: Option<(Option<f32>,)> = conn.query_row(
            "SELECT conf FROM person WHERE face_id = ?1",
            params![person.face_id],
            |r| Ok((r.get(0)?,)),
        ).optional()?;

        match existing {
            Some((old_conf,)) => {
                let should_upgrade = match (person.conf, old_conf) {
                    (Some(_), None) => true,
                    (Some(new_c), Some(old_c)) => new_c > old_c,
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
                } else if person.name.is_some() {
                    conn.execute(
                        "UPDATE person SET name = COALESCE(?2, name) WHERE face_id = ?1",
                        params![person.face_id, person.name],
                    )?;
                }
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                let (bx, by, bw, bh) = match &person.face_bbox {
                    Some(b) => (Some(b.x), Some(b.y), Some(b.w), Some(b.h)),
                    None => (None, None, None, None),
                };
                conn.execute(
                    "INSERT INTO person (id, face_id, name, thumbnail, conf, face_bbox_x, face_bbox_y, face_bbox_w, face_bbox_h)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![id, person.face_id, person.name, person.thumbnail, person.conf, bx, by, bw, bh],
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
            "UPDATE media_faces SET name = ?2 WHERE face_id = ?1",
            params![face_id, name],
        )?;
        Ok(())
    }

    pub fn get_people(db: &SqliteDb, source_dir: &str) -> Result<Vec<PersonGroup>> {
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
            "SELECT m.file_name, m.file_sha256, mf.face_id, mf.conf, mf.bbox_x, mf.bbox_y, mf.bbox_w, mf.bbox_h
             FROM media_faces mf
             JOIN media m ON m.id = mf.media_id
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
                if std::path::Path::new(&t).is_absolute() { t } else { format!("{}/{}", base, t) }
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

        conn.execute(
            "UPDATE media_faces SET face_id = ?1, name = ?2 WHERE face_id = ?3",
            params![target_face_id, final_name, source_face_id],
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
        conn.execute("DELETE FROM media_faces WHERE face_id = ?1", params![face_id])?;
        conn.execute("DELETE FROM person WHERE face_id = ?1", params![face_id])?;
        Ok(())
    }

    pub fn remove_face_from_person(db: &SqliteDb, media_id: &str, face_id: &str) -> Result<()> {
        let conn = db.conn();
        conn.execute(
            "DELETE FROM media_faces WHERE media_id = ?1 AND face_id = ?2",
            params![media_id, face_id],
        )?;
        Ok(())
    }
}
