//! HNSW Index implementation

use crate::distance::cosine_similarity;
use crate::layer::Layer;
use crate::node::{Node, NodeId};
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use std::collections::BinaryHeap;
use std::sync::Arc;

/// HNSW Index
pub struct HnswIndex {
    /// Layers (0 = bottom layer with all nodes)
    layers: Vec<Layer>,
    /// Entry point node ID
    entry_point: Arc<RwLock<Option<NodeId>>>,
    /// Maximum connections per node (M)
    max_connections: usize,
    /// Size of dynamic candidate list (efConstruction)
    ef_construction: usize,
    /// Level multiplier (mL)
    _level_multiplier: f64,
    /// Vector dimension
    dimension: usize,
    /// Next node ID
    _next_id: Arc<RwLock<NodeId>>,
}

impl HnswIndex {
    /// Create new HNSW index
    ///
    /// # Arguments
    /// * `max_connections` - M parameter (typically 16-32)
    /// * `ef_construction` - efConstruction parameter (typically 200)
    /// * `dimension` - Vector dimension
    pub fn new(max_connections: usize, ef_construction: usize, dimension: usize) -> Self {
        Self {
            layers: vec![Layer::new()], // Start with one layer
            entry_point: Arc::new(RwLock::new(None)),
            max_connections,
            ef_construction,
            _level_multiplier: 1.0 / (max_connections as f64).ln(),
            dimension,
            _next_id: Arc::new(RwLock::new(0)),
        }
    }

    /// Insert vector into index
    pub fn insert(&mut self, id: NodeId, vector: Vec<f32>) {
        assert_eq!(vector.len(), self.dimension, "Vector dimension mismatch");

        // Determine level for new node
        let level = self.random_level();

        // Ensure we have enough layers
        while self.layers.len() <= level {
            self.layers.push(Layer::new());
        }

        // Create node
        let node = Node::new(id, vector.clone());

        // Get entry point
        let entry_point = *self.entry_point.read();

        if entry_point.is_none() {
            // First node - just insert
            self.layers[0].insert(node.clone());
            *self.entry_point.write() = Some(id);
            return;
        }

        let entry_point = entry_point.unwrap();

        // Search for nearest neighbors at each layer
        let mut current_nearest = vec![(entry_point, 0.0)];

        // Start from top layer and go down
        for layer_idx in (level + 1..self.layers.len()).rev() {
            current_nearest = self.search_layer(&vector, current_nearest, 1, layer_idx);
        }

        // Insert into layers 0..=level
        for layer_idx in 0..=level {
            // Search for ef_construction nearest neighbors
            let candidates = self.search_layer(
                &vector,
                current_nearest.clone(),
                self.ef_construction,
                layer_idx,
            );

            // Select M neighbors
            let neighbors = self.select_neighbors(&vector, candidates, self.max_connections);

            // Add connections
            let mut new_node = node.clone();
            for (neighbor_id, distance) in &neighbors {
                new_node.add_connection(*neighbor_id, *distance);
            }

            // Insert node
            self.layers[layer_idx].insert(new_node);

            // Update neighbors' connections
            for (neighbor_id, distance) in neighbors {
                if let Some(mut neighbor) = self.layers[layer_idx].get(neighbor_id) {
                    neighbor.add_connection(id, distance);
                    neighbor.prune_connections(self.max_connections);
                    self.layers[layer_idx].update_connections(neighbor_id, neighbor.connections);
                }
            }

            current_nearest = vec![(id, 0.0)];
        }

        // Update entry point if necessary
        if level > self.get_entry_level() {
            *self.entry_point.write() = Some(id);
        }
    }

    /// Search for k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Vec<(NodeId, f32)> {
        assert_eq!(query.len(), self.dimension, "Query dimension mismatch");

        let entry_point = *self.entry_point.read();
        if entry_point.is_none() {
            return Vec::new();
        }

        let entry_point = entry_point.unwrap();
        
        // Calculate distance to entry point
        let entry_node = self.layers[0].get(entry_point).unwrap();
        let entry_dist = 1.0 - cosine_similarity(query, &entry_node.vector);
        let mut current_nearest = vec![(entry_point, entry_dist)];

        // Search from top layer to layer 1
        for layer_idx in (1..self.layers.len()).rev() {
            current_nearest = self.search_layer(query, current_nearest, 1, layer_idx);
        }

        // Search layer 0 with ef = max(k, ef_construction)
        let ef = k.max(self.ef_construction);
        let candidates = self.search_layer(query, current_nearest, ef, 0);

