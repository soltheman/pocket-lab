//! Walk routes for AI actors: a singleplayer map's own path nodes when it has them, else
//! the runtime's baked navigation graph (the one the bots use). A* from the node nearest
//! the actor to the node nearest its goal; the actor steers at the route's next node.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy_ecs::prelude::Resource;
use std::sync::Arc;

/// The graph's nodes (feet positions) and, per node, the nodes a step reaches and its cost.
#[derive(Clone, Debug, Default)]
pub struct ActorNav {
    pub nodes: Vec<[f32; 3]>,
    pub adj: Vec<Vec<(u32, f32)>>,
    /// How far from the graph an actor or a goal may be and still use it.
    pub snap: f32,
}

/// The walk graph the runtime baked (the bots' navigation), shared into the authority.
#[derive(Clone, Debug, Resource)]
pub struct BakedActorNav(pub Arc<ActorNav>);

/// A* expansions one route may take.
const EXPAND_BUDGET: usize = 6000;

impl ActorNav {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The node nearest `feet`, preferring the same floor over one above or below.
    pub fn nearest(&self, feet: [f32; 3]) -> Option<usize> {
        self.nearest_k(feet, 1).first().copied()
    }

    /// The `k` nodes nearest `feet`, nearest first (same floor preferred).
    pub fn nearest_k(&self, feet: [f32; 3], k: usize) -> Vec<usize> {
        let mut scored: Vec<(f32, usize)> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                let dx = node[0] - feet[0];
                let dy = node[1] - feet[1];
                let dz = node[2] - feet[2];
                let flat = dx * dx + dy * dy;
                (flat <= self.snap * self.snap && dz.abs() <= 72.0)
                    .then_some((flat + 4.0 * dz * dz, index))
            })
            .collect();
        scored.sort_by(|a, b| a.0.total_cmp(&b.0));
        scored.into_iter().take(k).map(|(_, index)| index).collect()
    }

    /// How many nodes `node` reaches (its connected part of the graph), for diagnostics.
    pub fn reach_count(&self, node: usize) -> usize {
        let mut seen = vec![false; self.nodes.len()];
        let mut stack = vec![node];
        let mut count = 0;
        while let Some(at) = stack.pop() {
            if at >= seen.len() || std::mem::replace(&mut seen[at], true) {
                continue;
            }
            count += 1;
            stack.extend(self.adj[at].iter().map(|&(next, _)| next as usize));
        }
        count
    }

    /// The nodes from `from` to `to`, both ends snapped onto the graph; `None` when either
    /// end is off the graph or no route joins them within the budget.
    pub fn route(&self, from: [f32; 3], to: [f32; 3]) -> Option<Vec<[f32; 3]>> {
        self.route_between(self.nearest(from)?, self.nearest(to)?)
    }

    /// The nodes from node `start` to node `goal` (A*), or `None` when no route joins them
    /// within the budget.
    pub fn route_between(&self, start: usize, goal: usize) -> Option<Vec<[f32; 3]>> {
        if start >= self.nodes.len() || goal >= self.nodes.len() {
            return None;
        }
        if start == goal {
            return Some(vec![self.nodes[goal]]);
        }
        let h = |index: usize| distance(self.nodes[index], self.nodes[goal]);
        let mut best = vec![f32::INFINITY; self.nodes.len()];
        let mut came = vec![u32::MAX; self.nodes.len()];
        let mut open = BinaryHeap::new();
        best[start] = 0.0;
        open.push((Reverse(Ordered(h(start))), start as u32));
        let mut expanded = 0;
        while let Some((_, node)) = open.pop() {
            let node = node as usize;
            if node == goal {
                let mut path = vec![self.nodes[goal]];
                let mut at = goal;
                while came[at] != u32::MAX {
                    at = came[at] as usize;
                    path.push(self.nodes[at]);
                }
                path.reverse();
                return Some(path);
            }
            expanded += 1;
            if expanded > EXPAND_BUDGET {
                return None;
            }
            for &(next, cost) in &self.adj[node] {
                let next = next as usize;
                let through = best[node] + cost;
                if through < best[next] {
                    best[next] = through;
                    came[next] = node as u32;
                    open.push((Reverse(Ordered(through + h(next))), next as u32));
                }
            }
        }
        None
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[derive(Clone, Copy, PartialEq, PartialOrd)]
struct Ordered(f32);

impl Eq for Ordered {}

impl Ord for Ordered {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_around_a_gap() {
        // 0 - 1 - 2 in a line, and 0 - 3 - 2 a detour; 1 is cut off from 2.
        let nav = ActorNav {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [100.0, 0.0, 0.0],
                [200.0, 0.0, 0.0],
                [100.0, 100.0, 0.0],
            ],
            adj: vec![
                vec![(1, 100.0), (3, 141.0)],
                vec![(0, 100.0)],
                vec![(3, 141.0)],
                vec![(0, 141.0), (2, 141.0)],
            ],
            snap: 192.0,
        };
        let route = nav.route([0.0, 0.0, 0.0], [200.0, 0.0, 0.0]).unwrap();
        assert_eq!(
            route,
            vec![[0.0, 0.0, 0.0], [100.0, 100.0, 0.0], [200.0, 0.0, 0.0]]
        );
        assert!(nav.route([0.0, 0.0, 0.0], [900.0, 0.0, 0.0]).is_none());
    }
}
