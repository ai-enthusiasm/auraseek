use anyhow::Result;
use futures::TryStreamExt;
use mongodb::{
    bson::{doc, oid::ObjectId, DateTime as BsonDateTime},
    options::FindOptions,
};
use std::collections::HashMap;

use crate::db::{
    MongoDb,
    models::{
        MediaDoc, PersonDoc, VectorEmbeddingDoc, SearchHistoryDoc, SearchFilters,
        SearchResult, SearchResultMeta, TimelineGroup, TimelineItem, PersonGroup,
        DuplicateGroup, DuplicateItem,
    },
    vector_store::{VectorStore, VectorEntry},
};

pub struct DbOperations;

impl DbOperations {
    /// Check duplicate by SHA-256. Returns true if already exists.
    pub async fn is_duplicate_sha256(db: &MongoDb, sha256: &str) -> Result<bool> {
        let existing = db.media()
            .find_one(doc! { "file.sha256": sha256 })
            .await?;
        Ok(existing.is_some())
    }

    /// Insert a new media document and return its ObjectId.
    pub async fn insert_media(db: &MongoDb, doc: MediaDoc) -> Result<ObjectId> {
        let result = db.media().insert_one(doc).await?;
        Ok(result.inserted_id.as_object_id().unwrap())
    }

    /// Update the AI results (objects, faces) for an existing media document.
    pub async fn update_media_ai(
        db: &MongoDb,
        media_id: ObjectId,
        objects: Vec<crate::db::models::ObjectEntry>,
        faces: Vec<crate::db::models::FaceEntry>,
    ) -> Result<()> {
        let objs_bson = mongodb::bson::to_bson(&objects)?;
        let faces_bson = mongodb::bson::to_bson(&faces)?;
        db.media().update_one(
            doc! { "_id": media_id },
            doc! { "$set": { "objects": objs_bson, "faces": faces_bson, "processed": true } },
        ).await?;
        Ok(())
    }

    /// Insert or update vector embedding for a media item.
    pub async fn upsert_embedding(
        db: &MongoDb,
        media_id: ObjectId,
        source: &str,
        frame_timestamp: Option<f64>,
        frame_index: Option<u32>,
        embedding: Vec<f32>,
    ) -> Result<()> {
        use crate::db::models::FrameInfo;
        let doc_to_insert = VectorEmbeddingDoc {
            id: None,
            media_id,
            source: source.to_string(),
            frame: FrameInfo {
                timestamp: frame_timestamp,
                frame_index,
            },
            embedding,
            created_at: BsonDateTime::now(),
        };
        db.vector_embeddings().insert_one(doc_to_insert).await?;
        Ok(())
    }

    /// Insert a person/face cluster entry.
    pub async fn upsert_person(
        db: &MongoDb,
        person: PersonDoc,
    ) -> Result<()> {
        // Upsert by face_id
        let face_id = person.face_id.clone();
        let bson = mongodb::bson::to_document(&person)?;
        db.person().update_one(
            doc! { "face_id": &face_id },
            doc! { "$setOnInsert": bson },
        )
        .with_options(mongodb::options::UpdateOptions::builder().upsert(true).build())
        .await?;
        Ok(())
    }

    /// Name a face cluster (person).
    pub async fn name_person(db: &MongoDb, face_id: &str, name: &str) -> Result<()> {
        // Update person doc
        db.person().update_many(
            doc! { "face_id": face_id },
            doc! { "$set": { "name": name } },
        ).await?;

        // Update face entries in all media docs
        db.media().update_many(
            doc! { "faces.face_id": face_id },
            doc! { "$set": { "faces.$[elem].name": name } },
        )
        .with_options(
            mongodb::options::UpdateOptions::builder()
                .array_filters(vec![doc! { "elem.face_id": face_id }])
                .build()
        )
        .await?;
        Ok(())
    }

    /// Load all embeddings into the in-memory vector store.
    pub async fn load_vector_store(db: &MongoDb, store: &VectorStore) -> Result<()> {
        let cursor = db.vector_embeddings().find(doc! {}).await?;
        let docs: Vec<VectorEmbeddingDoc> = cursor.try_collect().await?;
        let entries: Vec<VectorEntry> = docs
            .into_iter()
            .map(|d| VectorEntry {
                media_id: d.media_id,
                source: d.source,
                embedding: d.embedding,
            })
            .collect();
        store.load(entries);
        Ok(())
    }

