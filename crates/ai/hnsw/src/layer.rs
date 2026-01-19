//! HNSW layer management

use alloc::collections::BTreeSet;
use crate::node::NodeId;

/// A layer in the HNSW graph
#[derive(Debug, Clone)]
pub struct Layer {
    /// Layer number (0 is bottom)
    pub level: usize,
    /// Nodes present in this layer
    pub nodes: BTreeSet<NodeId>,
}

impl Layer {
    /// Create a new empty layer
    pub fn new(level: usize) -> Self {
        Layer {
            level,
            nodes: BTreeSet::new(),
        }
    }

    /// Add a node to this layer
    pub fn add_node(&mut self, node: NodeId) {
        self.nodes.insert(node);
    }

    /// Remove a node from this layer
    pub fn remove_node(&mut self, node: NodeId) {
        self.nodes.remove(&node);
    }

    /// Check if node exists in this layer
    pub fn contains(&self, node: NodeId) -> bool {
        self.nodes.contains(&node)
    }

    /// Get number of nodes in this layer
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if layer is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
