/// Database operations – SurrealDB v3 edition
/// All vector search uses SurrealDB's built-in vector::similarity::cosine
use anyhow::Result;
use crate::db::surreal::SurrealDb;
use crate::db::models::*;
use std::collections::HashMap;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};

pub fn record_id_to_string(id: &RecordId) -> String {
    let key_str = match &id.key {
        RecordIdKey::String(s) => s.clone(),
        RecordIdKey::Number(n) => n.to_string(),
        _ => "unknown".to_string(),
    };
    format!("{}:{}", id.table, key_str)
}

#[allow(dead_code)]
fn strip_table_prefix(id: &str) -> &str {
    id.find(':').map(|i| &id[i+1..]).unwrap_or(id)
}

/// Build a `SearchResult` from a `MediaRow`, deriving the file path from source_dir.
pub fn row_to_search_result(row: &MediaRow, score: f32, source_dir: &str) -> SearchResult {
    let id_str  = record_id_to_string(&row.id);
    let base    = source_dir.trim_end_matches('/');
    SearchResult {
        media_id:         id_str,
        similarity_score: score,
        file_path:        format!("{}/{}", base, row.file.name),
        media_type:       row.media_type.clone(),
        width:            row.metadata.width,
        height:           row.metadata.height,
        detected_objects: row.objects.iter().map(|o| DetectedObject {
            class_name: o.class_name.clone(),
            conf:       o.conf,
            bbox:       BboxInfo { x: o.bbox.x, y: o.bbox.y, w: o.bbox.w, h: o.bbox.h },
            mask_rle:   o.mask_rle.clone(),
        }).collect(),
        detected_faces: row.faces.iter().map(|f| DetectedFace {
            face_id: f.face_id.clone(),
            name:    f.name.clone(),
            conf:    f.conf,
            bbox:    BboxInfo { x: f.bbox.x, y: f.bbox.y, w: f.bbox.w, h: f.bbox.h },
        }).collect(),
        metadata: SearchResultMeta {
            width:      row.metadata.width,
            height:     row.metadata.height,
            created_at: row.metadata.created_at.as_ref().map(|dt| dt.to_string()),
            objects:    row.objects.iter().map(|o| o.class_name.clone()).collect(),
            faces:      row.faces.iter().filter_map(|f| f.name.clone()).collect(),
        },
    }
}

pub struct DbOperations;

impl DbOperations {
    // ─── Media CRUD ──────────────────────────────────────────────────

    /// Check duplicate by SHA-256
    pub async fn is_duplicate_sha256(db: &SurrealDb, sha256: &str) -> Result<bool> {
        let sha = sha256.to_string();
        let mut res = db.db.query(
            "SELECT id FROM media WHERE file.sha256 = $sha LIMIT 1"
        )
        .bind(("sha", sha))
        .await?;
        let rows: Vec<IdOnly> = res.take(0)?;
        Ok(!rows.is_empty())
    }

    /// Insert a new media document, returns the record id as string
    pub async fn insert_media(db: &SurrealDb, doc: MediaDoc) -> Result<String> {
        let created: Option<IdOnly> = db.db
            .create("media")
            .content(doc)
            .await?;
        let id = created
            .ok_or_else(|| anyhow::anyhow!("Failed to create media record"))?
            .id;
        Ok(record_id_to_string(&id))
    }

    /// Update AI results on a media record
    pub async fn update_media_ai(
        db: &SurrealDb,
        media_id: &str,
        objects: Vec<ObjectEntry>,
        faces: Vec<FaceEntry>,
    ) -> Result<()> {
        let objs_json = serde_json::to_string(&objects)?;
        let faces_json = serde_json::to_string(&faces)?;
        let query = format!(
            "UPDATE {} SET objects = $objs, faces = $faces, processed = true",
            media_id
        );
        db.db.query(&query)
        .bind(("objs", serde_json::from_str::<serde_json::Value>(&objs_json)?))
        .bind(("faces", serde_json::from_str::<serde_json::Value>(&faces_json)?))
        .await?
        .check()
        .map_err(|e| anyhow::anyhow!("update_media_ai failed: {}", e))?;
        Ok(())
    }

