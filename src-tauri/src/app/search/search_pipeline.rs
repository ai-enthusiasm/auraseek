/// Search pipeline orchestration – SQLite + Qdrant edition
use anyhow::Result;
use std::collections::HashMap;

use crate::core::models::{SearchMode, SearchQuery, SearchResult};
use crate::infrastructure::ai::AuraSeekEngine;
use crate::infrastructure::database::SqliteDb;
use crate::infrastructure::database::operations::{DbOperations, read_media_rows_from_query, row_to_search_result};
use crate::infrastructure::search::text_search::{encode_text_query, search_by_text_embedding};
use crate::infrastructure::search::image_search::{encode_image_query, search_by_image_embedding};
use qdrant_client::Qdrant;

pub struct SearchPipeline;

impl SearchPipeline {
    /// `sqlite` is passed as the outer mutex so we can lock/unlock it around
    /// sync SQLite operations without holding the (non-Send) guard across
    /// async `.await` points.
    pub async fn run(
        query: &SearchQuery,
        engine: &mut AuraSeekEngine,
        sqlite: &std::sync::Mutex<Option<SqliteDb>>,
        qdrant: &Qdrant,
        source_dir: &str,
    ) -> Result<Vec<SearchResult>> {
        let config = crate::core::config::AppConfig::global();
        let threshold = config.search_threshold;
        let limit = config.search_limit;
        let collection = &config.qdrant_collection;

        let lo = config.search_sql_limit_object_filter;
        let lf = config.search_sql_limit_face_filter;
        let lfo = config.search_sql_limit_filter_only;

        match query.mode {
            SearchMode::Text => {
                let text = query.text.as_deref().unwrap_or("");
                crate::log_info!("🔍 [SearchPipeline::run] mode=Text text='{}' threshold={}", text, threshold);
                let embedding = encode_text_query(engine, text)?;
                let raw_hits = search_by_text_embedding(qdrant, collection, &embedding, threshold, limit).await?;
                Self::resolve_and_filter(sqlite, raw_hits, query, source_dir)
            }

            SearchMode::Image => {
                let path = query.image_path.as_deref().unwrap_or("");
                crate::log_info!("🔍 [SearchPipeline::run] mode=Image path='{}' threshold={}", path, threshold);
                let embedding = encode_image_query(engine, path)?;
                let raw_hits = search_by_image_embedding(qdrant, collection, &embedding, threshold, limit).await?;
                Self::resolve_and_filter(sqlite, raw_hits, query, source_dir)
            }

            SearchMode::Combined => {
                let text = query.text.as_deref().unwrap_or("");
                let path = query.image_path.as_deref().unwrap_or("");
                crate::log_info!("🔍 [SearchPipeline::run] mode=Combined text='{}' path='{}' threshold={}", text, path, threshold);

                let text_emb  = encode_text_query(engine, text)?;
                let img_emb   = encode_image_query(engine, path)?;
                let text_hits = search_by_text_embedding(qdrant, collection, &text_emb, threshold, limit).await?;
                let img_hits  = search_by_image_embedding(qdrant, collection, &img_emb, threshold, limit).await?;

                crate::log_info!(
                    "🔍 [SearchPipeline::run] combined text_hits={} img_hits={}",
                    text_hits.len(),
                    img_hits.len()
                );

                let text_map: HashMap<String, f32> = text_hits.into_iter().collect();
                let mut combined = vec![];
                for (mid, img_score) in img_hits {
                    if let Some(text_score) = text_map.get(&mid) {
                        combined.push((mid, (img_score + text_score) / 2.0));
                    }
                }
                crate::log_info!(
                    "🔍 [SearchPipeline::run] combined_intersection={}",
                    combined.len()
                );
                combined.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                Self::resolve_and_filter(sqlite, combined, query, source_dir)
            }

            SearchMode::ObjectFilter => {
                let class = query.filters.object.clone().unwrap_or_default();
                crate::log_info!("🔍 ObjectFilter: class_name='{}'", class);

                let results = {
                    let guard = sqlite.lock().unwrap();
                    let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not initialized"))?;
                    let conn = db.conn();
                    let rows = read_media_rows_from_query(
                        &conn,
                        &format!(
                            "SELECT DISTINCT m.*
                         FROM media m JOIN media_objects o ON o.media_id = m.id
                         WHERE o.class_name = ?1 AND m.deleted_at IS NULL AND m.is_hidden = 0
                         ORDER BY m.meta_created_at DESC LIMIT {}",
                            lo
                        ),
                        &[&class as &dyn rusqlite::ToSql],
                    )?;
                    drop(conn);
                    rows.iter()
                        .map(|row| row_to_search_result(row, 1.0, source_dir))
                        .collect::<Vec<_>>()
                };

                DbOperations::apply_filters(
                    results, None, None,
                    query.filters.month, query.filters.year,
                    query.filters.media_type.as_deref(),
                )
            }

            SearchMode::FaceFilter => {
                let name = query.filters.face.clone().unwrap_or_default();
                crate::log_info!("🔍 FaceFilter: name='{}'", name);

                let results = {
                    let guard = sqlite.lock().unwrap();
                    let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not initialized"))?;
                    let conn = db.conn();
                    let rows = read_media_rows_from_query(
                        &conn,
                        &format!(
                            "SELECT DISTINCT m.*
                         FROM media m JOIN media_faces f ON f.media_id = m.id
                         WHERE (f.face_id = ?1 OR f.name = ?1) AND m.deleted_at IS NULL AND m.is_hidden = 0
                         ORDER BY m.meta_created_at DESC LIMIT {}",
                            lf
                        ),
                        &[&name as &dyn rusqlite::ToSql],
                    )?;
                    drop(conn);
                    rows.iter()
                        .map(|row| row_to_search_result(row, 1.0, source_dir))
                        .collect::<Vec<_>>()
                };

                DbOperations::apply_filters(
                    results, None, None,
                    query.filters.month, query.filters.year,
                    query.filters.media_type.as_deref(),
                )
            }

            SearchMode::FilterOnly => {
                crate::log_info!("🔍 FilterOnly: {:?}", query.filters);

                let results = {
                    let guard = sqlite.lock().unwrap();
                    let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not initialized"))?;
                    let conn = db.conn();
                    let rows = read_media_rows_from_query(
                        &conn,
                        &format!(
                            "SELECT * FROM media WHERE deleted_at IS NULL AND is_hidden = 0
                         ORDER BY meta_created_at DESC LIMIT {}",
                            lfo
                        ),
                        &[],
                    )?;
                    drop(conn);
                    rows.iter()
                        .map(|row| row_to_search_result(row, 1.0, source_dir))
                        .collect::<Vec<_>>()
                };

                DbOperations::apply_filters(
                    results,
                    query.filters.object.as_deref(),
                    query.filters.face.as_deref(),
                    query.filters.month, query.filters.year,
                    query.filters.media_type.as_deref(),
                )
            }
        }
    }

    /// Sync helper: resolve vector search hits into SearchResults via SQLite,
    /// then apply filters. Called after all async (qdrant) work is done.
    fn resolve_and_filter(
        sqlite: &std::sync::Mutex<Option<SqliteDb>>,
        raw_hits: Vec<(String, f32)>,
        query: &SearchQuery,
        source_dir: &str,
    ) -> Result<Vec<SearchResult>> {
        let guard = sqlite.lock().unwrap();
        let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not initialized"))?;

        let mut results = DbOperations::resolve_search_results(db, raw_hits, source_dir)?;

        results = DbOperations::apply_filters(
            results,
            query.filters.object.as_deref(),
            query.filters.face.as_deref(),
            query.filters.month,
            query.filters.year,
            query.filters.media_type.as_deref(),
        )?;

        Ok(results)
    }
}

