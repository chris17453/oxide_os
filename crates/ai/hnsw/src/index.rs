//! HNSW index implementation

use alloc::collections::{BTreeMap, BinaryHeap};
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::distance::cosine_distance;
use crate::layer::Layer;
use crate::node::{Node, NodeId};
use crate::{FileId, HnswConfig, SearchResult};

/// Natural log approximation for no_std
fn ln_f64(x: f64) -> f64 {
    if x <= 0.0 {
        return f64::NEG_INFINITY;
    }
    // Use the identity: ln(x) = 2 * atanh((x-1)/(x+1))
    // and Taylor series for atanh
    let y = (x - 1.0) / (x + 1.0);
    let y2 = y * y;
    let mut sum = y;
    let mut term = y;
    for i in 1..20 {
        term *= y2;
        sum += term / (2 * i + 1) as f64;
    }
    2.0 * sum
}

/// Candidate for search (ordered by distance, min-heap behavior)
#[derive(Debug, Clone)]
struct Candidate {
    distance: f32,
    node_id: NodeId,
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse for min-heap (BinaryHeap is max-heap by default)
        other
            .distance
            .partial_cmp(&self.distance)
            .unwrap_or(Ordering::Equal)
    }
}

/// HNSW vector index
pub struct HnswIndex {
    /// Configuration
    config: HnswConfig,
    /// All nodes
    nodes: BTreeMap<NodeId, Node>,
    /// File ID to Node ID mapping
    file_to_node: BTreeMap<FileId, NodeId>,
    /// Layers
    layers: Vec<Layer>,
    /// Entry point node
    entry_point: Option<NodeId>,
    /// Next node ID
    next_id: usize,
}

impl HnswIndex {
    /// Create a new empty index
    pub fn new(config: HnswConfig) -> Self {
        HnswIndex {
            config,
            nodes: BTreeMap::new(),
            file_to_node: BTreeMap::new(),
            layers: vec![Layer::new(0)],
            entry_point: None,
            next_id: 0,
        }
    }

    /// Get number of indexed vectors
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Generate random layer for new node
    fn random_layer(&self) -> usize {
        // Simplified: use a basic random based on current node count
        let r = (self.next_id as f64 * 0.618033988749895) % 1.0;
        let l = (-ln_f64(r) * self.config.ml) as usize;
        l.min(16) // Cap at 16 layers
    }

    /// Insert a vector with associated file ID
    pub fn insert(&mut self, file_id: FileId, vector: Vec<f32>) -> Result<NodeId, &'static str> {
        if vector.len() != self.config.dim {
            return Err("Vector dimension mismatch");
        }

        // Check if already indexed
        if self.file_to_node.contains_key(&file_id) {
            // Update existing
            let node_id = self.file_to_node[&file_id];
            if let Some(node) = self.nodes.get_mut(&node_id) {
                node.vector = vector;
            }
            return Ok(node_id);
        }

        // Create new node
        let node_id = NodeId(self.next_id);
        self.next_id += 1;

        let max_layer = self.random_layer();

        // Ensure we have enough layers
        while self.layers.len() <= max_layer {
            self.layers.push(Layer::new(self.layers.len()));
        }

        let node = Node::new(
            node_id,
            file_id,
            vector.clone(),
            max_layer,
            self.config.m_max,
        );
        self.nodes.insert(node_id, node);
        self.file_to_node.insert(file_id, node_id);

        // Add to layers
        for l in 0..=max_layer {
            self.layers[l].add_node(node_id);
        }

        // Connect to graph
        if let Some(ep) = self.entry_point {
            // Find entry point for each layer
            let mut curr_obj = ep;
            let top_layer = self.get_max_layer();

            // Search down from top
            for layer in (max_layer + 1..=top_layer).rev() {
                let nearest = self.search_layer(&vector, curr_obj, 1, layer);
                if !nearest.is_empty() {
                    curr_obj = nearest[0].node_id;
                }
            }

            // Insert at each layer
            for layer in (0..=max_layer).rev() {
                let neighbors =
                    self.search_layer(&vector, curr_obj, self.config.ef_construction, layer);

                // Select M best neighbors
                let m = if layer == 0 {
                    self.config.m * 2
                } else {
                    self.config.m
                };
                let selected: Vec<NodeId> = neighbors.iter().take(m).map(|c| c.node_id).collect();

                // Connect bidirectionally
                if let Some(node) = self.nodes.get_mut(&node_id) {
                    node.set_connections(layer, selected.clone());
                }

                // Get vector copy for pruning
                let node_vec = self.nodes.get(&node_id).map(|n| n.vector.clone());

                for &neighbor_id in &selected {
                    if let Some(neighbor) = self.nodes.get_mut(&neighbor_id) {
                        neighbor.add_connection(layer, node_id);

                        // Prune if too many connections
                        if neighbor.connections[layer].len() > self.config.m_max {
                            if let Some(ref v) = node_vec {
                                self.prune_connections(neighbor_id, layer, v);
                            }
                        }
                    }
                }

                if !neighbors.is_empty() {
                    curr_obj = neighbors[0].node_id;
                }
            }
        }

        // Update entry point if needed
        if self.entry_point.is_none() || max_layer > self.get_entry_layer() {
            self.entry_point = Some(node_id);
        }

