//! HNSW graph nodes

use crate::FileId;
use alloc::vec::Vec;

/// Node identifier in the graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);

/// A node in the HNSW graph
#[derive(Debug, Clone)]
pub struct Node {
    /// Node identifier
    pub id: NodeId,
    /// Associated file
    pub file_id: FileId,
    /// Embedding vector
    pub vector: Vec<f32>,
    /// Maximum layer this node exists in
    pub max_layer: usize,
    /// Connections at each layer
    pub connections: Vec<Vec<NodeId>>,
}

impl Node {
    /// Create a new node
    pub fn new(
        id: NodeId,
        file_id: FileId,
        vector: Vec<f32>,
        max_layer: usize,
        m_max: usize,
    ) -> Self {
        let mut connections = Vec::with_capacity(max_layer + 1);
        for _ in 0..=max_layer {
            connections.push(Vec::with_capacity(m_max));
        }

        Node {
            id,
            file_id,
            vector,
            max_layer,
            connections,
        }
    }

    /// Get connections at a specific layer
    pub fn get_connections(&self, layer: usize) -> &[NodeId] {
        if layer < self.connections.len() {
            &self.connections[layer]
        } else {
            &[]
        }
    }

    /// Add a connection at a specific layer
    pub fn add_connection(&mut self, layer: usize, neighbor: NodeId) {
        if layer < self.connections.len() {
            if !self.connections[layer].contains(&neighbor) {
                self.connections[layer].push(neighbor);
            }
        }
    }

    /// Remove a connection at a specific layer
    pub fn remove_connection(&mut self, layer: usize, neighbor: NodeId) {
        if layer < self.connections.len() {
            self.connections[layer].retain(|&n| n != neighbor);
        }
    }

    /// Set connections at a specific layer
    pub fn set_connections(&mut self, layer: usize, neighbors: Vec<NodeId>) {
        if layer < self.connections.len() {
            self.connections[layer] = neighbors;
        }
    }
}