    // ─── Embeddings (vector stored in SurrealDB) ─────────────────────

    /// Insert embedding vector
    pub async fn insert_embedding(
        db: &SurrealDb,
        media_id: &str,
        source: &str,
        frame_ts: Option<f64>,
        frame_idx: Option<u32>,
        embedding: Vec<f32>,
    ) -> Result<()> {
        let src = source.to_string();
        let query = format!(
            "CREATE embedding SET
                media_id = {},
                source   = $src,
                frame_ts = $fts,
                frame_idx = $fidx,
                vec      = $vec",
            media_id
        );
        db.db.query(&query)
        .bind(("src", src))
        .bind(("fts", frame_ts))
        .bind(("fidx", frame_idx))
        .bind(("vec", embedding))
        .await?
        .check()
        .map_err(|e| anyhow::anyhow!("insert_embedding failed: {}", e))?;
        Ok(())
    }

    /// Vector search using cosine similarity (SurrealDB built-in)
    pub async fn vector_search(
        db: &SurrealDb,
        query_vec: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<(String, f32)>> {
        let mut res = db.db.query(
            "SELECT
                media_id,
                vector::similarity::cosine(vec, $qvec) AS score
            FROM embedding
            WHERE vector::similarity::cosine(vec, $qvec) >= $thresh
            ORDER BY score DESC
            LIMIT $lim"
        )
        .bind(("qvec", query_vec.to_vec()))
        .bind(("thresh", threshold))
        .bind(("lim", limit))
        .await?;

        #[derive(serde::Deserialize, SurrealValue)]
        struct Hit {
            media_id: RecordId,
            score: f32,
        }
        let hits: Vec<Hit> = res.take(0)?;
        Ok(hits.into_iter().map(|h| (record_id_to_string(&h.media_id), h.score)).collect())
    }

    /// Get embedding count
    pub async fn embedding_count(db: &SurrealDb) -> Result<u64> {
        let mut res = db.db.query("SELECT count() as cnt FROM embedding GROUP ALL").await?;
        #[derive(serde::Deserialize, SurrealValue)]
        struct C { cnt: u64 }
        let rows: Vec<C> = res.take(0)?;
        Ok(rows.first().map(|r| r.cnt).unwrap_or(0))
    }

    // ─── Person / Face ───────────────────────────────────────────────

    /// Upsert a person (face cluster)
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
        .bind(("fid", fid))
        .bind(("name", name))
        .bind(("thumb", thumb))
        .bind(("conf", conf))
        .bind(("bbox", bbox))
        .await?
        .check()
        .map_err(|e| anyhow::anyhow!("upsert_person failed: {}", e))?;
        Ok(())
    }

    /// Name a face cluster
    pub async fn name_person(db: &SurrealDb, face_id: &str, name: &str) -> Result<()> {
        let fid = face_id.to_string();
        let n = name.to_string();
        // Update person table
        db.db.query("UPDATE person SET name = $name WHERE face_id = $fid")
            .bind(("name", n.clone()))
            .bind(("fid", fid.clone()))
            .await?;
        // Update face entries embedded in media docs
        db.db.query(
            "UPDATE media SET faces = faces.map(|$f| IF $f.face_id = $fid THEN $f.{*, name: $name} ELSE $f END) WHERE faces.*.face_id CONTAINS $fid"
        )
        .bind(("fid", fid))
        .bind(("name", n))
        .await?;
        Ok(())
    }

    // ─── Favorites ─────────────────────────────────────────────────────

