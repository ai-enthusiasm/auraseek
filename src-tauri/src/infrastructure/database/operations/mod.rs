pub mod media;
pub mod embedding;
pub mod person;
pub mod trash;
pub mod search;
pub mod config;
pub mod album;
pub mod duplicates;

use anyhow::Result;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use crate::core::models::{
    SearchResult, SearchResultMeta, TimelineItem, TimelineGroup,
    DetectedObject, BboxInfo, DetectedFace,
};
use crate::infrastructure::database::models::{
    MediaRow, FileInfo, MediaMetadata, ObjectEntry, FaceEntry, Bbox,
};

pub struct DbOperations;

pub fn row_to_search_result(row: &MediaRow, score: f32, source_dir: &str) -> SearchResult {
    let base = source_dir.trim_end_matches('/');
    SearchResult {
        media_id:         row.id.clone(),
        similarity_score: score,
        file_path:        format!("{}/{}", base, row.file.name),
        media_type:       row.media_type.clone(),
        width:            row.metadata.width,
        height:           row.metadata.height,
        detected_objects: row.objects.iter().map(|o| DetectedObject {
            class_name: o.class_name.clone(), conf: o.conf,
            bbox: BboxInfo { x: o.bbox.x, y: o.bbox.y, w: o.bbox.w, h: o.bbox.h },
            mask_rle: o.mask_rle.clone(),
        }).collect(),
        detected_faces: row.faces.iter().map(|f| DetectedFace {
            face_id: f.face_id.clone(), name: f.name.clone(), conf: f.conf,
            bbox: BboxInfo { x: f.bbox.x, y: f.bbox.y, w: f.bbox.w, h: f.bbox.h },
        }).collect(),
        metadata: SearchResultMeta {
            width: row.metadata.width, height: row.metadata.height,
            created_at: row.metadata.created_at.clone(),
            objects: row.objects.iter().map(|o| o.class_name.clone()).collect(),
            faces: row.faces.iter().filter_map(|f| f.name.clone()).collect(),
        },
        thumbnail_path: row.thumbnail.clone(),
    }
}

