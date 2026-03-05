/// Image-to-embedding search
use anyhow::Result;
use crate::processor::{AuraSeekEngine, vision::preprocess_aura};
use crate::db::VectorStore;

pub fn encode_image_query(
    engine: &mut AuraSeekEngine,
    image_path: &str,
) -> Result<Vec<f32>> {
    let blob = preprocess_aura(image_path)?;
    engine.aura.encode_image(blob, 256, 256)
}

pub fn search_by_image_embedding(
    vector_store: &VectorStore,
    embedding: &[f32],
    threshold: f32,
    limit: usize,
) -> Vec<(mongodb::bson::oid::ObjectId, f32)> {
    vector_store.search(embedding, threshold, limit)
}