        Ok(node_id)
    }

    /// Search for k nearest neighbors
    pub fn search(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        if self.is_empty() || query.len() != self.config.dim {
            return Vec::new();
        }

        let ep = match self.entry_point {
            Some(ep) => ep,
            None => return Vec::new(),
        };

        let mut curr_obj = ep;
        let top_layer = self.get_max_layer();

        // Search down from top layer
        for layer in (1..=top_layer).rev() {
            let nearest = self.search_layer(query, curr_obj, 1, layer);
            if !nearest.is_empty() {
                curr_obj = nearest[0].node_id;
            }
        }

        // Search at layer 0 with ef_search
        let candidates = self.search_layer(query, curr_obj, self.config.ef_search.max(k), 0);

        // Return top k
        candidates
            .into_iter()
            .take(k)
            .map(|c| {
                let file_id = self.nodes[&c.node_id].file_id;
                SearchResult {
                    id: file_id,
                    distance: c.distance,
                }
            })
            .collect()
    }

    /// Delete a file from the index
    pub fn delete(&mut self, file_id: FileId) -> Result<(), &'static str> {
        let node_id = match self.file_to_node.remove(&file_id) {
            Some(id) => id,
            None => return Err("File not found in index"),
        };

        // Collect connection info first
        let (max_layer, connections) = match self.nodes.get(&node_id) {
            Some(node) => (node.max_layer, node.connections.clone()),
            None => return Ok(()),
        };

        // Remove from all neighbors
        for layer in 0..=max_layer {
            for &neighbor_id in &connections[layer] {
                if let Some(neighbor) = self.nodes.get_mut(&neighbor_id) {
                    neighbor.remove_connection(layer, node_id);
                }
            }
            self.layers[layer].remove_node(node_id);
        }

        // Remove node
        self.nodes.remove(&node_id);

        // Update entry point if needed
        if self.entry_point == Some(node_id) {
            self.entry_point = self.find_new_entry_point();
        }

        Ok(())
    }

    /// Search within a single layer
    fn search_layer(
        &self,
        query: &[f32],
        entry: NodeId,
        ef: usize,
        layer: usize,
    ) -> Vec<Candidate> {
        let mut visited = BTreeMap::new();
        let mut candidates = BinaryHeap::new();
        let mut results = BinaryHeap::new();

        let entry_dist = self.distance_to(query, entry);
        visited.insert(entry, true);
        candidates.push(Candidate {
            distance: entry_dist,
            node_id: entry,
        });
        results.push(Candidate {
            distance: -entry_dist,
            node_id: entry,
        }); // Max-heap for worst result

        while let Some(current) = candidates.pop() {
            // Get worst result distance
            let worst_dist = if let Some(worst) = results.peek() {
                -worst.distance
            } else {
                f32::INFINITY
            };

            if current.distance > worst_dist {
                break;
            }

            // Explore neighbors
            if let Some(node) = self.nodes.get(&current.node_id) {
                for &neighbor_id in node.get_connections(layer) {
                    if visited.contains_key(&neighbor_id) {
                        continue;
                    }
                    visited.insert(neighbor_id, true);

                    let dist = self.distance_to(query, neighbor_id);
                    let worst_dist = if let Some(worst) = results.peek() {
                        -worst.distance
                    } else {
                        f32::INFINITY
                    };

                    if dist < worst_dist || results.len() < ef {
                        candidates.push(Candidate {
                            distance: dist,
                            node_id: neighbor_id,
                        });
                        results.push(Candidate {
                            distance: -dist,
                            node_id: neighbor_id,
                        });

                        if results.len() > ef {
                            results.pop(); // Remove worst
                        }
                    }
                }
            }
        }

        // Convert results to vec (sorted by distance)
        let mut result_vec: Vec<_> = results
            .into_iter()
            .map(|c| Candidate {
                distance: -c.distance,
                node_id: c.node_id,
            })
            .collect();
        result_vec.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(Ordering::Equal)
        });
        result_vec
    }

    /// Compute distance from query to a node
    fn distance_to(&self, query: &[f32], node_id: NodeId) -> f32 {
        match self.nodes.get(&node_id) {
            Some(node) => cosine_distance(query, &node.vector),
            None => f32::INFINITY,
        }
    }

    /// Prune connections for a node at a layer
    fn prune_connections(&mut self, node_id: NodeId, layer: usize, _reference: &[f32]) {
        if let Some(node) = self.nodes.get(&node_id) {
            let node_vec = node.vector.clone();
            let mut connections = node.connections[layer].clone();

            // Sort by distance and keep only m_max
            connections.sort_by(|&a, &b| {
                let dist_a = self.distance_to(&node_vec, a);
                let dist_b = self.distance_to(&node_vec, b);
                dist_a.partial_cmp(&dist_b).unwrap_or(Ordering::Equal)
            });

            connections.truncate(self.config.m_max);

            if let Some(node) = self.nodes.get_mut(&node_id) {
                node.set_connections(layer, connections);
            }
        }
    }

    /// Get maximum layer in the graph
    fn get_max_layer(&self) -> usize {
        self.layers.len().saturating_sub(1)
    }

    /// Get entry point's layer
    fn get_entry_layer(&self) -> usize {
        match self.entry_point {
            Some(ep) => self.nodes.get(&ep).map(|n| n.max_layer).unwrap_or(0),
            None => 0,
        }
    }

    /// Find new entry point after deletion
    fn find_new_entry_point(&self) -> Option<NodeId> {
        for layer in (0..self.layers.len()).rev() {
            if let Some(&node_id) = self.layers[layer].nodes.iter().next() {
                return Some(node_id);
            }
        }
        None
    }
}

impl Default for HnswIndex {
    fn default() -> Self {
        Self::new(HnswConfig::default())
    }
}
