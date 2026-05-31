use anyhow::{Context, Result};
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    Condition, Filter, PointStruct,
    SearchPointsBuilder, UpsertPointsBuilder, DeletePointsBuilder,
};
use qdrant_client::Payload;
use super::DbOperations;

impl DbOperations {
    pub async fn insert_embedding(
        client: &Qdrant,
        collection: &str,
        media_id: &str,
        source: &str,
        frame_ts: Option<f64>,
        frame_idx: Option<u32>,
        embedding: Vec<f32>,
    ) -> Result<()> {
        let point_id = uuid::Uuid::new_v4().to_string();
        let mut payload = Payload::new();
        payload.insert("media_id", media_id.to_string());
        payload.insert("source", source.to_string());
        if let Some(ts) = frame_ts {
            payload.insert("frame_ts", ts);
        }
        if let Some(idx) = frame_idx {
            payload.insert("frame_idx", idx as i64);
        }

        let point = PointStruct::new(point_id, embedding, payload);
        let res = client.upsert_points(
            UpsertPointsBuilder::new(collection, vec![point.clone()]).wait(true)
        ).await;

        if let Err(ref e) = res {
            let err_str = e.to_string();
            if err_str.contains("Service runtime error")
                || err_str.contains("Not recovered from previous error")
                || err_str.contains("failed to open file")
            {
                crate::log_warn!(
                    "⚠️ Qdrant collection '{}' seems corrupted or degraded. Recreating collection and retrying write... Error: {}",
                    collection,
                    err_str
                );
                let _ = client.delete_collection(collection).await;
                if let Err(recreate_err) = crate::infrastructure::database::QdrantService::ensure_collection(client, collection, 384).await {
                    crate::log_error!("❌ Failed to recreate collection '{}' during recovery: {}", collection, recreate_err);
                } else {
                    client.upsert_points(
                        UpsertPointsBuilder::new(collection, vec![point]).wait(true)
                    ).await.context("insert_embedding: retry upsert failed")?;
                    return Ok(());
                }
            }
        }

        res.context("insert_embedding: upsert failed")?;
        Ok(())
    }

    pub async fn delete_embeddings_for_media(
        client: &Qdrant,
        collection: &str,
        media_id: &str,
    ) -> Result<()> {
        let filter = Filter::must([
            Condition::matches("media_id", media_id.to_string()),
        ]);
        client.delete_points(
            DeletePointsBuilder::new(collection).points(filter).wait(true)
        ).await.context("delete_embeddings_for_media failed")?;
        Ok(())
    }

    pub async fn vector_search(
        client: &Qdrant,
        collection: &str,
        query_vec: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<(String, f32)>> {
        let results = client.search_points(
            SearchPointsBuilder::new(collection, query_vec.to_vec(), limit as u64)
                .score_threshold(threshold)
                .with_payload(true)
        ).await.context("vector_search failed")?;

        let hits: Vec<(String, f32)> = results.result.into_iter().filter_map(|p| {
            let media_id = p.payload.get("media_id")?
                .as_str()
                .map(|s| s.to_string())?;
            Some((media_id, p.score))
        }).collect();

        Ok(hits)
    }

    pub async fn embedding_count(client: &Qdrant, collection: &str) -> Result<u64> {
        let info = client.collection_info(collection).await
            .context("embedding_count: collection_info failed")?;
        Ok(info.result
            .map(|r| r.points_count.unwrap_or(0))
            .unwrap_or(0))
    }

    pub async fn clear_qdrant_collection(client: &Qdrant, collection: &str) -> Result<()> {
        client.delete_collection(collection).await
            .context("clear_qdrant_collection: delete failed")?;
        crate::log_info!("🧹 Qdrant collection '{}' deleted", collection);
        Ok(())
    }

    pub async fn insert_face_embedding(
        client: &Qdrant,
        collection: &str,
        media_id: &str,
        face_id: &str,
        bbox: [f32; 4],
        embedding: Vec<f32>,
    ) -> Result<()> {
        let point_id = uuid::Uuid::new_v4().to_string();
        let payload_json = serde_json::json!({
            "media_id": media_id,
            "face_id": face_id,
            "bbox": bbox.to_vec(),
            "source": "face"
        });
        let payload = Payload::try_from(payload_json)
            .map_err(|e| anyhow::anyhow!("failed to create payload: {}", e))?;

        let point = PointStruct::new(point_id, embedding, payload);
        let res = client.upsert_points(
            UpsertPointsBuilder::new(collection, vec![point.clone()]).wait(true)
        ).await;

        if let Err(ref e) = res {
            let err_str = e.to_string();
            if err_str.contains("Service runtime error")
                || err_str.contains("Not recovered from previous error")
                || err_str.contains("failed to open file")
            {
                crate::log_warn!(
                    "⚠️ Qdrant face collection '{}' seems corrupted or degraded. Recreating collection and retrying write... Error: {}",
                    collection,
                    err_str
                );
                let _ = client.delete_collection(collection).await;
                if let Err(recreate_err) = crate::infrastructure::database::QdrantService::ensure_collection(client, collection, 512).await {
                    crate::log_error!("❌ Failed to recreate face collection '{}' during recovery: {}", collection, recreate_err);
                } else {
                    client.upsert_points(
                        UpsertPointsBuilder::new(collection, vec![point]).wait(true)
                    ).await.context("insert_face_embedding: retry upsert failed")?;
                    return Ok(());
                }
            }
        }

        res.context("insert_face_embedding: upsert failed")?;
        Ok(())
    }

    pub async fn delete_face_embeddings_for_media(
        client: &Qdrant,
        collection: &str,
        media_id: &str,
    ) -> Result<()> {
        let filter = Filter::must([
            Condition::matches("media_id", media_id.to_string()),
            Condition::matches("source", "face".to_string()),
        ]);
        client.delete_points(
            DeletePointsBuilder::new(collection).points(filter).wait(true)
        ).await.context("delete_face_embeddings_for_media failed")?;
        Ok(())
    }

    pub async fn vector_search_face(
        client: &Qdrant,
        collection: &str,
        query_vec: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Result<Vec<(String, f32)>> {
        let results = client.search_points(
            SearchPointsBuilder::new(collection, query_vec.to_vec(), limit as u64)
                .score_threshold(threshold)
                .with_payload(true)
        ).await.context("vector_search_face failed")?;

        let hits: Vec<(String, f32)> = results.result.into_iter().filter_map(|p| {
            let face_id = p.payload.get("face_id")?
                .as_str()
                .map(|s| s.to_string())?;
            Some((face_id, p.score))
        }).collect();

        Ok(hits)
    }
}
