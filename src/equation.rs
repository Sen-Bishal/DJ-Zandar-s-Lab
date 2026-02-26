#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

/// AST for computing global Destruction entropy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DestructionNode {
    EntityCount(u32),
    ConflictEvent(f64),
    EntropyMultiplier(f64),
}

/// Deterministically evaluates an entropy score in `[0.0, 1.0]`.
pub fn evaluate_destruction_ast(nodes: &[DestructionNode]) -> f64 {
    let mut base_entropy = 0.0_f64;
    let mut multiplier = 1.0_f64;

    #[cfg(not(target_arch = "wasm32"))]
    let ordered_nodes: Vec<DestructionNode> = nodes.par_iter().copied().collect();
    #[cfg(target_arch = "wasm32")]
    let ordered_nodes: Vec<DestructionNode> = nodes.to_vec();

    for node in ordered_nodes {
        match node {
            DestructionNode::EntityCount(count) => {
                // Scales toward 0.35 at one million entities.
                let normalized = (count as f64 / 1_000_000.0).clamp(0.0, 1.0);
                base_entropy += normalized * 0.35;
            }
            DestructionNode::ConflictEvent(severity) => {
                base_entropy += severity.clamp(0.0, 1.0) * 0.5;
            }
            DestructionNode::EntropyMultiplier(scale) => {
                multiplier *= scale.clamp(0.0, 4.0);
            }
        }
    }

    (base_entropy * multiplier).clamp(0.0, 1.0)
}