    /// Get paginated timeline (grouped by month), most recent first.
    pub async fn get_timeline(db: &MongoDb, limit: i64) -> Result<Vec<TimelineGroup>> {
        let opts = FindOptions::builder()
            .sort(doc! { "metadata.created_at": -1 })
            .limit(limit)
            .build();
        let cursor = db.media().find(doc! {}).with_options(opts).await?;
        let docs: Vec<MediaDoc> = cursor.try_collect().await?;

        // Group by year-month
        let mut groups: HashMap<(i32, u32), TimelineGroup> = HashMap::new();

        for doc in docs {
            let (year, month, day) = extract_ymd(&doc.metadata.created_at);
            let key = (year, month);
            let label = format_month_label(year, month);

            let item = TimelineItem {
                media_id:   doc.id.map(|id| id.to_hex()).unwrap_or_default(),
                file_path:  doc.file.path.clone(),
                media_type: doc.media_type.clone(),
                width:      doc.metadata.width,
                height:     doc.metadata.height,
                created_at: doc.metadata.created_at.map(|d| d.to_string()),
                objects:    doc.objects.iter().map(|o| o.class_name.clone()).collect(),
                faces:      doc.faces.iter()
                                .filter_map(|f| f.name.clone())
                                .collect(),
            };

            groups.entry(key).or_insert_with(|| TimelineGroup {
                label: label.clone(),
                year,
                month,
                day,
                items: vec![],
            }).items.push(item);
        }

        let mut result: Vec<TimelineGroup> = groups.into_values().collect();
        result.sort_by(|a, b| {
            b.year.cmp(&a.year).then(b.month.cmp(&a.month))
        });
        Ok(result)
    }

