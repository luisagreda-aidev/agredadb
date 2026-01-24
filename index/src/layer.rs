//! Layer in HNSW graph

use crate::node::{Node, NodeId};
use ahash::AHashMap;
use parking_lot::RwLock;
use std::sync::Arc;

/// Layer in HNSW graph
pub struct Layer {
    nodes: Arc<RwLock<AHashMap<NodeId, Node>>>,
}

impl Layer {
    pub fn new() -> Self {
        Self {
            nodes: Arc::new(RwLock::new(AHashMap::new())),
        }
    }

    /// Insert node into layer
    pub fn insert(&self, node: Node) {
        let mut nodes = self.nodes.write();
        nodes.insert(node.id, node);
    }

    /// Get node by ID
    pub fn get(&self, id: NodeId) -> Option<Node> {
        let nodes = self.nodes.read();
        nodes.get(&id).cloned()
    }

    /// Update node connections
    pub fn update_connections(&self, id: NodeId, connections: Vec<(NodeId, f32)>) {
        let mut nodes = self.nodes.write();
        if let Some(node) = nodes.get_mut(&id) {
            node.connections = connections;
        }
    }

    /// Get all node IDs
    pub fn get_all_ids(&self) -> Vec<NodeId> {
        let nodes = self.nodes.read();
        nodes.keys().copied().collect()
    }

    /// Get number of nodes
    pub fn len(&self) -> usize {
        let nodes = self.nodes.read();
        nodes.len()
    }

    /// Check if layer is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_creation() {
        let layer = Layer::new();
        assert_eq!(layer.len(), 0);
        assert!(layer.is_empty());
    }

    #[test]
    fn test_layer_insert() {
        let layer = Layer::new();
        let node = Node::new(0, vec![1.0, 2.0, 3.0]);
        
        layer.insert(node);
        
        assert_eq!(layer.len(), 1);
        assert!(!layer.is_empty());
    }

    #[test]
    fn test_layer_get() {
        let layer = Layer::new();
        let node = Node::new(0, vec![1.0, 2.0, 3.0]);
        
        layer.insert(node.clone());
        
        let retrieved = layer.get(0).unwrap();
        assert_eq!(retrieved.id, 0);
        assert_eq!(retrieved.vector, vec![1.0, 2.0, 3.0]);
    }
}
