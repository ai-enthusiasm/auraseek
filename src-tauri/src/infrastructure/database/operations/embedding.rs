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
        client.upsert_points(
            UpsertPointsBuilder::new(collection, vec![point]).wait(true)
        ).await.context("insert_embedding: upsert failed")?;

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
}
