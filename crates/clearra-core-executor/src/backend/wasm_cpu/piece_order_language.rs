use std::{collections::HashMap, sync::Arc};

use super::{buildup::BuildOrderGraph, mix_digest, WasmExactSearchError};

const EMPTY_REFERENCE: u32 = u32::MAX;
const INITIAL_BUCKET_COUNT: usize = 1024;
const COMPACT_PIECE_RANGE_EDGE_LIMIT: usize = u8::MAX as usize;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(super) struct CanonicalPieceEdge(u32);

impl CanonicalPieceEdge {
    const CHILD_BITS: u32 = 29;
    const CHILD_MASK: u32 = (1_u32 << Self::CHILD_BITS) - 1;

    fn new(child: u32, piece_code: u8) -> Option<Self> {
        let piece_index = piece_code.checked_sub(1)?;
        (piece_index < 7 && child <= Self::CHILD_MASK)
            .then_some(Self((u32::from(piece_index) << Self::CHILD_BITS) | child))
    }

    pub const fn child(self) -> u32 {
        self.0 & Self::CHILD_MASK
    }

    pub const fn piece_code(self) -> u8 {
        ((self.0 >> Self::CHILD_BITS) as u8) + 1
    }
}

const _: () = assert!(core::mem::size_of::<CanonicalPieceEdge>() == 4);

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct CanonicalPieceNode {
    edge_start: u32,
    next_same_bucket: u32,
    edge_count: u16,
    remaining_depth: u8,
    accepting: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PieceLanguageNodeView {
    pub accepting: bool,
    pub remaining_depth: u8,
}

const _: () = assert!(core::mem::size_of::<CanonicalPieceNode>() == 12);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CoverageCacheKey {
    language_id: u32,
    pattern_index_id: u32,
}

/// Query-local canonical piece-order language and coverage cache.
///
/// The cache is namespaced by one exact SearchProblem session, so its fixed
/// PieceSource, initial HoldAutomaton state, rules, and universe identity are
/// implicit. A language id is assigned only after exact edge-list comparison;
/// hashes never authorize merging.
#[derive(Default)]
pub(super) struct PieceOrderLanguageCache {
    nodes: Vec<CanonicalPieceNode>,
    edges: Vec<CanonicalPieceEdge>,
    piece_range_ends: Vec<[u8; 7]>,
    bucket_heads: Vec<u32>,
    graph_classes: Vec<u32>,
    edge_scratch: Vec<CanonicalPieceEdge>,
    interning_disabled: bool,
    coverage: HashMap<CoverageCacheKey, Arc<[u64]>>,
    seen_once: HashMap<CoverageCacheKey, ()>,
    coverage_hits: usize,
    coverage_misses: usize,
}

pub(super) enum CoverageCacheLookup {
    Hit(Arc<[u64]>),
    Miss { admit_after_compute: bool },
}

impl PieceOrderLanguageCache {
    pub fn canonicalize(&mut self, graph: &BuildOrderGraph) -> Result<u32, WasmExactSearchError> {
        self.ensure_buckets();
        if self.graph_classes.len() < graph.nodes.len() {
            self.graph_classes
                .try_reserve_exact(graph.nodes.len() - self.graph_classes.len())
                .map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_class_storage_unavailable",
                    )
                })?;
            self.graph_classes
                .resize(graph.nodes.len(), EMPTY_REFERENCE);
        }
        self.graph_classes[..graph.nodes.len()].fill(EMPTY_REFERENCE);

        for node_index in (0..graph.nodes.len()).rev() {
            let source = &graph.nodes[node_index];
            if !source.live {
                continue;
            }
            self.edge_scratch.clear();
            for edge in graph
                .piece_edges(node_index)
                .iter()
                .filter(|edge| graph.nodes[edge.to as usize].live)
            {
                let child = self.graph_classes[edge.to as usize];
                if child == EMPTY_REFERENCE {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_not_topological",
                    ));
                }
                self.edge_scratch.try_reserve(1).map_err(|_| {
                    WasmExactSearchError::InvalidProblem(
                        "wasm_piece_language_edge_scratch_unavailable",
                    )
                })?;
                self.edge_scratch.push(
                    CanonicalPieceEdge::new(child, piece_code(edge.piece)).ok_or(
                        WasmExactSearchError::InvalidProblem(
                            "wasm_piece_language_child_index_overflow",
                        ),
                    )?,
                );
            }
            self.edge_scratch.sort_unstable();
            self.edge_scratch.dedup();
            let remaining_depth = self.remaining_depth(source.accepting())?;
            let reference = self.intern_node(source.accepting(), remaining_depth)?;
            self.graph_classes[node_index] = reference;
        }

        self.graph_classes
            .get(graph.root as usize)
            .copied()
            .filter(|reference| *reference != EMPTY_REFERENCE)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_root_not_live",
            ))
    }

    pub fn coverage(
        &mut self,
        language_id: u32,
        pattern_index_id: u32,
    ) -> Result<CoverageCacheLookup, WasmExactSearchError> {
        let key = CoverageCacheKey {
            language_id,
            pattern_index_id,
        };
        if let Some(words) = self.coverage.get(&key) {
            self.coverage_hits = self.coverage_hits.saturating_add(1);
            return Ok(CoverageCacheLookup::Hit(Arc::clone(words)));
        }
        self.coverage_misses = self.coverage_misses.saturating_add(1);
        let admit_after_compute = self.seen_once.remove(&key).is_some();
        if !admit_after_compute {
            self.seen_once.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_piece_language_admission_storage_unavailable",
                )
            })?;
            self.seen_once.insert(key, ());
        }
        Ok(CoverageCacheLookup::Miss {
            admit_after_compute,
        })
    }

    pub fn insert_coverage(
        &mut self,
        language_id: u32,
        pattern_index_id: u32,
        local_words: &[u64],
    ) -> Result<(), WasmExactSearchError> {
        let key = CoverageCacheKey {
            language_id,
            pattern_index_id,
        };
        self.coverage.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_coverage_cache_unavailable")
        })?;
        let mut words = Vec::new();
        words.try_reserve_exact(local_words.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_coverage_words_unavailable")
        })?;
        words.extend_from_slice(local_words);
        self.coverage.insert(key, words.into());
        Ok(())
    }

    pub const fn coverage_hits(&self) -> usize {
        self.coverage_hits
    }

    pub const fn coverage_misses(&self) -> usize {
        self.coverage_misses
    }

    pub fn node(&self, reference: u32) -> Option<PieceLanguageNodeView> {
        self.nodes
            .get(reference as usize)
            .map(|node| PieceLanguageNodeView {
                accepting: node.accepting,
                remaining_depth: node.remaining_depth,
            })
    }

    pub fn edges_for_piece(&self, reference: u32, piece_code: u8) -> Option<&[CanonicalPieceEdge]> {
        let piece_index = usize::from(piece_code.checked_sub(1)?);
        if piece_index >= 7 {
            return None;
        }
        let node = self.nodes.get(reference as usize).copied()?;
        let edges = self.node_edges(node);
        if edges.len() > COMPACT_PIECE_RANGE_EDGE_LIMIT {
            let relative_start = edges.partition_point(|edge| edge.piece_code() < piece_code);
            let relative_end = edges.partition_point(|edge| edge.piece_code() <= piece_code);
            return edges.get(relative_start..relative_end);
        }
        let range_ends = self.piece_range_ends.get(reference as usize)?;
        let relative_start = if piece_index == 0 {
            0
        } else {
            usize::from(range_ends[piece_index - 1])
        };
        let relative_end = usize::from(range_ends[piece_index]);
        let edge_start = node.edge_start as usize;
        self.edges
            .get(edge_start + relative_start..edge_start + relative_end)
    }

    pub fn edge_count(&self, reference: u32) -> Option<usize> {
        self.nodes
            .get(reference as usize)
            .map(|node| usize::from(node.edge_count))
    }

    pub fn edge(&self, reference: u32, index: usize) -> Option<CanonicalPieceEdge> {
        let node = self.nodes.get(reference as usize).copied()?;
        (index < usize::from(node.edge_count)).then(|| self.edges[node.edge_start as usize + index])
    }

    pub fn union_roots(
        &mut self,
        left: Option<u32>,
        right: u32,
    ) -> Result<u32, WasmExactSearchError> {
        let Some(left) = left else {
            return Ok(right);
        };
        if left == right {
            return Ok(left);
        }
        let left_node =
            *self
                .nodes
                .get(left as usize)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_piece_language_union_node_out_of_range",
                ))?;
        let right_node =
            *self
                .nodes
                .get(right as usize)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_piece_language_union_node_out_of_range",
                ))?;
        if left_node.remaining_depth != right_node.remaining_depth {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_union_depth_mismatch",
            ));
        }
        let left_edges = self.node_edges(left_node).to_vec();
        let right_edges = self.node_edges(right_node).to_vec();
        self.edge_scratch.clear();
        self.edge_scratch
            .try_reserve(left_edges.len().saturating_add(right_edges.len()))
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_piece_language_edge_scratch_unavailable")
            })?;
        self.edge_scratch.extend_from_slice(&left_edges);
        self.edge_scratch.extend_from_slice(&right_edges);
        self.edge_scratch.sort_unstable();
        self.edge_scratch.dedup();
        self.intern_node(
            left_node.accepting || right_node.accepting,
            left_node.remaining_depth,
        )
    }

    pub fn clear_coverage_caches(&mut self) {
        self.coverage.clear();
        self.seen_once.clear();
    }

    pub fn retained_bytes(&self) -> usize {
        self.nodes.capacity() * core::mem::size_of::<CanonicalPieceNode>()
            + self.edges.capacity() * core::mem::size_of::<CanonicalPieceEdge>()
            + self.piece_range_ends.capacity() * core::mem::size_of::<[u8; 7]>()
            + self.bucket_heads.capacity() * core::mem::size_of::<u32>()
            + self.graph_classes.capacity() * core::mem::size_of::<u32>()
            + self.edge_scratch.capacity() * core::mem::size_of::<CanonicalPieceEdge>()
            + self.coverage.capacity()
                * (core::mem::size_of::<CoverageCacheKey>() + core::mem::size_of::<Arc<[u64]>>())
            + self.seen_once.capacity() * core::mem::size_of::<CoverageCacheKey>()
            + self
                .coverage
                .values()
                .map(|words| words.len() * core::mem::size_of::<u64>())
                .sum::<usize>()
    }

    pub fn local_live_bytes(&self) -> usize {
        self.nodes.len() * core::mem::size_of::<CanonicalPieceNode>()
            + self.edges.len() * core::mem::size_of::<CanonicalPieceEdge>()
            + self.piece_range_ends.len() * core::mem::size_of::<[u8; 7]>()
            + self.coverage.len()
                * (core::mem::size_of::<CoverageCacheKey>() + core::mem::size_of::<Arc<[u64]>>())
            + self.seen_once.len() * core::mem::size_of::<CoverageCacheKey>()
    }

    pub fn clear_retain_capacity(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.piece_range_ends.clear();
        self.bucket_heads.fill(EMPTY_REFERENCE);
        self.graph_classes.fill(EMPTY_REFERENCE);
        self.edge_scratch.clear();
        self.coverage.clear();
        self.seen_once.clear();
        self.interning_disabled = false;
    }

    fn remaining_depth(&self, accepting: bool) -> Result<u8, WasmExactSearchError> {
        if accepting {
            if !self.edge_scratch.is_empty() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_piece_language_accepting_node_has_edges",
                ));
            }
            return Ok(0);
        }
        let Some(first) = self.edge_scratch.first() else {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_live_node_has_no_edges",
            ));
        };
        let child_depth = self.nodes[first.child() as usize].remaining_depth;
        if self
            .edge_scratch
            .iter()
            .any(|edge| self.nodes[edge.child() as usize].remaining_depth != child_depth)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_depth_mismatch",
            ));
        }
        child_depth
            .checked_add(1)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_piece_language_depth_overflow",
            ))
    }

    fn intern_node(
        &mut self,
        accepting: bool,
        remaining_depth: u8,
    ) -> Result<u32, WasmExactSearchError> {
        let hash = language_node_hash(accepting, remaining_depth, &self.edge_scratch);
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            let mut reference = self.bucket_heads[bucket];
            while reference != EMPTY_REFERENCE {
                let node = self.nodes[reference as usize];
                if node.accepting == accepting
                    && node.remaining_depth == remaining_depth
                    && self.node_edges(node) == self.edge_scratch.as_slice()
                {
                    return Ok(reference);
                }
                reference = node.next_same_bucket;
            }
        }

        let reference = u32::try_from(self.nodes.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_node_index_overflow")
        })?;
        let edge_start = u32::try_from(self.edges.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_edge_index_overflow")
        })?;
        let edge_count = u16::try_from(self.edge_scratch.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_edge_count_overflow")
        })?;
        self.nodes.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_node_storage_unavailable")
        })?;
        self.piece_range_ends.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_piece_language_node_storage_unavailable")
        })?;
        self.edges
            .try_reserve_exact(self.edge_scratch.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_piece_language_edge_storage_unavailable")
            })?;
        let next_same_bucket = if self.interning_disabled {
            EMPTY_REFERENCE
        } else {
            self.bucket_heads[hash as usize & (self.bucket_heads.len() - 1)]
        };
        let piece_range_ends = if self.edge_scratch.len() <= COMPACT_PIECE_RANGE_EDGE_LIMIT {
            compact_piece_range_ends(&self.edge_scratch)
        } else {
            [0; 7]
        };
        self.edges.extend_from_slice(&self.edge_scratch);
        self.nodes.push(CanonicalPieceNode {
            edge_start,
            next_same_bucket,
            edge_count,
            remaining_depth,
            accepting,
        });
        self.piece_range_ends.push(piece_range_ends);
        if !self.interning_disabled {
            let bucket = hash as usize & (self.bucket_heads.len() - 1);
            self.bucket_heads[bucket] = reference;
            self.grow_buckets_if_needed();
        }
        Ok(reference)
    }

    fn node_edges(&self, node: CanonicalPieceNode) -> &[CanonicalPieceEdge] {
        let start = node.edge_start as usize;
        &self.edges[start..start + node.edge_count as usize]
    }

    fn ensure_buckets(&mut self) {
        if !self.bucket_heads.is_empty() || self.interning_disabled {
            return;
        }
        if self
            .bucket_heads
            .try_reserve_exact(INITIAL_BUCKET_COUNT)
            .is_err()
        {
            self.interning_disabled = true;
            return;
        }
        self.bucket_heads
            .resize(INITIAL_BUCKET_COUNT, EMPTY_REFERENCE);
    }

    fn grow_buckets_if_needed(&mut self) {
        if self.interning_disabled
            || self.nodes.len().saturating_mul(4) < self.bucket_heads.len().saturating_mul(3)
        {
            return;
        }
        let Some(capacity) = self.bucket_heads.len().checked_mul(2) else {
            self.interning_disabled = true;
            return;
        };
        let mut replacement = Vec::new();
        if replacement.try_reserve_exact(capacity).is_err() {
            self.interning_disabled = true;
            return;
        }
        replacement.resize(capacity, EMPTY_REFERENCE);
        for reference in 0..self.nodes.len() {
            let node = self.nodes[reference];
            let hash =
                language_node_hash(node.accepting, node.remaining_depth, self.node_edges(node));
            let bucket = hash as usize & (capacity - 1);
            self.nodes[reference].next_same_bucket = replacement[bucket];
            replacement[bucket] = reference as u32;
        }
        self.bucket_heads = replacement;
    }
}

