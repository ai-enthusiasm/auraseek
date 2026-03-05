/// In-memory vector index for fast cosine similarity search over embeddings.
/// Optimized for ≤100k images (100k × 384 f32 ≈ 148 MB, well within <200ms search).
use std::sync::RwLock;
use mongodb::bson::oid::ObjectId;

pub struct VectorEntry {
    pub media_id:    ObjectId,
    pub source:      String,
    pub embedding:   Vec<f32>,
}

pub struct VectorStore {
    entries: RwLock<Vec<VectorEntry>>,
}

impl VectorStore {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
        }
    }

    /// Load all entries into the index (called on startup and after ingest)
    pub fn load(&self, entries: Vec<VectorEntry>) {
        let mut w = self.entries.write().unwrap();
        *w = entries;
    }

    /// Add a single entry without full reload
    pub fn add(&self, entry: VectorEntry) {
        let mut w = self.entries.write().unwrap();
        w.push(entry);
    }

    /// Search for nearest neighbors above threshold, returns (media_id, score) sorted desc
    pub fn search(
        &self,
        query: &[f32],
        threshold: f32,
        limit: usize,
    ) -> Vec<(ObjectId, f32)> {
        let r = self.entries.read().unwrap();
        let query_norm = l2_norm(query);
        if query_norm < 1e-8 {
            return vec![];
        }

        let mut scored: Vec<(ObjectId, f32)> = r
            .iter()
            .filter_map(|e| {
                let score = cosine_dot_normalized(query, query_norm, &e.embedding);
                if score >= threshold {
                    Some((e.media_id, score))
                } else {
                    None
                }
            })
            .collect();

        // Sort descending by score
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored
    }

    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }
}

fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn cosine_dot_normalized(query: &[f32], query_norm: f32, stored: &[f32]) -> f32 {
    if query.len() != stored.len() { return 0.0; }
    let dot: f32 = query.iter().zip(stored.iter()).map(|(a, b)| a * b).sum();
    let stored_norm = l2_norm(stored);
    dot / (query_norm * stored_norm + 1e-8)
}
