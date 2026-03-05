/// Text-to-embedding search
use anyhow::Result;
use crate::processor::AuraSeekEngine;
use crate::db::VectorStore;

pub fn encode_text_query(
    engine: &mut AuraSeekEngine,
    text: &str,
) -> Result<Vec<f32>> {
    let max_len = 64;
    let (input_ids, attention_mask) = engine.text_proc.encode(text, max_len);
    let seq_len = input_ids.iter().position(|&id| id == 0).unwrap_or(max_len);
    let seq_len = seq_len.max(1);
    engine.aura.encode_text(input_ids, attention_mask, seq_len)
}

pub fn search_by_text_embedding(
    vector_store: &VectorStore,
    embedding: &[f32],
    threshold: f32,
    limit: usize,
) -> Vec<(mongodb::bson::oid::ObjectId, f32)> {
    vector_store.search(embedding, threshold, limit)
}