fn compact_piece_range_ends(edges: &[CanonicalPieceEdge]) -> [u8; 7] {
    debug_assert!(edges.len() <= COMPACT_PIECE_RANGE_EDGE_LIMIT);
    let mut range_ends = [0_u8; 7];
    let mut cursor = 0usize;
    for piece_code in 1..=7 {
        while cursor < edges.len() && edges[cursor].piece_code() <= piece_code {
            cursor += 1;
        }
        range_ends[piece_code as usize - 1] =
            u8::try_from(cursor).expect("compact piece ranges fit in u8");
    }
    range_ends
}

fn language_node_hash(accepting: bool, remaining_depth: u8, edges: &[CanonicalPieceEdge]) -> u64 {
    let mut hash = mix_digest(0, u64::from(accepting));
    hash = mix_digest(hash, u64::from(remaining_depth));
    hash = mix_digest(hash, edges.len() as u64);
    for edge in edges {
        hash = mix_digest(hash, u64::from(edge.piece_code()));
        hash = mix_digest(hash, u64::from(edge.child()));
    }
    hash
}

const fn piece_code(piece: clearra_core_domain::piece::piece_kind::PieceKind) -> u8 {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    match piece {
        PieceKind::I => 1,
        PieceKind::O => 2,
        PieceKind::T => 3,
        PieceKind::S => 4,
        PieceKind::Z => 5,
        PieceKind::J => 6,
        PieceKind::L => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::{CanonicalPieceEdge, PieceOrderLanguageCache};

    #[test]
    fn piece_ranges_remain_exact_above_the_compact_edge_limit() {
        let mut cache = PieceOrderLanguageCache::default();
        cache.ensure_buckets();
        for piece_code in 1..=3 {
            for child in 0..128 {
                cache
                    .edge_scratch
                    .push(CanonicalPieceEdge::new(child, piece_code).expect("edge"));
            }
        }

        let root = cache.intern_node(false, 1).expect("wide language node");

        assert_eq!(cache.edge_count(root), Some(384));
        for piece_code in 1..=3 {
            let edges = cache
                .edges_for_piece(root, piece_code)
                .expect("piece range");
            assert_eq!(edges.len(), 128);
            assert!(edges.iter().all(|edge| edge.piece_code() == piece_code));
        }
        for piece_code in 4..=7 {
            assert_eq!(
                cache
                    .edges_for_piece(root, piece_code)
                    .expect("empty piece range")
                    .len(),
                0
            );
        }
    }
}