    pub async fn toggle_favorite(db: &SurrealDb, media_id: &str) -> Result<bool> {
        let query = format!(
            "UPDATE {} SET favorite = !favorite RETURN AFTER",
            media_id
        );
        let mut res = db.db.query(&query).await?
            .check()
            .map_err(|e| anyhow::anyhow!("toggle_favorite failed: {}", e))?;

        #[derive(serde::Deserialize, SurrealValue)]
        struct FavRow { favorite: bool }
        let rows: Vec<FavRow> = res.take(0)?;
        Ok(rows.first().map(|r| r.favorite).unwrap_or(false))
    }

    // ─── Config (source_dir) ─────────────────────────────────────────

    /// Get the configured source directory (always stored as `config_auraseek:main`).
    pub async fn get_source_dir(db: &SurrealDb) -> Result<Option<String>> {
        let mut res = db.db.query("SELECT source_dir FROM config_auraseek:main").await?;
        #[derive(serde::Deserialize, SurrealValue)]
        struct Row { source_dir: Option<String> }
        let rows: Vec<Row> = res.take(0)?;
        Ok(rows.into_iter().next().and_then(|r| r.source_dir))
    }

    /// Upsert the source directory into a single config record `config_auraseek:main`.
    pub async fn set_source_dir(db: &SurrealDb, source_dir: &str) -> Result<()> {
        let dir = source_dir.to_string();
        db.db.query(
            "UPSERT config_auraseek:main SET source_dir = $dir, updated_at = time::now()"
        )
        .bind(("dir", dir))
        .await?
        .check()
        .map_err(|e| anyhow::anyhow!("set_source_dir failed: {}", e))?;
        Ok(())
    }

    /// Delete all media, embeddings, and persons for a fresh start.
    pub async fn clear_database(db: &SurrealDb) -> Result<()> {
        db.db.query("DELETE media").await?.check()?;
        db.db.query("DELETE embedding").await?.check()?;
        db.db.query("DELETE person").await?.check()?;
        db.db.query("DELETE search_history").await?.check()?;
        Ok(())
    }

