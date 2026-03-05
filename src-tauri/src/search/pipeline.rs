/// Search orchestration pipeline: text, image, or combined text+image search.
/// Combined mode: run both in parallel, merge by media_id (intersection).
use anyhow::Result;
use std::collections::HashMap;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

use crate::db::{MongoDb, VectorStore, DbOperations};
use crate::db::models::{SearchResult, SearchFilters};
use crate::processor::AuraSeekEngine;
use crate::search::text_search::{encode_text_query, search_by_text_embedding};
use crate::search::image_search::{encode_image_query, search_by_image_embedding};

const SIMILARITY_THRESHOLD: f32 = 0.2;
const RESULT_LIMIT: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchMode {
    Text,
    Image,
    Combined, // text AND image (intersection)
    ObjectFilter,
    FaceFilter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub mode:       SearchMode,
    pub text:       Option<String>,
    pub image_path: Option<String>,
    pub filters:    SearchQueryFilters,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchQueryFilters {
    pub object:     Option<String>,
    pub face:       Option<String>,
    pub month:      Option<u32>,
    pub year:       Option<i32>,
    pub media_type: Option<String>,
}

pub struct SearchPipeline;

impl SearchPipeline {
    pub async fn run(
        query: &SearchQuery,
        engine: &mut AuraSeekEngine,
        vector_store: &VectorStore,
        db: &MongoDb,
    ) -> Result<Vec<SearchResult>> {
        let results = match &query.mode {
            SearchMode::Text => {
                let text = query.text.as_deref().unwrap_or("");
                match encode_text_query(engine, text) {
                    Ok(embedding) => {
                        let hits = search_by_text_embedding(vector_store, &embedding, SIMILARITY_THRESHOLD, RESULT_LIMIT);
                        DbOperations::resolve_search_results(db, hits).await?
                    }
                    Err(e) => {
                        eprintln!("Text encoding failed: {}", e);
                        vec![]
                    }
                }
            }

            SearchMode::Image => {
                let image_path = query.image_path.as_deref().unwrap_or("");
                match encode_image_query(engine, image_path) {
                    Ok(embedding) => {
                        let hits = search_by_image_embedding(vector_store, &embedding, SIMILARITY_THRESHOLD, RESULT_LIMIT);
                        DbOperations::resolve_search_results(db, hits).await?
                    }
                    Err(e) => {
                        eprintln!("Image encoding failed: {}", e);
                        vec![]
                    }
                }
            }

            SearchMode::Combined => {
                // Run both encodings, intersect results
                let text = query.text.as_deref().unwrap_or("");
                let image_path = query.image_path.as_deref().unwrap_or("");

                let text_hits = encode_text_query(engine, text)
                    .map(|emb| search_by_text_embedding(vector_store, &emb, SIMILARITY_THRESHOLD, RESULT_LIMIT))
                    .unwrap_or_default();

                let image_hits = encode_image_query(engine, image_path)
                    .map(|emb| search_by_image_embedding(vector_store, &emb, SIMILARITY_THRESHOLD, RESULT_LIMIT))
                    .unwrap_or_default();

                // Merge: take items present in BOTH, use max score
                let text_map: HashMap<ObjectId, f32> = text_hits.into_iter().collect();
                let image_map: HashMap<ObjectId, f32> = image_hits.into_iter().collect();

                let merged: Vec<(ObjectId, f32)> = text_map.iter()
                    .filter_map(|(id, t_score)| {
                        image_map.get(id).map(|i_score| {
                            (*id, (t_score + i_score) / 2.0) // average score
                        })
                    })
                    .collect();

                DbOperations::resolve_search_results(db, merged).await?
            }

            SearchMode::ObjectFilter => {
                // Direct MongoDB query by object class name
                let class = query.filters.object.as_deref().unwrap_or("");
                Self::object_search(db, class).await?
            }

            SearchMode::FaceFilter => {
                // Direct MongoDB query by face name
                let name = query.filters.face.as_deref().unwrap_or("");
                Self::face_search(db, name).await?
            }
        };

        // Apply post-filters (object, face, month, year, media_type)
        let filters = &query.filters;
        DbOperations::apply_filters(
            db,
            results,
            filters.object.as_deref(),
            filters.face.as_deref(),
            filters.month,
            filters.year,
            filters.media_type.as_deref(),
        ).await
    }

    /// Search by object class name directly in MongoDB.
    async fn object_search(db: &MongoDb, class_name: &str) -> Result<Vec<SearchResult>> {
        use futures::TryStreamExt;
        use mongodb::bson::doc;
        use crate::db::models::{MediaDoc, SearchResultMeta};

        let cursor = db.media()
            .find(doc! { "objects.class_name": class_name })
            .await?;
        let docs: Vec<MediaDoc> = cursor.try_collect().await?;

        Ok(docs.into_iter().filter_map(|doc| {
            let oid = doc.id?;
            Some(SearchResult {
                media_id: oid.to_hex(),
                similarity_score: 1.0,
                file_path: doc.file.path,
                media_type: doc.media_type,
                metadata: SearchResultMeta {
                    width: doc.metadata.width,
                    height: doc.metadata.height,
                    created_at: doc.metadata.created_at.map(|d| d.to_string()),
                    objects: doc.objects.iter().map(|o| o.class_name.clone()).collect(),
                    faces: doc.faces.iter().filter_map(|f| f.name.clone()).collect(),
                },
            })
        }).collect())
    }

    /// Search by face name directly in MongoDB.
    async fn face_search(db: &MongoDb, name: &str) -> Result<Vec<SearchResult>> {
        use futures::TryStreamExt;
        use mongodb::bson::doc;
        use crate::db::models::{MediaDoc, SearchResultMeta};

        let cursor = db.media()
            .find(doc! { "faces.name": name })
            .await?;
        let docs: Vec<MediaDoc> = cursor.try_collect().await?;

        Ok(docs.into_iter().filter_map(|doc| {
            let oid = doc.id?;
            Some(SearchResult {
                media_id: oid.to_hex(),
                similarity_score: 1.0,
                file_path: doc.file.path,
                media_type: doc.media_type,
                metadata: SearchResultMeta {
                    width: doc.metadata.width,
                    height: doc.metadata.height,
                    created_at: doc.metadata.created_at.map(|d| d.to_string()),
                    objects: doc.objects.iter().map(|o| o.class_name.clone()).collect(),
                    faces: doc.faces.iter().filter_map(|f| f.name.clone()).collect(),
                },
            })
        }).collect())
    }
}