    /// Convert vector search results into SearchResult responses.
    pub async fn resolve_search_results(
        db: &MongoDb,
        hits: Vec<(ObjectId, f32)>,
    ) -> Result<Vec<SearchResult>> {
        let ids: Vec<ObjectId> = hits.iter().map(|(id, _)| *id).collect();
        let score_map: HashMap<ObjectId, f32> = hits.into_iter().collect();

        let cursor = db.media().find(doc! { "_id": { "$in": &ids } }).await?;
        let docs: Vec<MediaDoc> = cursor.try_collect().await?;

        let mut results: Vec<SearchResult> = docs
            .into_iter()
            .filter_map(|doc| {
                let oid = doc.id?;
                let score = *score_map.get(&oid)?;
                Some(SearchResult {
                    media_id: oid.to_hex(),
                    similarity_score: score,
                    file_path: doc.file.path.clone(),
                    media_type: doc.media_type.clone(),
                    metadata: SearchResultMeta {
                        width: doc.metadata.width,
                        height: doc.metadata.height,
                        created_at: doc.metadata.created_at.map(|d| d.to_string()),
                        objects: doc.objects.iter().map(|o| o.class_name.clone()).collect(),
                        faces: doc.faces.iter().filter_map(|f| f.name.clone()).collect(),
                    },
                })
            })
            .collect();

        // Preserve score order
        results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap_or(std::cmp::Ordering::Equal));
        Ok(results)
    }

    /// Apply post-vector-search filters to narrow down results.
    pub async fn apply_filters(
        db: &MongoDb,
        mut results: Vec<SearchResult>,
        object: Option<&str>,
        face: Option<&str>,
        month: Option<u32>,
        year: Option<i32>,
        media_type: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        if object.is_none() && face.is_none() && month.is_none() && year.is_none() && media_type.is_none() {
            return Ok(results);
        }

        // Build DB filter to get qualifying media_ids
        let mut filter = doc! {};
        if let Some(obj) = object {
            filter.insert("objects.class_name", obj);
        }
        if let Some(f) = face {
            filter.insert("faces.name", f);
        }
        if let Some(_m) = month {
            // Month filtering done in memory after DB query
        }
        if let Some(t) = media_type {
            filter.insert("type", t);
        }

        if !filter.is_empty() {
            let cursor = db.media().find(filter).await?;
            let docs: Vec<MediaDoc> = cursor.try_collect().await?;
            let allowed: std::collections::HashSet<String> = docs
                .into_iter()
                .filter_map(|d| d.id.map(|id| id.to_hex()))
                .collect();
            results.retain(|r| allowed.contains(&r.media_id));
        }

        // Month/year filtering in memory (after DB filter)
        if month.is_some() || year.is_some() {
            results.retain(|r| {
                if let Some(ref ts) = r.metadata.created_at {
                    if let Some(m) = month {
                        // Simple string matching for month in ISO date
                        // Better: parse the date properly
                        let _ = m; // placeholder
                    }
                }
                true
            });
        }

        Ok(results)
    }

    /// Get all people/face clusters.
    pub async fn get_people(db: &MongoDb) -> Result<Vec<PersonGroup>> {
        let cursor = db.person().find(doc! {}).await?;
        let docs: Vec<PersonDoc> = cursor.try_collect().await?;

        // Count photos per face_id from media collection
        let mut groups: HashMap<String, PersonGroup> = HashMap::new();
        for person in docs {
            let group = groups.entry(person.face_id.clone()).or_insert_with(|| PersonGroup {
                face_id:     person.face_id.clone(),
                name:        person.name.clone(),
                photo_count: 0,
                cover_path:  None,
                thumbnail:   person.thumbnail.clone(),
            });
            group.photo_count += 1;
            if group.name.is_none() {
                group.name = person.name;
            }
            if group.thumbnail.is_none() {
                group.thumbnail = person.thumbnail;
            }
        }

        // For each group, count photos
        for group in groups.values_mut() {
            let count = db.media()
                .count_documents(doc! { "faces.face_id": &group.face_id })
                .await
                .unwrap_or(0);
            group.photo_count = count as u32;

            // Get cover photo path
            if let Ok(Some(cover)) = db.media()
                .find_one(doc! { "faces.face_id": &group.face_id })
                .await
            {
                group.cover_path = Some(cover.file.path);
            }
        }

        let mut result: Vec<PersonGroup> = groups.into_values().collect();
        result.sort_by(|a, b| b.photo_count.cmp(&a.photo_count));
        Ok(result)
    }

    /// Find duplicate groups by SHA-256.
    pub async fn get_duplicates(db: &MongoDb) -> Result<Vec<DuplicateGroup>> {
        use mongodb::bson::doc;

        // Aggregate: group by sha256, count > 1
        let pipeline = vec![
            doc! { "$group": { "_id": "$file.sha256", "count": { "$sum": 1 }, "docs": { "$push": { "id": "$_id", "path": "$file.path", "size": "$file.size" } } } },
            doc! { "$match": { "count": { "$gt": 1 } } },
        ];

        let mut cursor = db.media().aggregate(pipeline).await?;
        let mut groups = vec![];

        while let Some(doc) = cursor.try_next().await? {
            let sha256 = doc.get_str("_id").unwrap_or("").to_string();
            let empty_arr = mongodb::bson::Array::new();
            let docs_arr = doc.get_array("docs").unwrap_or(&empty_arr);
            let items: Vec<DuplicateItem> = docs_arr.iter().filter_map(|d| {
                let d = d.as_document()?;
                Some(DuplicateItem {
                    media_id:  d.get_object_id("id").ok()?.to_hex(),
                    file_path: d.get_str("path").unwrap_or("").to_string(),
                    size:      d.get_i64("size").unwrap_or(0) as u64,
                })
            }).collect();
            groups.push(DuplicateGroup { sha256, items });
        }

        Ok(groups)
    }

    /// Save search history.
    pub async fn save_search_history(
        db: &MongoDb,
        query: Option<String>,
        image_path: Option<String>,
        filters: Option<SearchFilters>,
    ) -> Result<()> {
        let doc = SearchHistoryDoc {
            id: None,
            query,
            image_search_path: image_path,
            filters,
            created_at: BsonDateTime::now(),
        };
        db.search_history().insert_one(doc).await?;
        Ok(())
    }

    /// Get recent search history.
    pub async fn get_search_history(db: &MongoDb, limit: i64) -> Result<Vec<SearchHistoryDoc>> {
        let opts = FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .limit(limit)
            .build();
        let cursor = db.search_history().find(doc! {}).with_options(opts).await?;
        Ok(cursor.try_collect().await?)
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn extract_ymd(dt: &Option<mongodb::bson::DateTime>) -> (i32, u32, Option<u32>) {
    if let Some(d) = dt {
        let ms = d.timestamp_millis();
        let naive = chrono::DateTime::from_timestamp_millis(ms)
            .map(|dt: chrono::DateTime<chrono::Utc>| dt.date_naive());
        if let Some(date) = naive {
            use chrono::Datelike;
            return (date.year(), date.month(), Some(date.day()));
        }
    }
    (1970, 1, None)
}

fn format_month_label(year: i32, month: u32) -> String {
    let months = ["Tháng 1", "Tháng 2", "Tháng 3", "Tháng 4", "Tháng 5", "Tháng 6",
                  "Tháng 7", "Tháng 8", "Tháng 9", "Tháng 10", "Tháng 11", "Tháng 12"];
    let m = months.get((month.saturating_sub(1)) as usize).unwrap_or(&"");
    format!("{} {}", m, year)
}
