//! Walk routes for AI actors: a singleplayer map's own path nodes when it has them, else
//! the runtime's baked navigation graph (the one the bots use). A* from the node nearest
//! the actor to the node nearest its goal; the actor steers at the route's next node.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};

use bevy_ecs::prelude::Resource;
use std::sync::Arc;

/// Links scripts closed: BO1 runs `disconnectpaths` on a closed door's pieces and on
/// debris, and `connectpaths` once they open. Counted per directed link, so a doorway
/// two pieces close opens only when both have opened.
#[derive(Clone, Debug, Default)]
pub struct NavLinkBlocks {
    count: HashMap<(u32, u32), u32>,
    by_entity: BTreeMap<u64, Vec<(u32, u32)>>,
    /// Entities a script has run `connectpaths` on.
    connected: std::collections::BTreeSet<u64>,
    /// The map's dynamic-path brushes have closed their links (once, at the start).
    dynamic_closed: bool,
}

impl NavLinkBlocks {
    /// True the first time only: the map's dynamic-path brushes are due to close.
    pub fn take_dynamic_pass(&mut self) -> bool {
        !std::mem::replace(&mut self.dynamic_closed, true)
    }

    /// A script has opened the links through `entity` (`connectpaths`).
    pub fn was_connected(&self, entity: u64) -> bool {
        self.connected.contains(&entity)
    }

    /// Closes `links` for `entity`; nothing when it already holds some closed.
    /// Returns how many it closed.
    pub fn disconnect(&mut self, entity: u64, links: Vec<(u32, u32)>) -> usize {
        if self.by_entity.contains_key(&entity) || links.is_empty() {
            return 0;
        }
        for link in &links {
            *self.count.entry(*link).or_default() += 1;
        }
        let closed = links.len();
        self.by_entity.insert(entity, links);
        closed
    }

    /// Reopens what `entity` closed; returns how many links it held.
    pub fn connect(&mut self, entity: u64) -> usize {
        self.connected.insert(entity);
        let Some(links) = self.by_entity.remove(&entity) else {
            return 0;
        };
        for link in &links {
            if let Some(count) = self.count.get_mut(link) {
                *count -= 1;
                if *count == 0 {
                    self.count.remove(link);
                }
            }
        }
        links.len()
    }

    pub fn blocks(&self, from: u32, to: u32) -> bool {
        self.count.contains_key(&(from, to))
    }

    pub fn is_empty(&self) -> bool {
        self.count.is_empty()
    }
}

/// Heights above the nodes (feet) at which a step between them is tested against a
/// closing entity: knee and chest of a zombie.
const LINK_TEST_HEIGHTS: [f32; 2] = [20.0, 50.0];

/// The graph's nodes (feet positions) and, per node, the nodes a step reaches and its cost.
#[derive(Clone, Debug, Default)]
pub struct ActorNav {
    pub nodes: Vec<[f32; 3]>,
    pub adj: Vec<Vec<(u32, f32)>>,
    /// How far from the graph an actor or a goal may be and still use it.
    pub snap: f32,
    /// Directed links that are traversals (a jump down, a climb up): not walked but
    /// moved over from one end to the other.
    pub traversals: std::collections::HashSet<(u32, u32)>,
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

    /// The directed links whose step (at a zombie's knee or chest height) passes through
    /// the world box `mins`..`maxs`.
    pub fn links_through(&self, mins: [f32; 3], maxs: [f32; 3]) -> Vec<(u32, u32)> {
        let mut links = Vec::new();
        for (from, steps) in self.adj.iter().enumerate() {
            for &(to, _) in steps {
                let (a, b) = (self.nodes[from], self.nodes[to as usize]);
                let crosses = LINK_TEST_HEIGHTS.iter().any(|&h| {
                    segment_hits_box([a[0], a[1], a[2] + h], [b[0], b[1], b[2] + h], mins, maxs)
                });
                if crosses {
                    links.push((from as u32, to));
                }
            }
        }
        links
    }

    /// The nodes from node `start` to node `goal` (A*), or `None` when no route joins them
    /// within the budget.
    pub fn route_between(&self, start: usize, goal: usize) -> Option<Vec<[f32; 3]>> {
        self.route_between_open(start, goal, &NavLinkBlocks::default())
    }

    /// [`Self::route_between`] over the links `blocks` leaves open.
    pub fn route_between_open(
        &self,
        start: usize,
        goal: usize,
        blocks: &NavLinkBlocks,
    ) -> Option<Vec<[f32; 3]>> {
        self.route_nodes_open(start, goal, blocks)
            .map(|path| path.into_iter().map(|node| self.nodes[node]).collect())
    }

    /// The node indices of [`Self::route_between_open`]'s route.
    pub fn route_nodes_open(
        &self,
        start: usize,
        goal: usize,
        blocks: &NavLinkBlocks,
    ) -> Option<Vec<usize>> {
        self.route_nodes_from(&[(start, 0.0)], goal, blocks)
    }