    /// Prune any media records whose files no longer exist on disk.
    pub async fn prune_missing_media(db: &SurrealDb, source_dir: &str) -> Result<usize> {
        let mut res = db.db.query("SELECT id, file.name AS name FROM media").await?;
        #[derive(serde::Deserialize, SurrealValue)]
        struct IdNameRow { id: RecordId, name: Option<String> }
        let rows: Vec<IdNameRow> = res.take(0)?;
        
        let mut count = 0;
        let base = std::path::Path::new(source_dir);
        
        for r in rows {
            if let Some(name) = r.name {
                let path = base.join(&name);
                if !path.exists() {
                    let id_str = record_id_to_string(&r.id);
                    crate::log_info!("🗑️ Pruning missing file: {}", name);
                    
                    // Delete embedding first
                    db.db.query(format!("DELETE embedding WHERE media_id = {}", id_str)).await?.check()?;
                    // Delete the media record
                    db.db.query(format!("DELETE {}", id_str)).await?.check()?;
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    // ─── Trash & Hidden ──────────────────────────────────────────────
    
    pub async fn move_to_trash(db: &SurrealDb, media_id: &str) -> Result<()> {
        let query = format!("UPDATE {} SET deleted_at = time::now()", media_id);
        db.db.query(&query).await?.check()
            .map_err(|e| anyhow::anyhow!("move_to_trash failed: {}", e))?;
        Ok(())
    }

    pub async fn restore_from_trash(db: &SurrealDb, media_id: &str) -> Result<()> {
        let query = format!("UPDATE {} SET deleted_at = NONE", media_id);
        db.db.query(&query).await?.check()
            .map_err(|e| anyhow::anyhow!("restore_from_trash failed: {}", e))?;
        Ok(())
    }

    pub async fn get_trash(db: &SurrealDb, source_dir: &str) -> Result<Vec<TimelineGroup>> {
        let mut res = db.db.query(
            "SELECT * FROM media WHERE type::is_none(deleted_at) = false ORDER BY deleted_at ASC"
        ).await?;
        let rows: Vec<MediaRow> = res.take(0)?;
        Self::group_rows_into_timeline(rows, source_dir)
    }

    pub async fn empty_trash(db: &SurrealDb) -> Result<()> {
        // Fetch paths first to delete from disk
        let mut res = db.db.query("SELECT file.path FROM media WHERE type::is_none(deleted_at) = false").await?;
        #[derive(serde::Deserialize, SurrealValue)]
        struct PathRow { path: Option<String> }
        let rows: Vec<PathRow> = res.take(0)?;
        for r in rows.into_iter().filter_map(|r| r.path) {
            let _ = std::fs::remove_file(&r); // best effort
        }
        db.db.query("DELETE media WHERE type::is_none(deleted_at) = false").await?.check()?;
        Ok(())
    }
    
    pub async fn auto_purge_trash(db: &SurrealDb) -> Result<()> {
        let mut res = db.db.query("SELECT file.path FROM media WHERE type::is_none(deleted_at) = false AND deleted_at < time::now() - 30d").await?;
        #[derive(serde::Deserialize, SurrealValue)]
        struct PathRow { path: Option<String> }
        let rows: Vec<PathRow> = res.take(0)?;
        for r in rows.into_iter().filter_map(|r| r.path) {
            let _ = std::fs::remove_file(&r); // best effort
        }
        db.db.query("DELETE media WHERE type::is_none(deleted_at) = false AND deleted_at < time::now() - 30d").await?.check()?;
        Ok(())
    }

    pub async fn hide_photo(db: &SurrealDb, media_id: &str) -> Result<()> {
        let query = format!("UPDATE {} SET is_hidden = true", media_id);
        db.db.query(&query).await?.check()
            .map_err(|e| anyhow::anyhow!("hide_photo failed: {}", e))?;
        Ok(())
    }

    pub async fn unhide_photo(db: &SurrealDb, media_id: &str) -> Result<()> {
        let query = format!("UPDATE {} SET is_hidden = false", media_id);
        db.db.query(&query).await?.check()
            .map_err(|e| anyhow::anyhow!("unhide_photo failed: {}", e))?;
        Ok(())
    }

    pub async fn get_hidden_photos(db: &SurrealDb, source_dir: &str) -> Result<Vec<TimelineGroup>> {
        let mut res = db.db.query(
            "SELECT * FROM media WHERE is_hidden = true AND deleted_at = NONE ORDER BY metadata.created_at DESC"
        ).await?;
        let rows: Vec<MediaRow> = res.take(0)?;
        Self::group_rows_into_timeline(rows, source_dir)
    }

    // ─── Timeline ────────────────────────────────────────────────────

    pub async fn get_timeline(db: &SurrealDb, limit: usize, source_dir: &str) -> Result<Vec<TimelineGroup>> {
        let mut res = db.db.query(
            "SELECT * FROM media WHERE deleted_at = NONE AND is_hidden = false ORDER BY metadata.created_at DESC LIMIT $lim"
        )
        .bind(("lim", limit))
        .await?;
        let rows: Vec<MediaRow> = res.take(0)?;
        Self::group_rows_into_timeline(rows, source_dir)
    }

    fn group_rows_into_timeline(rows: Vec<MediaRow>, source_dir: &str) -> Result<Vec<TimelineGroup>> {

        let mut groups: HashMap<(i32, u32), TimelineGroup> = HashMap::new();

        for row in rows {
            let (year, month) = parse_ym(&row.metadata.created_at);
            let label = format_month_label(year, month);
            let file_path = format!("{}/{}", source_dir.trim_end_matches('/'), row.file.name);
            let item = TimelineItem {
                media_id:   record_id_to_string(&row.id),
                file_path,
                media_type: row.media_type.clone(),
                width:      row.metadata.width,
                height:     row.metadata.height,
                created_at: row.metadata.created_at.as_ref().map(|dt| dt.to_string()),
                objects:    row.objects.iter().map(|o| o.class_name.clone()).collect(),
                faces:      row.faces.iter().filter_map(|f| f.name.clone()).collect(),
                face_ids:   row.faces.iter().map(|f| f.face_id.clone()).collect(),
                favorite:   row.favorite,
                deleted_at: row.deleted_at.as_ref().map(|dt| dt.to_string()),
                is_hidden:  row.is_hidden,
                detected_objects: row.objects.iter().map(|o| DetectedObject {
                    class_name: o.class_name.clone(),
                    conf: o.conf,
                    bbox: BboxInfo { x: o.bbox.x, y: o.bbox.y, w: o.bbox.w, h: o.bbox.h },
                    mask_rle: o.mask_rle.clone(),
                }).collect(),
                detected_faces: row.faces.iter().map(|f| DetectedFace {
                    face_id: f.face_id.clone(),
                    name: f.name.clone(),
                    conf: f.conf,
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

    // ─── Search result resolution ────────────────────────────────────

    pub async fn resolve_search_results(
        db: &SurrealDb,
        hits: Vec<(String, f32)>,
        source_dir: &str,
    ) -> Result<Vec<SearchResult>> {
        if hits.is_empty() { return Ok(vec![]); }

        let mut score_map: HashMap<String, f32> = HashMap::new();
        for (mid, score) in &hits {
            let raw = mid.strip_prefix("media:").unwrap_or(mid);
            let entry = score_map.entry(raw.to_string()).or_insert(0.0);
            if *score > *entry { *entry = *score; }
        }

        let ids: Vec<String> = score_map.keys().cloned().collect();
        let ids_str = ids.iter().map(|id| format!("media:{}", id)).collect::<Vec<_>>().join(", ");
        let query = format!("SELECT * FROM media WHERE id IN [{}] AND deleted_at = NONE AND is_hidden = false", ids_str);

        let mut res = db.db.query(&query).await?;
        let rows: Vec<MediaRow> = res.take(0)?;

        let mut results: Vec<SearchResult> = rows.into_iter().filter_map(|row| {
            let id_str = record_id_to_string(&row.id);
            let raw    = id_str.strip_prefix("media:").unwrap_or(&id_str);
            let score  = *score_map.get(raw)?;
            Some(row_to_search_result(&row, score, source_dir))
        }).collect();

        results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    /// Apply post-search filters
    pub async fn apply_filters(
        _db: &SurrealDb,
        mut results: Vec<SearchResult>,
        object: Option<&str>,
        face: Option<&str>,
        month: Option<u32>,
        year: Option<i32>,
        media_type: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        if let Some(obj) = object {
            results.retain(|r| r.metadata.objects.iter().any(|o| o.to_lowercase().contains(&obj.to_lowercase())));
        }
        if let Some(f) = face {
            results.retain(|r| r.metadata.faces.iter().any(|n| n.to_lowercase().contains(&f.to_lowercase())));
        }
        if let Some(t) = media_type {
            results.retain(|r| r.media_type == t);
        }
        if month.is_some() || year.is_some() {
            results.retain(|r| {
                if let Some(ref dt_str) = r.metadata.created_at {
                    // Try multiple date formats — SurrealDB Datetime can output various formats
                    let parsed_year_month = parse_year_month_from_str(dt_str);
                    if let Some((y, m)) = parsed_year_month {
                        if let Some(fy) = year {
                            if y != fy { return false; }
                        }
                        if let Some(fm) = month {
                            if m != fm { return false; }
                        }
                        return true;
                    }
                }
                false
            });
        }
        Ok(results)
    }

    // ─── Helpers ──────────────────────────────────────────────────────
}

/// Extract (year, month) from a date string — handles all common formats
fn parse_year_month_from_str(s: &str) -> Option<(i32, u32)> {
    use chrono::Datelike;
    // RFC3339: "2026-03-05T12:45:09+00:00" or "2026-03-05T12:45:09Z"
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some((dt.year(), dt.month()));
    }
    // ISO without tz offset: "2026-03-05T12:45:09"
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some((dt.year(), dt.month()));
    }
    // With fractional seconds: "2026-03-05T12:45:09.123"
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some((dt.year(), dt.month()));
    }
    // Space-separated: "2026-03-05 12:45:09"
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some((dt.year(), dt.month()));
    }
    // Just a date: "2026-03-05"
    if let Ok(dt) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some((dt.year(), dt.month()));
    }
    // Last resort: extract YYYY-MM from the string directly
    if s.len() >= 7 {
        let parts: Vec<&str> = s.split(|c: char| c == '-' || c == '/').collect();
        if parts.len() >= 2 {
            if let (Ok(y), Ok(m)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) {
                if (1900..=2100).contains(&y) && (1..=12).contains(&m) {
                    return Some((y, m));
                }
            }
        }
    }
    None
}

impl DbOperations {
    // ─── Distinct objects (for filter panel) ───────────────────────────

    pub async fn get_distinct_objects(db: &SurrealDb) -> Result<Vec<String>> {
        let mut res = db.db.query(
            "SELECT array::distinct(objects.*.class_name) AS names FROM media WHERE array::len(objects) > 0 AND deleted_at = NONE AND is_hidden = false"
        ).await?;
        #[derive(serde::Deserialize, SurrealValue)]
        struct Row { names: Vec<String> }
        let rows: Vec<Row> = res.take(0)?;
        let mut all: Vec<String> = rows.into_iter().flat_map(|r| r.names).collect();
        all.sort();
        all.dedup();
        Ok(all)
    }

    // ─── People ──────────────────────────────────────────────────────

    pub async fn get_people(db: &SurrealDb, source_dir: &str) -> Result<Vec<PersonGroup>> {
        let mut res = db.db.query(
            "SELECT
                face_id,
                name,
                thumbnail,
                conf,
                face_bbox,
                (SELECT count() FROM media WHERE faces.*.face_id CONTAINS $parent.face_id AND deleted_at = NONE AND is_hidden = false GROUP ALL)[0].count AS photo_count,
                (SELECT file.path FROM media WHERE faces.*.face_id CONTAINS $parent.face_id AND deleted_at = NONE AND is_hidden = false LIMIT 1)[0].file.path AS cover_path
            FROM person
            ORDER BY photo_count DESC"
        ).await?;

        #[derive(serde::Deserialize, SurrealValue)]
        struct PRow {
            face_id: String,
            name: Option<String>,
            thumbnail: Option<String>,
            conf: Option<f32>,
            face_bbox: Option<crate::db::models::Bbox>,
            photo_count: Option<u64>,
            cover_name: Option<String>,
        }
        let rows: Vec<PRow> = res.take(0)?;
        let base = source_dir.trim_end_matches('/');
        Ok(rows.into_iter().map(|r| {
            let cover_path = r.cover_name.as_ref().map(|n| format!("{}/{}", base, n));
            // thumbnail may be stored as full path (legacy) or just filename; try to normalise
            let thumbnail = r.thumbnail.map(|t| {
                if std::path::Path::new(&t).is_absolute() { t }
                else { format!("{}/{}", base, t) }
            });
            PersonGroup {
                face_id: r.face_id,
                name: r.name,
                photo_count: r.photo_count.unwrap_or(0) as u32,
                cover_path,
                thumbnail,
                conf: r.conf,
                face_bbox: r.face_bbox.map(|b| crate::db::models::BboxInfo { x: b.x, y: b.y, w: b.w, h: b.h }),
            }
        }).collect())
    }

    // ─── Duplicates ──────────────────────────────────────────────────

    pub async fn get_duplicates(db: &SurrealDb, source_dir: &str) -> Result<Vec<DuplicateGroup>> {
        let mut groups = vec![];
        let base = source_dir.trim_end_matches('/');

        // 1. Exact Hash Duplicates
        let mut res = db.db.query(
            "SELECT file.sha256 AS sha256, array::group(id) AS ids, count() AS cnt
            FROM media
            WHERE deleted_at = NONE AND is_hidden = false
            GROUP BY file.sha256
            HAVING cnt > 1"
        ).await?;

        #[derive(serde::Deserialize, SurrealValue)]
        struct DupRow {
            sha256: String,
            ids: Vec<RecordId>,
        }
        let rows: Vec<DupRow> = res.take(0)?;

        let mut hash_sets = std::collections::HashSet::new();

        for row in rows {
            let ids_str = row.ids.iter().map(|id| record_id_to_string(id)).collect::<Vec<_>>().join(", ");
            let query = format!("SELECT id, file.path, file.size FROM media WHERE id IN [{}] AND deleted_at = NONE AND is_hidden = false", ids_str);
            let mut r2 = db.db.query(&query).await?;
            #[derive(serde::Deserialize, SurrealValue)]
            struct DI { id: RecordId, name: Option<String>, size: Option<u64> }
            let items: Vec<DI> = r2.take(0)?;
            
            if items.len() < 2 { continue; } // safe-guard

            let mut sorted_ids: Vec<String> = items.iter().map(|i| record_id_to_string(&i.id)).collect();
            sorted_ids.sort();
            hash_sets.insert(sorted_ids.join(","));

            groups.push(DuplicateGroup {
                group_id: row.sha256.clone(),
                reason: "Trùng mã Hash (Khớp dữ liệu nhị phân chính xác 100%)".to_string(),
                items: items.into_iter().map(|i| DuplicateItem {
                    media_id: record_id_to_string(&i.id),
                    file_path: i.name.map(|n| format!("{}/{}", base, n)).unwrap_or_default(),
                    size: i.size.unwrap_or(0),
                }).collect(),
            });
        }

        // 2. Vector Cosine Similarity >= 0.95 Duplicates
        // Note: fetch vectors belonging only to media that is not deleted/hidden
        let mut res2 = db.db.query(
            "SELECT media_id, vec FROM embedding WHERE media_id.deleted_at = NONE AND media_id.is_hidden = false"
        ).await?;
        #[derive(serde::Deserialize, SurrealValue)]
        struct EmbRow { media_id: RecordId, vec: Vec<f32> }
        let emb_rows: Vec<EmbRow> = res2.take(0)?;
        
        if emb_rows.len() > 1 {
            use rayon::prelude::*;
            let threshold = 0.95_f32;
            
            fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
                let mut dot: f32 = 0.0;
                let mut norm_a: f32 = 0.0;
                let mut norm_b: f32 = 0.0;
                for (x, y) in a.iter().zip(b.iter()) {
                    dot += x * y;
                    norm_a += x * x;
                    norm_b += y * y;
                }
                if norm_a == 0.0 || norm_b == 0.0 { return 0.0; }
                dot / (norm_a.sqrt() * norm_b.sqrt())
            }

            // Find all matching pairs
            let pairs: Vec<(usize, usize)> = (0..emb_rows.len()).into_par_iter().flat_map(|i| {
                let mut local_pairs = vec![];
                for j in (i+1)..emb_rows.len() {
                    // Do not cluster same media_id
                    if emb_rows[i].media_id == emb_rows[j].media_id {
                        continue;
                    }
                    if cosine_similarity(&emb_rows[i].vec, &emb_rows[j].vec) >= threshold {
                        local_pairs.push((i, j));
                    }
                }
                local_pairs
            }).collect();

            if !pairs.is_empty() {
                // Disjoint Set Union
                let mut parent: Vec<usize> = (0..emb_rows.len()).collect();
                fn find(p: &mut Vec<usize>, i: usize) -> usize {
                    if p[i] == i { i } else {
                        let r = find(p, p[i]);
                        p[i] = r;
                        r
                    }
                }
                for (i, j) in pairs {
                    let ri = find(&mut parent, i);
                    let rj = find(&mut parent, j);
                    if ri != rj { parent[ri] = rj; }
                }

                let mut clusters: std::collections::HashMap<usize, std::collections::HashSet<RecordId>> = std::collections::HashMap::new();
                for i in 0..emb_rows.len() {
                    let r = find(&mut parent, i);
                    clusters.entry(r).or_default().insert(emb_rows[i].media_id.clone());
                }

                let mut cluster_idx = 0;
                for cluster in clusters.into_values() {
                    if cluster.len() < 2 { continue; }
                    
                    let ids_str = cluster.iter().map(|id| record_id_to_string(id)).collect::<Vec<_>>().join(", ");
                    let query = format!("SELECT id, file.path, file.size FROM media WHERE id IN [{}]", ids_str);
                    let mut r3 = db.db.query(&query).await?;
                    #[derive(serde::Deserialize, SurrealValue)]
                    struct DI { id: RecordId, name: Option<String>, size: Option<u64> }
                    let items: Vec<DI> = r3.take(0)?;

                    if items.len() < 2 { continue; }

                    let mut sorted_ids: Vec<String> = items.iter().map(|i| record_id_to_string(&i.id)).collect();
                    sorted_ids.sort();
                    
                    // Skip if exactly identical to some hash duplicate group to avoid redundant groups showing
                    if hash_sets.contains(&sorted_ids.join(",")) {
                        continue;
                    }

                    groups.push(DuplicateGroup {
                        group_id: format!("sim_{}_{cluster_idx}", record_id_to_string(cluster.iter().next().unwrap())),
                        reason: "Khung hình tương tự nhau (AI phát hiện giống > 95%)".to_string(),
                        items: items.into_iter().map(|i| DuplicateItem {
                            media_id: record_id_to_string(&i.id),
                            file_path: i.name.map(|n| format!("{}/{}", base, n)).unwrap_or_default(),
                            size: i.size.unwrap_or(0),
                        }).collect(),
                    });
                    cluster_idx += 1;
                }
            }
        }

        Ok(groups)
    }

    // ─── Search History ──────────────────────────────────────────────

    pub async fn save_search_history(
        db: &SurrealDb,
        query: Option<String>,
        image_path: Option<String>,
        filters: Option<SearchFilters>,
    ) -> Result<()> {
        let _: Option<IdOnly> = db.db
            .create("search_history")
            .content(SearchHistoryDoc { query, image_path, filters })
            .await?;
        Ok(())
    }

    pub async fn get_search_history(db: &SurrealDb, limit: usize) -> Result<Vec<SearchHistoryRow>> {
        let mut res = db.db.query(
            "SELECT * FROM search_history ORDER BY created_at DESC LIMIT $lim"
        )
        .bind(("lim", limit))
        .await?;
        Ok(res.take(0)?)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────

fn parse_ym(dt: &Option<surrealdb::types::Datetime>) -> (i32, u32) {
    if let Some(dt_val) = dt {
        use chrono::Datelike;
        return (dt_val.year(), dt_val.month() as u32);
    }
    (1970, 1)
}

fn format_month_label(year: i32, month: u32) -> String {
    let months = ["Tháng 1", "Tháng 2", "Tháng 3", "Tháng 4", "Tháng 5", "Tháng 6",
                  "Tháng 7", "Tháng 8", "Tháng 9", "Tháng 10", "Tháng 11", "Tháng 12"];
    let m = months.get((month.saturating_sub(1)) as usize).unwrap_or(&"");
    format!("{} {}", m, year)
}
