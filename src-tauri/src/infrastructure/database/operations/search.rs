use anyhow::Result;
use rusqlite::params;
use std::collections::HashMap;
use crate::infrastructure::database::SqliteDb;
use crate::infrastructure::database::models::SearchFilters;
use crate::core::models::SearchResult;
use super::{DbOperations, read_media_row, row_to_search_result, parse_year_month_from_str};
use crate::infrastructure::database::models::SearchHistoryRow;

impl DbOperations {
    pub fn resolve_search_results(
        db: &SqliteDb,
        hits: Vec<(String, f32)>,
        source_dir: &str,
    ) -> Result<Vec<SearchResult>> {
        if hits.is_empty() { return Ok(vec![]); }

        let mut score_map: HashMap<String, f32> = HashMap::new();
        for (mid, score) in &hits {
            let entry = score_map.entry(mid.clone()).or_insert(0.0);
            if *score > *entry { *entry = *score; }
        }

        let conn = db.conn();
        let mut results: Vec<SearchResult> = Vec::new();
        for (media_id, score) in &score_map {
            if let Some(row) = read_media_row(&conn, media_id)? {
                if row.deleted_at.is_some() || row.is_hidden { continue; }
                results.push(row_to_search_result(&row, *score, source_dir));
            }
        }

        results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    pub fn apply_filters(
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
            let f_lower = f.to_lowercase();
            results.retain(|r| {
                r.metadata.faces.iter().any(|n| n.to_lowercase().contains(&f_lower))
                    || r.detected_faces.iter().any(|face| {
                        face.face_id.eq_ignore_ascii_case(f)
                            || face.name
                                .as_ref()
                                .map(|name| name.to_lowercase().contains(&f_lower))
                                .unwrap_or(false)
                    })
            });
        }
        if let Some(t) = media_type {
            let normalized = if t == "photo" { "image" } else { t };
            results.retain(|r| r.media_type == normalized);
        }
        if month.is_some() || year.is_some() {
            results.retain(|r| {
                if let Some(ref dt_str) = r.metadata.created_at {
                    if let Some((y, m)) = parse_year_month_from_str(dt_str) {
                        if let Some(fy) = year { if y != fy { return false; } }
                        if let Some(fm) = month { if m != fm { return false; } }
                        return true;
                    }
                }
                false
            });
        }
        Ok(results)
    }

    pub fn save_search_history(
        db: &SqliteDb,
        query: Option<String>,
        image_path: Option<String>,
        filters: Option<SearchFilters>,
    ) -> Result<()> {
        let conn = db.conn();
        let (f_obj, f_face, f_month, f_year, f_mt) = match filters {
            Some(f) => (f.object, f.face, f.month, f.year, f.media_type),
            None => (None, None, None, None, None),
        };
        conn.execute(
            "INSERT INTO search_history (query, image_path, filter_object, filter_face, filter_month, filter_year, filter_media_type)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![query, image_path, f_obj, f_face, f_month, f_year, f_mt],
        )?;
        Ok(())
    }

    pub fn get_search_history(db: &SqliteDb, limit: usize) -> Result<Vec<SearchHistoryRow>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, query, image_path, created_at FROM search_history
             WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(SearchHistoryRow {
                id:         r.get(0)?,
                query:      r.get(1)?,
                image_path: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?.filter_map(|r| r.ok()).collect();
        Ok(rows)
    }
}
