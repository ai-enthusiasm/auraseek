use anyhow::Result;
use crate::infrastructure::database::SqliteDb;
use crate::core::models::{DuplicateGroup, DuplicateItem};
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{vector_output, Condition, Filter, ScrollPointsBuilder, SearchPointsBuilder};
use super::DbOperations;
use std::sync::Arc;

const DUPLICATE_THRESHOLD: f32 = 0.92;

fn hamming(a: u64, b: u64) -> u32 { (a ^ b).count_ones() }

fn find(p: &mut Vec<usize>, i: usize) -> usize {
    if p[i] == i { i } else { let r = find(p, p[i]); p[i] = r; r }
}

fn resolve_video_thumb_fallback(base_dir: &str, file_name: &str, thumb_cache_dir: Option<&std::path::Path>) -> Option<String> {
    let video_abs = std::path::Path::new(base_dir).join(file_name);
    let stem = video_abs.file_stem()?.to_string_lossy();
    let thumb_file = format!("{}.thumb.jpg", stem);
    if let Some(cache_dir) = thumb_cache_dir {
        let p = cache_dir.join(&thumb_file);
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
    }
    let parent = video_abs.parent().unwrap_or(std::path::Path::new(base_dir));
    let p = parent.join(&thumb_file);
    if p.exists() { return Some(p.to_string_lossy().to_string()); }
    None
}

