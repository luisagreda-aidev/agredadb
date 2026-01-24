//! Node representation in HNSW graph

pub type NodeId = usize;

/// Node in HNSW graph
#[derive(Clone)]
pub struct Node {
    pub id: NodeId,
    pub vector: Vec<f32>,
    pub connections: Vec<(NodeId, f32)>, // (neighbor_id, distance)
}

impl Node {
    pub fn new(id: NodeId, vector: Vec<f32>) -> Self {
        Self {
            id,
            vector,
            connections: Vec::new(),
        }
    }

    /// Add connection to neighbor
    pub fn add_connection(&mut self, neighbor_id: NodeId, distance: f32) {
        self.connections.push((neighbor_id, distance));
    }

    /// Get k nearest neighbors
    pub fn get_nearest(&self, k: usize) -> Vec<(NodeId, f32)> {
        let mut connections = self.connections.clone();
        connections.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        connections.truncate(k);
        connections
    }

    /// Prune connections to keep only M best
    pub fn prune_connections(&mut self, max_connections: usize) {
        if self.connections.len() <= max_connections {
            return;
        }

        // Sort by distance (ascending)
        self.connections.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        
        // Keep only M best
        self.connections.truncate(max_connections);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_creation() {
        let vector = vec![1.0, 2.0, 3.0];
        let node = Node::new(0, vector.clone());
        
        assert_eq!(node.id, 0);
        assert_eq!(node.vector, vector);
        assert_eq!(node.connections.len(), 0);
    }

    #[test]
    fn test_add_connection() {
        let mut node = Node::new(0, vec![1.0, 2.0, 3.0]);
        
        node.add_connection(1, 0.5);
        node.add_connection(2, 0.3);
        
        assert_eq!(node.connections.len(), 2);
    }

    #[test]
    fn test_prune_connections() {
        let mut node = Node::new(0, vec![1.0, 2.0, 3.0]);
        
        node.add_connection(1, 0.5);
        node.add_connection(2, 0.3);
        node.add_connection(3, 0.7);
        node.add_connection(4, 0.2);
        
        node.prune_connections(2);
        
        assert_eq!(node.connections.len(), 2);
        assert_eq!(node.connections[0].0, 4); // Closest
        assert_eq!(node.connections[1].0, 2); // Second closest
    }
}
