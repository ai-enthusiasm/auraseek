/// Search pipeline orchestration – SQLite + Qdrant edition
use anyhow::Result;
use std::collections::HashMap;use crate::core::models::{SearchMode, SearchQuery, SearchResult, SearchQueryFilters};
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

            SearchMode::ObjectFilter | SearchMode::FaceFilter | SearchMode::FilterOnly => {
                crate::log_info!("🔍 [SearchPipeline::run] Unified Filter: {:?}", query.filters);
                Self::query_by_filters_sql(sqlite, &query.filters, source_dir)
            }
        }
    }

    /// Unified filter query using dynamic SQL to properly intersect all selected filter options
    fn query_by_filters_sql(
        sqlite: &std::sync::Mutex<Option<SqliteDb>>,
        filters: &SearchQueryFilters,
        source_dir: &str,
    ) -> Result<Vec<SearchResult>> {
        let guard = sqlite.lock().unwrap();
        let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not initialized"))?;
        let conn = db.conn();

        let mut query_parts = vec![];
        let mut joins = vec![];
        let mut params: Vec<String> = vec![];
        let mut param_index = 1;

        let mut select = "SELECT DISTINCT m.* FROM media m".to_string();

        if let Some(ref objs) = filters.objects {
            for (i, obj) in objs.iter().enumerate() {
                if !obj.is_empty() {
                    let mo_alias = format!("mo{}", i);
                    let oc_alias = format!("oc{}", i);
                    joins.push(format!("JOIN media_objects {0} ON {0}.media_id = m.id", mo_alias));
                    joins.push(format!("JOIN object_class {0} ON {1}.class_id = {0}.id", oc_alias, mo_alias));
                    query_parts.push(format!("{}.name = ?{}", oc_alias, param_index));
                    params.push(obj.clone());
                    param_index += 1;
                }
            }
        }

        if let Some(ref face) = filters.face {
            if !face.is_empty() {
                joins.push("JOIN media_faces mf ON mf.media_id = m.id".to_string());
                joins.push("LEFT JOIN person p ON mf.person_id = p.id".to_string());
                query_parts.push(format!(
                    "(p.face_id = ?{0} OR mf.name = ?{0} OR p.name = ?{0})",
                    param_index
                ));
                params.push(face.clone());
                param_index += 1;
            }
        }

        if let Some(ref mt) = filters.media_type {
            if !mt.is_empty() {
                let normalized = if mt == "photo" { "image" } else { mt.as_str() };
                query_parts.push(format!("m.media_type = ?{}", param_index));
                params.push(normalized.to_string());
                param_index += 1;
            }
        }

        query_parts.push("m.deleted_at IS NULL".to_string());
        query_parts.push("m.is_hidden = 0".to_string());

        if !joins.is_empty() {
            select = format!("{} {}", select, joins.join(" "));
        }

        let limit = crate::core::config::AppConfig::global().search_limit.min(2000);
        let where_clause = query_parts.join(" AND ");
        let sql = format!(
            "{} WHERE {} ORDER BY m.meta_created_at DESC LIMIT {}",
            select, where_clause, limit
        );

        crate::log_info!("🔍 Executing Filter SQL: {} with params: {:?}", sql, params);

        let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        let rows = read_media_rows_from_query(&conn, &sql, &params_ref)?;
        drop(conn);

        let mut results = rows.iter()
            .map(|row| row_to_search_result(row, 1.0, source_dir))
            .collect::<Vec<_>>();

        if filters.month.is_some() || filters.year.is_some() {
            results = DbOperations::apply_filters(
                results, None, None,
                filters.month, filters.year,
                None,
            )?;
        }

        Ok(results)
    }

    /// Sync helper: resolve vector search hits into SearchResults via SQLite,
    /// then apply filters. Called after all async (qdrant) work is done.
    fn resolve_and_filter(
        sqlite: &std::sync::Mutex<Option<SqliteDb>>,
        raw_hits: Vec<(String, f32)>,
        query: &SearchQuery,
        source_dir: &str,
    ) -> Result<Vec<SearchResult>> {
        crate::log_info!(
            "🔍 [resolve_and_filter] raw_hits={} filters={:?}",
            raw_hits.len(),
            query.filters
        );
        let guard = sqlite.lock().unwrap();
        let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not initialized"))?;

        let mut results = DbOperations::resolve_search_results(db, raw_hits, source_dir)?;
        crate::log_info!("🔍 [resolve_and_filter] resolved_results={}", results.len());

        results = DbOperations::apply_filters(
            results,
            query.filters.objects.as_deref(),
            query.filters.face.as_deref(),
            query.filters.month,
            query.filters.year,
            query.filters.media_type.as_deref(),
        )?;
        crate::log_info!("🔍 [resolve_and_filter] filtered_results={}", results.len());

        Ok(results)
    }
}