/// Read objects from media_objects table for a given media_id.
fn read_objects(conn: &Connection, media_id: &str) -> Result<Vec<ObjectEntry>> {
    let mut stmt = conn.prepare(
        "SELECT class_name, conf, bbox_x, bbox_y, bbox_w, bbox_h, mask_area, mask_path, mask_rle
         FROM media_objects WHERE media_id = ?1"
    )?;
    let rows = stmt.query_map(params![media_id], |r| {
        let mask_rle_json: Option<String> = r.get(8)?;
        Ok(ObjectEntry {
            class_name: r.get(0)?,
            conf:       r.get(1)?,
            bbox: Bbox { x: r.get(2)?, y: r.get(3)?, w: r.get(4)?, h: r.get(5)? },
            mask_area:  r.get(6)?,
            mask_path:  r.get(7)?,
            mask_rle:   mask_rle_json.and_then(|j| serde_json::from_str(&j).ok()),
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Read faces from media_faces table for a given media_id.
fn read_faces(conn: &Connection, media_id: &str) -> Result<Vec<FaceEntry>> {
    let mut stmt = conn.prepare(
        "SELECT face_id, name, conf, bbox_x, bbox_y, bbox_w, bbox_h
         FROM media_faces WHERE media_id = ?1"
    )?;
    let rows = stmt.query_map(params![media_id], |r| {
        Ok(FaceEntry {
            face_id: r.get(0)?,
            name:    r.get(1)?,
            conf:    r.get(2)?,
            bbox: Bbox { x: r.get(3)?, y: r.get(4)?, w: r.get(5)?, h: r.get(6)? },
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Assemble a MediaRow from a rusqlite Row (base media columns) + child tables.
fn media_row_from_sqlite(r: &rusqlite::Row) -> rusqlite::Result<MediaRowBase> {
    Ok(MediaRowBase {
        id:         r.get("id")?,
        media_type: r.get("media_type")?,
        file_name:  r.get("file_name")?,
        file_size:  r.get::<_, i64>("file_size")? as u64,
        file_sha256:r.get("file_sha256")?,
        file_phash: r.get("file_phash")?,
        meta_width: r.get("meta_width")?,
        meta_height:r.get("meta_height")?,
        meta_duration: r.get("meta_duration")?,
        meta_fps:   r.get("meta_fps")?,
        meta_created_at: r.get("meta_created_at")?,
        meta_modified_at: r.get("meta_modified_at")?,
        processed:  r.get::<_, i32>("processed")? != 0,
        favorite:   r.get::<_, i32>("favorite")? != 0,
        is_hidden:  r.get::<_, i32>("is_hidden")? != 0,
        deleted_at: r.get("deleted_at")?,
        thumbnail:  r.get("thumbnail")?,
    })
}

struct MediaRowBase {
    id: String, media_type: String, file_name: String, file_size: u64,
    file_sha256: String, file_phash: Option<String>,
    meta_width: Option<u32>, meta_height: Option<u32>,
    meta_duration: Option<f64>, meta_fps: Option<f64>,
    meta_created_at: Option<String>, meta_modified_at: Option<String>,
    processed: bool, favorite: bool, is_hidden: bool,
    deleted_at: Option<String>, thumbnail: Option<String>,
}

fn base_to_media_row(b: MediaRowBase, objects: Vec<ObjectEntry>, faces: Vec<FaceEntry>) -> MediaRow {
    MediaRow {
        id: b.id,
        media_type: b.media_type,
        file: FileInfo { name: b.file_name, size: b.file_size, sha256: b.file_sha256, phash: b.file_phash },
        metadata: MediaMetadata {
            width: b.meta_width, height: b.meta_height,
            duration: b.meta_duration, fps: b.meta_fps,
            created_at: b.meta_created_at, modified_at: b.meta_modified_at,
        },
        objects, faces,
        processed: b.processed, favorite: b.favorite,
        thumbnail: b.thumbnail, deleted_at: b.deleted_at, is_hidden: b.is_hidden,
    }
}

/// Read a single MediaRow by ID, including objects and faces from child tables.
pub fn read_media_row(conn: &Connection, media_id: &str) -> Result<Option<MediaRow>> {
    use rusqlite::OptionalExtension;
    let base = conn.query_row(
        "SELECT * FROM media WHERE id = ?1", params![media_id], media_row_from_sqlite,
    ).optional()?;
    match base {
        None => Ok(None),
        Some(b) => {
            let objects = read_objects(conn, media_id)?;
            let faces = read_faces(conn, media_id)?;
            Ok(Some(base_to_media_row(b, objects, faces)))
        }
    }
}

/// Execute a SQL query that returns media rows, and read full MediaRow for each
/// (including objects/faces from child tables).
pub fn read_media_rows_from_query(conn: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<Vec<MediaRow>> {
    let mut stmt = conn.prepare(sql)?;
    let bases: Vec<MediaRowBase> = stmt.query_map(params, media_row_from_sqlite)?
        .filter_map(|r| r.ok())
        .collect();
    let mut rows = Vec::with_capacity(bases.len());
    for b in bases {
        let objects = read_objects(conn, &b.id)?;
        let faces = read_faces(conn, &b.id)?;
        rows.push(base_to_media_row(b, objects, faces));
    }
    Ok(rows)
}

impl DbOperations {
    pub(crate) fn group_rows_into_timeline(rows: Vec<MediaRow>, source_dir: &str) -> Result<Vec<TimelineGroup>> {
        let base = source_dir.trim_end_matches('/');
        let mut groups: HashMap<(i32, u32), TimelineGroup> = HashMap::new();

        let mut sorted_rows = rows;
        sorted_rows.sort_by(|a, b| {
            let (ay, am) = parse_ym(a);
            let (by, bm) = parse_ym(b);
            if ay != by { by.cmp(&ay) } else { bm.cmp(&am) }
        });

        for row in sorted_rows {
            let (year, month) = parse_ym(&row);
            let label = format_month_label(year, month);
            let file_path = format!("{}/{}", base, row.file.name);
            let thumbnail_path = row.thumbnail.clone();
            let item = TimelineItem {
                media_id:   row.id.clone(),
                file_path, media_type: row.media_type.clone(),
                width: row.metadata.width, height: row.metadata.height,
                created_at: row.metadata.created_at.clone(),
                objects:  row.objects.iter().map(|o| o.class_name.clone()).collect(),
                faces:    row.faces.iter().filter_map(|f| f.name.clone()).collect(),
                face_ids: row.faces.iter().map(|f| f.face_id.clone()).collect(),
                favorite: row.favorite,
                deleted_at: row.deleted_at.clone(),
                is_hidden: row.is_hidden,
                thumbnail_path,
                detected_objects: row.objects.iter().map(|o| DetectedObject {
                    class_name: o.class_name.clone(), conf: o.conf,
                    bbox: BboxInfo { x: o.bbox.x, y: o.bbox.y, w: o.bbox.w, h: o.bbox.h },
                    mask_rle: o.mask_rle.clone(),
                }).collect(),
                detected_faces: row.faces.iter().map(|f| DetectedFace {
                    face_id: f.face_id.clone(), name: f.name.clone(), conf: f.conf,
                    bbox: BboxInfo { x: f.bbox.x, y: f.bbox.y, w: f.bbox.w, h: f.bbox.h },
                }).collect(),
            };
            groups.entry((year, month)).or_insert_with(|| TimelineGroup {
                label, year, month, day: None, items: vec![],
            }).items.push(item);
        }

        let mut result: Vec<TimelineGroup> = groups.into_values().collect();
        result.sort_by(|a, b| b.year.cmp(&a.year).then(b.month.cmp(&a.month)));
        Ok(result)
    }
}

fn parse_ym(row: &MediaRow) -> (i32, u32) {
    if let Some(ref s) = row.metadata.created_at {
        if let Some(ym) = parse_year_month_from_str(s) { return ym; }
    }
    if let Some(ref s) = row.metadata.modified_at {
        if let Some(ym) = parse_year_month_from_str(s) { return ym; }
    }
    (1970, 1)
}

fn format_month_label(year: i32, month: u32) -> String {
    let months = ["Tháng 1","Tháng 2","Tháng 3","Tháng 4","Tháng 5","Tháng 6",
                  "Tháng 7","Tháng 8","Tháng 9","Tháng 10","Tháng 11","Tháng 12"];
    let m = months.get((month.saturating_sub(1)) as usize).unwrap_or(&"");
    format!("{} {}", m, year)
}

pub(crate) fn parse_year_month_from_str(s: &str) -> Option<(i32, u32)> {
    use chrono::Datelike;
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) { return Some((dt.year(), dt.month())); }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") { return Some((dt.year(), dt.month())); }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") { return Some((dt.year(), dt.month())); }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") { return Some((dt.year(), dt.month())); }
    if let Ok(dt) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") { return Some((dt.year(), dt.month())); }
    if s.len() >= 7 {
        let parts: Vec<&str> = s.split(|c: char| c == '-' || c == '/').collect();
        if parts.len() >= 2 {
            if let (Ok(y), Ok(m)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) {
                if (1900..=2100).contains(&y) && (1..=12).contains(&m) { return Some((y, m)); }
            }
        }
    }
    None
}