    /// The cheapest route to `goal` from any of `starts`, each with what getting onto it
    /// costs. An actor between two nodes enters at the one that makes the whole way
    /// shortest, not the nearest: on a staircase the nearest can be a dead end that only
    /// leads back up.
    pub fn route_nodes_from(
        &self,
        starts: &[(usize, f32)],
        goal: usize,
        blocks: &NavLinkBlocks,
    ) -> Option<Vec<usize>> {
        if goal >= self.nodes.len() || starts.iter().any(|&(start, _)| start >= self.nodes.len()) {
            return None;
        }
        let h = |index: usize| distance(self.nodes[index], self.nodes[goal]);
        let mut best = vec![f32::INFINITY; self.nodes.len()];
        let mut came = vec![u32::MAX; self.nodes.len()];
        let mut open = BinaryHeap::new();
        for &(start, cost) in starts {
            if cost < best[start] {
                best[start] = cost;
                open.push((Reverse(Ordered(cost + h(start))), start as u32));
            }
        }
        let mut expanded = 0;
        while let Some((_, node)) = open.pop() {
            let node = node as usize;
            if node == goal {
                let mut path = vec![goal];
                let mut at = goal;
                while came[at] != u32::MAX {
                    at = came[at] as usize;
                    path.push(at);
                }
                path.reverse();
                return Some(path);
            }
            expanded += 1;
            if expanded > EXPAND_BUDGET {
                return None;
            }
            for &(next, cost) in &self.adj[node] {
                if blocks.blocks(node as u32, next) {
                    continue;
                }
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

/// Whether the segment `a`..`b` meets the box `mins`..`maxs` (slab test; a segment
/// starting inside counts).
fn segment_hits_box(a: [f32; 3], b: [f32; 3], mins: [f32; 3], maxs: [f32; 3]) -> bool {
    let (mut enter, mut exit) = (0.0f32, 1.0f32);
    for axis in 0..3 {
        let d = b[axis] - a[axis];
        if d.abs() < 1e-6 {
            if a[axis] < mins[axis] || a[axis] > maxs[axis] {
                return false;
            }
            continue;
        }
        let t0 = (mins[axis] - a[axis]) / d;
        let t1 = (maxs[axis] - a[axis]) / d;
        enter = enter.max(t0.min(t1));
        exit = exit.min(t0.max(t1));
        if enter > exit {
            return false;
        }
    }
    true
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
            traversals: Default::default(),
        };
        let route = nav.route([0.0, 0.0, 0.0], [200.0, 0.0, 0.0]).unwrap();
        assert_eq!(
            route,
            vec![[0.0, 0.0, 0.0], [100.0, 100.0, 0.0], [200.0, 0.0, 0.0]]
        );
        assert!(nav.route([0.0, 0.0, 0.0], [900.0, 0.0, 0.0]).is_none());
    }

    /// Kino's lobby stairs: halfway down, the nearest node is a dead end that only leads
    /// back up to the landing. Entering at it sent the zombie up, then down again on the
    /// next plan, for ever; the route now enters at the node that makes the way shortest.
    #[test]
    fn enters_the_route_where_the_whole_way_is_shortest() {
        let nav = ActorNav {
            nodes: vec![
                [-370.0, -984.0, 186.0],  // 0: dead end on the stairs
                [-362.0, -821.0, 253.0],  // 1: the landing
                [-298.0, -1029.0, 210.0], // 2: further down the stairs
                [-240.0, -1426.0, 112.0], // 3: the lobby floor
            ],
            adj: vec![
                vec![(1, 164.0)],
                vec![(0, 164.0), (2, 218.0)],
                vec![(1, 218.0), (3, 400.0)],
                vec![(2, 400.0)],
            ],
            snap: 512.0,
            traversals: Default::default(),
        };
        let blocks = NavLinkBlocks::default();
        // Standing between the dead end (40 away) and the next node down (140 away).
        let route = nav
            .route_nodes_from(&[(0, 40.0), (2, 140.0)], 3, &blocks)
            .unwrap();
        assert_eq!(route, vec![2, 3]);
        // From the dead end alone the only way is back up first.
        assert_eq!(nav.route_nodes_open(0, 3, &blocks).unwrap(), vec![0, 1, 2, 3]);
    }

    /// A door across the straight way: closed, the route goes round it (or there is
    /// none); opened, the straight way is back.
    #[test]
    fn closed_door_blocks_its_links_until_opened() {
        let nav = ActorNav {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [200.0, 0.0, 0.0],
                [100.0, 300.0, 0.0],
            ],
            adj: vec![
                vec![(1, 200.0), (2, 316.0)],
                vec![(0, 200.0), (2, 316.0)],
                vec![(0, 316.0), (1, 316.0)],
            ],
            snap: 192.0,
            traversals: Default::default(),
        };
        // A thin door at x=100 across y -50..50, floor to 100 high.
        let links = nav.links_through([96.0, -50.0, 0.0], [104.0, 50.0, 100.0]);
        assert_eq!(links, vec![(0, 1), (1, 0)]);
        let mut blocks = NavLinkBlocks::default();
        assert_eq!(blocks.disconnect(7, links.clone()), 2);
        assert_eq!(blocks.disconnect(7, links), 0, "a second close is not counted");
        let detour = nav.route_between_open(0, 1, &blocks).unwrap();
        assert_eq!(detour.len(), 3, "around through node 2: {detour:?}");
        assert_eq!(blocks.connect(7), 2);
        assert!(blocks.is_empty());
        assert_eq!(nav.route_between_open(0, 1, &blocks).unwrap().len(), 2);
    }
}
