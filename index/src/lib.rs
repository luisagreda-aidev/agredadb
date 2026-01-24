//! # AgredaDB HNSW Index
//!
//! Hierarchical Navigable Small World (HNSW) index implementation for fast
//! approximate nearest neighbor search.
//!
//! ## Features
//!
//! - O(log N) search complexity
//! - 100-1000x faster than brute force
//! - Incremental updates
//! - SIMD-optimized distance calculations
//!
//! ## Example
//!
//! ```rust
//! use agredadb_hnsw::HnswIndex;
//!
//! // Create index
//! let mut index = HnswIndex::new(16, 200, 128);
//!
//! // Insert vectors
//! for i in 0..1000 {
//!     let vector = vec![i as f32; 128];
//!     index.insert(i, vector);
//! }
//!
//! // Search
//! let query = vec![500.0; 128];
//! let results = index.search(&query, 10);
//! ```

pub mod index;
pub mod distance;
pub mod layer;
pub mod node;

pub use index::HnswIndex;
pub use distance::{Distance, cosine_similarity, euclidean_distance};
pub use layer::Layer;
pub use node::{Node, NodeId};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_operations() {
        let mut index = HnswIndex::new(16, 200, 128);
        
        // Insert some vectors
        for i in 0..100 {
            let vector = vec![i as f32; 128];
            index.insert(i, vector);
        }
        
        // Search
        let query = vec![50.0; 128];
        let results = index.search(&query, 5);
        
        assert!(results.len() <= 5);
    }
}