impl DbOperations {
    /// Takes `Arc<Mutex<Option<SqliteDb>>>` so the sync MutexGuard is not held
    /// across the async Qdrant scroll calls.
    pub async fn get_duplicates(
        sqlite: &Arc<std::sync::Mutex<Option<SqliteDb>>>,
        qdrant: &Qdrant,
        collection: &str,
        source_dir: &str,
        media_type: Option<&str>,
        thumb_cache_dir: Option<&std::path::Path>,
    ) -> Result<Vec<DuplicateGroup>> {
        let mut groups: Vec<DuplicateGroup> = vec![];
        let base = source_dir.trim_end_matches('/');
        let mut covered: std::collections::HashSet<String> = std::collections::HashSet::new();

        // ── 1. Exact SHA-256 duplicates (sync SQLite) ──
        {
            let guard = sqlite.lock().unwrap();
            let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not initialized"))?;
            let conn = db.conn();

            let sha_sql = match media_type {
                Some(t) => format!(
                    "SELECT file_sha256, GROUP_CONCAT(id) AS ids, COUNT(*) AS cnt
                     FROM media WHERE deleted_at IS NULL AND is_hidden = 0 AND media_type = '{}'
                     GROUP BY file_sha256 HAVING cnt > 1", t),
                None =>
                    "SELECT file_sha256, GROUP_CONCAT(id) AS ids, COUNT(*) AS cnt
                     FROM media WHERE deleted_at IS NULL AND is_hidden = 0
                     GROUP BY file_sha256 HAVING cnt > 1".to_string(),
            };

            let mut stmt = conn.prepare(&sha_sql)?;
            let dup_rows: Vec<(String, String)> = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?.filter_map(|r| r.ok()).collect();

            for (sha256, ids_csv) in dup_rows {
                let ids: Vec<&str> = ids_csv.split(',').collect();
                if ids.len() < 2 { continue; }
                let placeholders: String = ids.iter().enumerate()
                    .map(|(i, _)| format!("?{}", i + 1)).collect::<Vec<_>>().join(",");
                let q = format!(
                    "SELECT id, file_name, file_size, thumbnail FROM media WHERE id IN ({})", placeholders
                );
                let mut s2 = conn.prepare(&q)?;
                let id_params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
                let items: Vec<(String, Option<String>, i64, Option<String>)> = s2.query_map(id_params.as_slice(), |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })?.filter_map(|r| r.ok()).collect();

                if items.len() < 2 { continue; }
                for (id, _, _, _) in &items { covered.insert(id.clone()); }
                groups.push(DuplicateGroup {
                    group_id: sha256.clone(),
                    reason: "Trùng Hash — giống nhau 100%".into(),
                    items: items.into_iter().map(|(id, name, size, thumb)| {
                        let file_path = name.as_ref().map(|n| format!("{}/{}", base, n)).unwrap_or_default();
                        let thumbnail_path = thumb.map(|t| if std::path::Path::new(&t).is_absolute() { t } else { format!("{}/{}", base, t) })
                            .or_else(|| { if media_type != Some("video") { return None; } resolve_video_thumb_fallback(base, name.as_deref()?, thumb_cache_dir) });
                        DuplicateItem { media_id: id, file_path, size: size as u64, thumbnail_path }
                    }).collect(),
                });
            }
        }

        // ── 2. pHash near-duplicate (Hamming <= 8, sync SQLite) ──
        {
            let guard = sqlite.lock().unwrap();
            let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not initialized"))?;
            let conn = db.conn();

            let phash_sql = match media_type {
                Some(t) => format!(
                    "SELECT id, file_name, file_size, file_phash, thumbnail FROM media
                     WHERE deleted_at IS NULL AND is_hidden = 0 AND file_phash IS NOT NULL AND media_type = '{}'", t),
                None =>
                    "SELECT id, file_name, file_size, file_phash, thumbnail FROM media
                     WHERE deleted_at IS NULL AND is_hidden = 0 AND file_phash IS NOT NULL".to_string(),
            };
            let mut stmt = conn.prepare(&phash_sql)?;
            let phash_items: Vec<(String, Option<String>, u64, u64, Option<String>)> = stmt.query_map([], |r| {
                let id: String = r.get(0)?;
                let name: Option<String> = r.get(1)?;
                let size: i64 = r.get(2)?;
                let phash_str: String = r.get(3)?;
                let thumb: Option<String> = r.get(4)?;
                Ok((id, name, size as u64, phash_str, thumb))
            })?.filter_map(|r| r.ok())
              .filter_map(|(id, name, size, phash_str, thumb)| {
                  let h = u64::from_str_radix(&phash_str, 16).ok()?;
                  Some((id, name, size, h, thumb))
              }).collect();

            let n = phash_items.len();
            let mut parent: Vec<usize> = (0..n).collect();
            for i in 0..n {
                for j in (i+1)..n {
                    if hamming(phash_items[i].3, phash_items[j].3) <= 8 {
                        let ri = find(&mut parent, i);
                        let rj = find(&mut parent, j);
                        if ri != rj { parent[ri] = rj; }
                    }
                }
            }
            let mut ph_clusters: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
            for i in 0..n { ph_clusters.entry(find(&mut parent, i)).or_default().push(i); }
            for (_root, idxs) in ph_clusters {
                if idxs.len() < 2 { continue; }
                let ids: Vec<&str> = idxs.iter().map(|&i| phash_items[i].0.as_str()).collect();
                if ids.iter().all(|id| covered.contains(*id)) { continue; }
                for id in &ids { covered.insert(id.to_string()); }
                groups.push(DuplicateGroup {
                    group_id: format!("phash_{}", ids[0]),
                    reason: "Ảnh gần giống nhau (pHash — cùng nội dung nhưng khác kích thước/nén)".into(),
                    items: idxs.iter().map(|&i| {
                        let ref item = phash_items[i];
                        DuplicateItem {
                            media_id: item.0.clone(),
                            file_path: item.1.as_ref().map(|n| format!("{}/{}", base, n)).unwrap_or_default(),
                            size: item.2,
                            thumbnail_path: item.4.as_ref().map(|t| if std::path::Path::new(t).is_absolute() { t.clone() } else { format!("{}/{}", base, t) })
                                .or_else(|| { if media_type != Some("video") { return None; } resolve_video_thumb_fallback(base, item.1.as_deref()?, thumb_cache_dir) }),
                        }
                    }).collect(),
                });
            }
        }

        // ── 3. Vision vector similarity via Qdrant nearest-neighbor search ──
        {
            let source_filter = match media_type {
                Some("video") => "video_frame",
                _ => "image",
            };

            let filter = Filter::must([
                Condition::matches("source", source_filter.to_string()),
            ]);

            let mut all_points = Vec::new();
            let mut next_offset: Option<qdrant_client::qdrant::PointId> = None;
            loop {
                let mut builder = ScrollPointsBuilder::new(collection)
                    .filter(filter.clone())
                    .limit(256)
                    .with_vectors(true)
                    .with_payload(true);
                if let Some(ref offset) = next_offset {
                    builder = builder.offset(offset.clone());
                }
                let resp = qdrant.scroll(builder).await?;
                let points = resp.result;
                let n_page = points.len();
                all_points.extend(points);
                match resp.next_page_offset {
                    Some(off) if n_page > 0 => next_offset = Some(off),
                    _ => break,
                }
            }

            struct EmbRow { media_id: String, vec: Vec<f32>, frame_idx: Option<u32> }
            let all_embs: Vec<EmbRow> = all_points.into_iter().filter_map(|p| {
                let media_id = p.payload.get("media_id")?.as_str()?.to_string();
                let frame_idx = p.payload.get("frame_idx").and_then(|v| v.as_integer()).map(|i| i as u32);
                let vectors = p.vectors?;
                let vec = match vectors.vectors_options? {
                    qdrant_client::qdrant::vectors_output::VectorsOptions::Vector(v) => match v.into_vector() {
                        vector_output::Vector::Dense(dense) => dense.data,
                        _ => return None,
                    },
                    _ => return None,
                };
                Some(EmbRow { media_id, vec, frame_idx })
            }).collect();

            let mut rep_embs: std::collections::HashMap<String, (&EmbRow, u32)> = std::collections::HashMap::new();
            for e in &all_embs {
                let fi = e.frame_idx.unwrap_or(u32::MAX);
                rep_embs.entry(e.media_id.clone())
                    .and_modify(|(prev, prev_fi)| { if fi < *prev_fi { *prev = e; *prev_fi = fi; } })
                    .or_insert((e, fi));
            }
            let rep: Vec<(&EmbRow, String)> = rep_embs.into_iter().map(|(mid, (e, _))| (e, mid)).collect();

            if rep.len() > 1 {
                let mut pairs: Vec<(usize, usize)> = Vec::new();
                let index_by_media_id: std::collections::HashMap<String, usize> = rep
                    .iter()
                    .enumerate()
                    .map(|(idx, (_, media_id))| (media_id.clone(), idx))
                    .collect();

                for i in 0..rep.len() {
                    let query_vec = rep[i].0.vec.clone();
                    let resp = qdrant.search_points(
                        SearchPointsBuilder::new(collection, query_vec, rep.len() as u64)
                            .filter(filter.clone())
                            .score_threshold(DUPLICATE_THRESHOLD)
                            .with_payload(true)
                    ).await?;

                    for hit in resp.result {
                        let Some(hit_media_id) = hit.payload
                            .get("media_id")
                            .and_then(|v| v.as_str())
                        else {
                            continue;
                        };
                        if hit_media_id == &rep[i].1 {
                            continue;
                        }
                        let Some(&j) = index_by_media_id.get(hit_media_id) else {
                            continue;
                        };
                        if i < j {
                            pairs.push((i, j));
                        }
                    }
                }

                pairs.sort_unstable();
                pairs.dedup();

                if !pairs.is_empty() {
                    let mut par: Vec<usize> = (0..rep.len()).collect();
                    for (i, j) in pairs {
                        let ri = find(&mut par, i);
                        let rj = find(&mut par, j);
                        if ri != rj { par[ri] = rj; }
                    }
                    let mut clusters: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
                    for i in 0..rep.len() { clusters.entry(find(&mut par, i)).or_default().push(i); }

                    // Lock SQLite again for the final DB lookups
                    let guard = sqlite.lock().unwrap();
                    let db = guard.as_ref().ok_or_else(|| anyhow::anyhow!("DB not initialized"))?;
                    let conn = db.conn();

                    let mut ci = 0usize;
                    for (_root, idxs) in clusters {
                        if idxs.len() < 2 { continue; }
                        let ids: Vec<String> = idxs.iter().map(|&i| rep[i].1.clone()).collect();
                        if ids.iter().all(|id| covered.contains(id)) { continue; }
                        for id in &ids { covered.insert(id.clone()); }

                        let placeholders: String = ids.iter().enumerate()
                            .map(|(i, _)| format!("?{}", i + 1)).collect::<Vec<_>>().join(",");
                        let q = format!(
                            "SELECT id, file_name, file_size, thumbnail FROM media WHERE id IN ({})", placeholders
                        );
                        let mut s3 = conn.prepare(&q)?;
                        let id_params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
                        let items: Vec<(String, Option<String>, i64, Option<String>)> = s3.query_map(id_params.as_slice(), |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                        })?.filter_map(|r| r.ok()).collect();

                        if items.len() < 2 { continue; }
                        let reason = match media_type {
                            Some("video") => "Video gần giống nhau (AI phát hiện đoạn đầu tương tự ≥ 92%)",
                            _ => "Ảnh gần giống nhau (AI phát hiện ≥ 92%)",
                        };
                        groups.push(DuplicateGroup {
                            group_id: format!("vec_{}_{}", ci, items[0].0),
                            reason: reason.into(),
                            items: items.into_iter().map(|(id, name, size, thumb)| {
                                let file_path = name.as_ref().map(|n| format!("{}/{}", base, n)).unwrap_or_default();
                                let thumbnail_path = thumb.map(|t| if std::path::Path::new(&t).is_absolute() { t } else { format!("{}/{}", base, t) })
                                    .or_else(|| { if media_type != Some("video") { return None; } resolve_video_thumb_fallback(base, name.as_deref()?, thumb_cache_dir) });
                                DuplicateItem { media_id: id, file_path, size: size as u64, thumbnail_path }
                            }).collect(),
                        });
                        ci += 1;
                    }
                }
            }
        }

        Ok(groups)
    }
}
