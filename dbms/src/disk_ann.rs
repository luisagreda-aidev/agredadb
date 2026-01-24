use std::sync::Arc;
use std::path::Path;
use rkyv::check_archived_root;
use agredadb_storage::{RawBlockManager, PersistentNode};
use wide::f32x8;

pub struct DiskAnnIndex {
    io_engine: Arc<RawBlockManager>,
    entry_point: u64,
}

impl DiskAnnIndex {
    pub fn new(_path: impl AsRef<Path>, io_engine: Arc<RawBlockManager>) -> Self {
        Self {
            io_engine,
            entry_point: 0,
        }
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<u64> {
        let mut current_offset = self.entry_point;
        let mut top_k = std::collections::BinaryHeap::new();
        let mut visited = std::collections::HashSet::new();

        for _ in 0..20 {
            if visited.contains(&current_offset) { break; }
            visited.insert(current_offset);

            let block_data = match self.io_engine.read_block(current_offset, 4096) {
                Ok(data) => data,
                Err(_) => break,
            };

            // Corregido: check_archived_root es seguro con las validaciones activas
            let node = match check_archived_root::<PersistentNode>(&block_data) {
                Ok(n) => n,
                Err(_) => break,
            };
            
            let dist = self.calculate_simd_distance(query, &node.vector);
            top_k.push((ordered_float::OrderedFloat(dist), node.id));

            if let Some(&next_offset) = node.neighbors.iter().min_by_key(|&&offset| {
                let _ = self.io_engine.read_block(offset, 4096);
                offset
            }) {
                current_offset = next_offset;
            } else {
                break;
            }
        }

        top_k.into_iter().take(k).map(|(_, id)| id).collect()
    }

    #[inline]
    fn calculate_simd_distance(&self, a: &[f32], b: &[f32]) -> f32 {
        let len = a.len().min(b.len());
        if len == 0 { return 1.0; }

        let mut dot = f32x8::ZERO;
        let mut norm_a = f32x8::ZERO;
        let mut norm_b = f32x8::ZERO;

        let chunks = len / 8;
        for i in 0..chunks {
            let va = f32x8::from(&a[i*8..(i+1)*8]);
            let vb = f32x8::from(&b[i*8..(i+1)*8]);
            dot += va * vb;
            norm_a += va * va;
            norm_b += vb * vb;
        }

        let dot_sum: f32 = dot.reduce_add();
        let norm_a_sum: f32 = norm_a.reduce_add();
        let norm_b_sum: f32 = norm_b.reduce_add();

        if norm_a_sum <= 0.0 || norm_b_sum <= 0.0 { return 1.0; }
        1.0 - (dot_sum / (norm_a_sum.sqrt() * norm_b_sum.sqrt()))
    }
}
