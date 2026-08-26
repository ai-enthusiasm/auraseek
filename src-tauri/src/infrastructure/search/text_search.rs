/// Text-to-embedding search
use anyhow::Result;
use unicode_normalization::UnicodeNormalization;

use crate::infrastructure::ai::AuraSeekEngine;
use qdrant_client::Qdrant;
use crate::infrastructure::database::DbOperations;

/// Tokenize `text` through the engine's BPE tokenizer and encode it with
/// the text tower model.
///
/// Preprocessing applied before tokenization (matching PhoBERT training):
///   1. Unicode NFC normalization  – ensures diacritics are in canonical form
///      so Telex / VNI / copy-paste all tokenize identically.
///   2. Lowercase                  – PhoBERT vocab is lowercase-only.
///   3. Collapse whitespace        – trims and collapses multiple spaces.
pub fn encode_text_query(
    engine: &mut AuraSeekEngine,
    text: &str,
) -> Result<Vec<f32>> {
    // ── 1. Normalize text ────────────────────────────────────────────────────
    let normalized = normalize_text(text);
    crate::log_info!("🔤 Text query: '{}' → normalized: '{}'", text, normalized);

    // ── 2. Tokenize via BPE ──────────────────────────────────────────────────
    let max_len = crate::core::config::AppConfig::global().text_query_max_len;
    let (input_ids, attention_mask) = engine.text_proc.encode(&normalized, max_len);

    let real_len = input_ids.iter()
        .position(|&id| id == engine.text_proc.tokenizer.pad_token_id as i64)
        .unwrap_or(max_len);
    crate::log_info!("🔤 Token ids ({} real / {} max): {:?}", real_len, max_len, &input_ids[..real_len]);

    // ── 3. Encode with text tower ────────────────────────────────────────────
    engine.aura.encode_text(input_ids, attention_mask)
}

/// Normalize a search query before BPE tokenization.
fn normalize_text(text: &str) -> String {
    let nfc: String = text.nfc().collect();
    nfc.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn search_by_text_embedding(
    client: &Qdrant,
    collection: &str,
    embedding: &[f32],
    threshold: f32,
    limit: usize,
) -> Result<Vec<(String, f32)>> {
    DbOperations::vector_search(client, collection, embedding, threshold, limit).await
}