        // Return top k
        let mut results = candidates;
        results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        results.truncate(k);
        results
    }

    /// Search single layer
    fn search_layer(
        &self,
        query: &[f32],
        entry_points: Vec<(NodeId, f32)>,
        ef: usize,
        layer_idx: usize,
    ) -> Vec<(NodeId, f32)> {
        let layer = &self.layers[layer_idx];

        // If layer is empty, return empty
        if layer.is_empty() {
            return Vec::new();
        }

        // Priority queue for candidates (max heap by distance)
        let mut candidates = BinaryHeap::new();
        // Priority queue for results (max heap by distance)
        let mut results = BinaryHeap::new();
        // Visited set
        let mut visited = ahash::AHashSet::new();

        // Initialize with entry points
        for (node_id, _) in entry_points {
            if let Some(node) = layer.get(node_id) {
                let distance = 1.0 - cosine_similarity(query, &node.vector);
                candidates.push((OrderedFloat(-distance), node_id));
                results.push((OrderedFloat(distance), node_id));
                visited.insert(node_id);
            }
        }

        // If no valid entry points, search all nodes (fallback for small indices)
        if candidates.is_empty() {
            let all_ids = layer.get_all_ids();
            for node_id in all_ids {
                if let Some(node) = layer.get(node_id) {
                    let distance = 1.0 - cosine_similarity(query, &node.vector);
                    candidates.push((OrderedFloat(-distance), node_id));
                    results.push((OrderedFloat(distance), node_id));
                    visited.insert(node_id);
                }
            }
        }

        // Search
        while let Some((neg_dist, current_id)) = candidates.pop() {
            let current_dist = -neg_dist.0;

            // Check if we should continue
            if let Some((max_dist, _)) = results.peek() {
                if current_dist > max_dist.0 {
                    break;
                }
            }

            // Get current node
            if let Some(current_node) = layer.get(current_id) {
                // Check all neighbors
                for (neighbor_id, _) in &current_node.connections {
                    if visited.contains(neighbor_id) {
                        continue;
                    }
                    visited.insert(*neighbor_id);

                    if let Some(neighbor_node) = layer.get(*neighbor_id) {
                        let distance = 1.0 - cosine_similarity(query, &neighbor_node.vector);

                        // Check if this is better than worst result
                        if results.len() < ef {
                            candidates.push((OrderedFloat(-distance), *neighbor_id));
                            results.push((OrderedFloat(distance), *neighbor_id));
                        } else if let Some((max_dist, _)) = results.peek() {
                            if distance < max_dist.0 {
                                candidates.push((OrderedFloat(-distance), *neighbor_id));
                                results.push((OrderedFloat(distance), *neighbor_id));
                                
                                // Remove worst result
                                if results.len() > ef {
                                    results.pop();
                                }
                            }
                        }
                    }
                }
            }
        }

        // Convert results to vector
        results
            .into_sorted_vec()
            .into_iter()
            .map(|(dist, id)| (id, dist.0))
            .collect()
    }

    /// Select M neighbors using heuristic
    fn select_neighbors(
        &self,
        _query: &[f32],
        candidates: Vec<(NodeId, f32)>,
        m: usize,
    ) -> Vec<(NodeId, f32)> {
        // Simple heuristic: select M nearest
        let mut sorted = candidates;
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        sorted.truncate(m);
        sorted
    }

    /// Generate random level for new node
    fn random_level(&self) -> usize {
        let mut level = 0;
        let mut rng = rand::random::<f64>();
        
        while rng < 0.5 && level < 16 {
            level += 1;
            rng = rand::random::<f64>();
        }
        
        level
    }

    /// Get level of entry point
    fn get_entry_level(&self) -> usize {
        self.layers.len() - 1
    }

    /// Get statistics
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            num_layers: self.layers.len(),
            num_nodes: self.layers[0].len(),
            max_connections: self.max_connections,
            ef_construction: self.ef_construction,
        }
    }
}

/// Index statistics
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub num_layers: usize,
    pub num_nodes: usize,
    pub max_connections: usize,
    pub ef_construction: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_creation() {
        let index = HnswIndex::new(16, 200, 128);
        let stats = index.stats();
        
        assert_eq!(stats.num_layers, 1);
        assert_eq!(stats.num_nodes, 0);
    }

    #[test]
    fn test_insert_and_search() {
        let mut index = HnswIndex::new(16, 200, 3);
        
        // Insert vectors
        index.insert(0, vec![1.0, 0.0, 0.0]);
        index.insert(1, vec![0.0, 1.0, 0.0]);
        index.insert(2, vec![0.0, 0.0, 1.0]);
        index.insert(3, vec![0.9, 0.1, 0.0]);
        
        // Search for nearest to [1, 0, 0]
        let results = index.search(&[1.0, 0.0, 0.0], 2);
        
        assert!(results.len() >= 2, "Should find at least 2 results, found {}", results.len());
        assert_eq!(results[0].0, 0, "First result should be exact match (id=0)");
        // Second result should be id=3 (closest to [1,0,0])
        assert_eq!(results[1].0, 3, "Second result should be id=3");
    }

    #[test]
    fn test_large_index() {
        let mut index = HnswIndex::new(16, 200, 128);
        
        // Insert 1000 vectors
        for i in 0..1000 {
            let vector: Vec<f32> = (0..128).map(|j| (i + j) as f32).collect();
            index.insert(i, vector);
        }
        
        let stats = index.stats();
        assert_eq!(stats.num_nodes, 1000);
        
        // Search
        let query: Vec<f32> = (0..128).map(|j| (500 + j) as f32).collect();
        let results = index.search(&query, 10);
        
        assert_eq!(results.len(), 10);
        // Should find node 500 as closest
        assert!(results.iter().any(|(id, _)| *id == 500));
    }
}
