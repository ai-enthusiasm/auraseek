use anyhow::Result;
use crate::infrastructure::database::surreal::SurrealDb;
use crate::infrastructure::database::models::{PersonDoc, FaceEntry, FileInfo, Bbox};
use crate::core::models::PersonGroup;
use surrealdb::types::SurrealValue;
use super::DbOperations;

impl DbOperations {
    pub async fn upsert_person(db: &SurrealDb, person: PersonDoc) -> Result<()> {
        let fid = person.face_id.clone();
        let name = person.name.clone();
        let thumb = person.thumbnail.clone();
        let conf = person.conf;
        let bbox = person.face_bbox.clone();
        db.db.query(
            "INSERT INTO person { face_id: $fid, name: $name, thumbnail: $thumb, conf: $conf, face_bbox: $bbox }
             ON DUPLICATE KEY UPDATE
                name = $input.name ?? name,
                conf = IF $input.conf IS NOT NONE AND (conf IS NONE OR $input.conf > conf) THEN $input.conf ELSE conf END,
                thumbnail = IF $input.conf IS NOT NONE AND (conf IS NONE OR $input.conf > conf) THEN $input.thumbnail ELSE thumbnail END,
                face_bbox = IF $input.conf IS NOT NONE AND (conf IS NONE OR $input.conf > conf) THEN $input.face_bbox ELSE face_bbox END"
        )
        .bind(("fid", fid)).bind(("name", name)).bind(("thumb", thumb)).bind(("conf", conf)).bind(("bbox", bbox))
        .await?.check().map_err(|e| anyhow::anyhow!("upsert_person failed: {}", e))?;
        Ok(())
    }

    pub async fn name_person(db: &SurrealDb, face_id: &str, name: &str) -> Result<()> {
        let fid = face_id.to_string();
        let n = name.to_string();
        db.db.query("UPDATE person SET name = $name WHERE face_id = $fid")
            .bind(("name", n.clone())).bind(("fid", fid.clone())).await?;
        db.db.query("UPDATE media SET faces[WHERE face_id = $fid].name = $name WHERE faces.*.face_id CONTAINS $fid")
            .bind(("fid", fid)).bind(("name", n)).await?;
        Ok(())
    }

    pub async fn get_people(db: &SurrealDb, source_dir: &str) -> Result<Vec<PersonGroup>> {
        #[derive(serde::Deserialize, SurrealValue)]
        struct MediaFacesRow {
            file: FileInfo,
            faces: Vec<FaceEntry>,
        }

        #[derive(Default, Clone)]
        struct Agg {
            photo_count: u32,
            cover_name: Option<String>,
            best_conf: Option<f32>,
            best_bbox: Option<Bbox>,
            best_cover_name: Option<String>,
            seen_media_sha: std::collections::HashSet<String>,
        }

        let mut media_res = db.db.query(
            "SELECT file, faces FROM media
             WHERE deleted_at = NONE AND is_hidden = false AND array::len(faces) > 0"
        ).await?;
        let media_rows: Vec<MediaFacesRow> = media_res.take(0)?;

        let mut agg: std::collections::HashMap<String, Agg> = std::collections::HashMap::new();
        for row in media_rows {
            for f in row.faces {
                let entry = agg.entry(f.face_id.clone()).or_default();
                if entry.seen_media_sha.insert(row.file.sha256.clone()) {
                    entry.photo_count += 1;
                }
                if entry.cover_name.is_none() {
                    entry.cover_name = Some(row.file.name.clone());
                }
                let should_replace = match (entry.best_conf, f.conf) {
                    (None, _) => true,
                    (Some(prev), cur) => cur > prev,
                };
                if should_replace {
                    entry.best_conf = Some(f.conf);
                    entry.best_bbox = Some(f.bbox.clone());
                    entry.best_cover_name = Some(row.file.name.clone());
                }
            }
        }

        #[derive(serde::Deserialize, SurrealValue)]
        struct PersonRow {
            face_id: String,
            name: Option<String>,
            thumbnail: Option<String>,
            conf: Option<f32>,
            face_bbox: Option<Bbox>,
        }
        let mut person_res = db.db.query(
            "SELECT face_id, name, thumbnail, conf, face_bbox FROM person"
        ).await?;
        let person_rows: Vec<PersonRow> = person_res.take(0)?;
        let person_map: std::collections::HashMap<String, PersonRow> = person_rows
            .into_iter()
            .map(|p| (p.face_id.clone(), p))
            .collect();

        let base = source_dir.trim_end_matches('/');
        let mut rows: Vec<PersonGroup> = agg.into_iter().map(|(face_id, a)| {
            let person = person_map.get(&face_id);
            // Keep cover image consistent with bbox source to avoid wrong/blank avatar crop.
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
                face_bbox: bbox.map(|b| crate::core::models::BboxInfo { x: b.x, y: b.y, w: b.w, h: b.h }),
            }
        }).collect();
        rows.sort_by(|a, b| b.photo_count.cmp(&a.photo_count).then_with(|| a.face_id.cmp(&b.face_id)));
        Ok(rows)
    }

    pub async fn merge_people(db: &SurrealDb, target_face_id: &str, source_face_id: &str) -> Result<()> {
        let src = source_face_id.to_string();
        let tgt = target_face_id.to_string();
        let src_id = if src.contains(':') { src.clone() } else { format!("person:{}", src) };
        let tgt_id = if tgt.contains(':') { tgt.clone() } else { format!("person:{}", tgt) };

        #[derive(serde::Deserialize, SurrealValue)]
        struct NameRow { name: Option<String> }
        let mut res = db.db.query("SELECT name FROM type::record($id)").bind(("id", src_id.clone())).await?;
        let source_name: Option<NameRow> = res.take(0)?;
        let mut res2 = db.db.query("SELECT name FROM type::record($id)").bind(("id", tgt_id.clone())).await?;
        let target_name_row: Option<NameRow> = res2.take(0)?;
        let final_name: Option<String> = target_name_row.and_then(|r| r.name).or(source_name.and_then(|r| r.name));

        db.db.query("UPDATE media SET faces[WHERE face_id = $src_raw].face_id = $tgt_raw, faces[WHERE face_id = $src_raw].name = $nm WHERE faces.*.face_id CONTAINS $src_raw")
            .bind(("src_raw", src)).bind(("tgt_raw", tgt)).bind(("nm", final_name.clone())).await?;
        db.db.query("DELETE type::record($id)").bind(("id", src_id)).await?.check()?;
        if let Some(ref n) = final_name {
            db.db.query("UPDATE type::record($id) SET name = $name").bind(("id", tgt_id)).bind(("name", n.clone())).await?.check()?;
        }
        Ok(())
    }

    pub async fn delete_person(db: &SurrealDb, face_id: &str) -> Result<()> {
        let fid = face_id.to_string();
        db.db.query("UPDATE media SET faces = faces.filter(|$f| $f.face_id != $fid) WHERE faces.*.face_id CONTAINS $fid")
            .bind(("fid", fid.clone())).await?;
        let table_id = if fid.contains(':') { fid } else { format!("person:{}", fid) };
        db.db.query("DELETE type::record($id)").bind(("id", table_id)).await?.check()?;
        Ok(())
    }

    pub async fn remove_face_from_person(db: &SurrealDb, media_id: &str, face_id: &str) -> Result<()> {
        let fid = face_id.to_string();
        let query = format!("UPDATE {} SET faces = faces.filter(|$f| $f.face_id != $fid)", media_id);
        db.db.query(&query).bind(("fid", fid)).await?.check()?;
        Ok(())
    }
}
