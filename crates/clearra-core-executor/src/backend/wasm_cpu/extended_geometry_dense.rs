use super::{
    extended_geometry::GeometryTarget,
    extended_geometry_component::{compile_dense_component_plan, ExtendedComponentPlanResult},
    extended_geometry_domain::{
        feasible_piece_mask, ExtendedDomainResult, ExtendedDomainWorkspace,
    },
    extended_inverse_catalog::{DenseExtendedGeometryCatalog, ExtendedInverseCatalog},
    geometry_family::{GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID},
    mix_digest, piece_index,
};

const NO_ROW: u32 = u32::MAX;
const UNION_LEVEL_COUNT: usize = 32;
const MEMO_ENTRY_CHUNK_CAPACITY: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DenseResidualKey {
    remaining: u64,
    packed_counts: u64,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct DenseResidualEntry {
    remaining: u64,
    packed_counts: u64,
    suffix_family: u32,
    reserved: u32,
}

const _: () = assert!(core::mem::size_of::<DenseResidualEntry>() == 24);

struct DenseResidualChunk {
    entries: Vec<DenseResidualEntry>,
    next: Vec<u32>,
}

struct DenseResidualMemo {
    bucket_heads: Vec<u32>,
    chunks: Vec<DenseResidualChunk>,
    entry_count: u32,
    insertion_disabled: bool,
}

impl DenseResidualMemo {
    fn new(target_depth: u8) -> Self {
        let bucket_count = match target_depth {
            0..=7 => 1 << 14,
            8..=12 => 1 << 17,
            _ => 1 << 18,
        };
        let mut bucket_heads = Vec::new();
        let insertion_disabled = bucket_heads.try_reserve_exact(bucket_count).is_err();
        if !insertion_disabled {
            bucket_heads.resize(bucket_count, 0);
        }
        Self {
            bucket_heads,
            chunks: Vec::new(),
            entry_count: 0,
            insertion_disabled,
        }
    }

    fn lookup(&self, key: DenseResidualKey) -> Option<u32> {
        if self.bucket_heads.is_empty() {
            return None;
        }
        let bucket = dense_residual_hash(key) as usize & (self.bucket_heads.len() - 1);
        let mut link = self.bucket_heads[bucket];
        while link != 0 {
            let index = link - 1;
            let entry = self.entry(index)?;
            if entry.remaining == key.remaining && entry.packed_counts == key.packed_counts {
                return Some(entry.suffix_family);
            }
            link = self.next(index)?;
        }
        None
    }

    fn insert(&mut self, key: DenseResidualKey, suffix_family: u32) {
        if self.insertion_disabled || self.lookup(key).is_some() || self.entry_count == u32::MAX {
            return;
        }
        let needs_chunk = self
            .chunks
            .last()
            .is_none_or(|chunk| chunk.entries.len() == MEMO_ENTRY_CHUNK_CAPACITY);
        if needs_chunk && !self.push_chunk() {
            self.insertion_disabled = true;
            return;
        }
        let bucket = dense_residual_hash(key) as usize & (self.bucket_heads.len() - 1);
        let previous = self.bucket_heads[bucket];
        let Some(chunk) = self.chunks.last_mut() else {
            self.insertion_disabled = true;
            return;
        };
        chunk.entries.push(DenseResidualEntry {
            remaining: key.remaining,
            packed_counts: key.packed_counts,
            suffix_family,
            reserved: 0,
        });
        chunk.next.push(previous);
        self.entry_count += 1;
        self.bucket_heads[bucket] = self.entry_count;
    }

    fn push_chunk(&mut self) -> bool {
        let mut entries = Vec::new();
        let mut next = Vec::new();
        if entries
            .try_reserve_exact(MEMO_ENTRY_CHUNK_CAPACITY)
            .is_err()
            || next.try_reserve_exact(MEMO_ENTRY_CHUNK_CAPACITY).is_err()
            || self.chunks.try_reserve(1).is_err()
        {
            return false;
        }
        self.chunks.push(DenseResidualChunk { entries, next });
        true
    }

    fn entry(&self, index: u32) -> Option<DenseResidualEntry> {
        let index = index as usize;
        self.chunks
            .get(index / MEMO_ENTRY_CHUNK_CAPACITY)?
            .entries
            .get(index % MEMO_ENTRY_CHUNK_CAPACITY)
            .copied()
    }

    fn next(&self, index: u32) -> Option<u32> {
        let index = index as usize;
        self.chunks
            .get(index / MEMO_ENTRY_CHUNK_CAPACITY)?
            .next
            .get(index % MEMO_ENTRY_CHUNK_CAPACITY)
            .copied()
    }

    fn retained_bytes(&self) -> usize {
        self.bucket_heads.capacity() * core::mem::size_of::<u32>()
            + self.chunks.capacity() * core::mem::size_of::<DenseResidualChunk>()
            + self
                .chunks
                .iter()
                .map(|chunk| {
                    chunk.entries.capacity() * core::mem::size_of::<DenseResidualEntry>()
                        + chunk.next.capacity() * core::mem::size_of::<u32>()
                })
                .sum::<usize>()
    }
}

fn dense_residual_hash(key: DenseResidualKey) -> u64 {
    mix_digest(mix_digest(0, key.remaining), key.packed_counts)
}

#[derive(Clone, Copy)]
struct DenseCompileFrame {
    remaining: u64,
    key: DenseResidualKey,
    support_cursor: usize,
    support_end: usize,
    support_scratch_checkpoint: usize,
    depth: u8,
    entered: bool,
    chosen_row: u32,
    union_levels: [u32; UNION_LEVEL_COUNT],
}

impl DenseCompileFrame {
    fn root(remaining: u64) -> Self {
        Self::child(remaining, 0, NO_ROW, 0)
    }

    fn child(
        remaining: u64,
        depth: u8,
        chosen_row: u32,
        support_scratch_checkpoint: usize,
    ) -> Self {
        Self {
            remaining,
            key: DenseResidualKey {
                remaining,
                packed_counts: 0,
            },
            support_cursor: 0,
            support_end: 0,
            support_scratch_checkpoint,
            depth,
            entered: false,
            chosen_row,
            union_levels: [FAMILY_INVALID; UNION_LEVEL_COUNT],
        }
    }
}

pub(super) enum DenseCompileAdvance {
    Pending,
    Complete,
    ResourceIncomplete,
}

pub(super) struct DenseExtendedFamilyCompiler {
    targets: Vec<GeometryTarget>,
    target_counts: Vec<[u8; 7]>,
    target_depth: u8,
    used_counts: [u8; 7],
    stack: Vec<DenseCompileFrame>,
    support_scratch: Vec<u32>,
    residual_memo: DenseResidualMemo,
    domain_workspace: ExtendedDomainWorkspace,
    family: GeometrySolutionFamily,
    root_family: u32,
    expanded_nodes: usize,
    peak_frontier: usize,
    domain_pruned_states: usize,
    hall_pruned_states: usize,
    column_pruned_states: usize,
    component_pruned_states: usize,
    component_compositions: usize,
}

impl DenseExtendedFamilyCompiler {
    pub fn new(required_cells: u64, targets: Vec<GeometryTarget>) -> Self {
        let target_depth = targets
            .first()
            .map_or(0, |target| target.counts.iter().copied().sum());
        let target_counts = targets.iter().map(|target| target.counts).collect();
        Self {
            targets,
            target_counts,
            target_depth,
            used_counts: [0; 7],
            stack: vec![DenseCompileFrame::root(required_cells)],
            support_scratch: Vec::new(),
            residual_memo: DenseResidualMemo::new(target_depth),
            domain_workspace: ExtendedDomainWorkspace::new(),
            family: GeometrySolutionFamily::new(),
            root_family: FAMILY_INVALID,
            expanded_nodes: 0,
            peak_frontier: 1,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_pruned_states: 0,
            component_compositions: 0,
        }
    }

    pub fn advance(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        dense_catalog: &DenseExtendedGeometryCatalog,
    ) -> DenseCompileAdvance {
        if self.stack.is_empty() {
            return DenseCompileAdvance::Complete;
        }
        let top_index = self.stack.len() - 1;
        if !self.stack[top_index].entered {
            let remaining = self.stack[top_index].remaining;
            let key = DenseResidualKey {
                remaining,
                packed_counts: pack_piece_counts(self.used_counts),
            };
            self.stack[top_index].key = key;
            if let Some(family) = self.residual_memo.lookup(key) {
                return self.finish_top(catalog, family, false);
            }

            self.expanded_nodes = self.expanded_nodes.saturating_add(1);
            self.peak_frontier = self.peak_frontier.max(self.stack.len());
            let depth = self.stack[top_index].depth;
            if remaining == 0 {
                let family = if depth == self.target_depth && self.completed_target() {
                    FAMILY_EMPTY
                } else {
                    FAMILY_INVALID
                };
                return self.finish_top(catalog, family, true);
            }
            if depth >= self.target_depth
                || remaining.count_ones() as usize
                    != usize::from(self.target_depth.saturating_sub(depth)) * 4
            {
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }

            let feasible_mask = feasible_piece_mask(&self.target_counts, self.used_counts);
            if self.target_depth >= 7 {
                match compile_dense_component_plan(
                    catalog,
                    dense_catalog,
                    remaining,
                    self.used_counts,
                    &self.target_counts,
                    feasible_mask,
                    &mut self.family,
                ) {
                    ExtendedComponentPlanResult::NotApplicable => {}
                    ExtendedComponentPlanResult::Impossible => {
                        self.component_pruned_states =
                            self.component_pruned_states.saturating_add(1);
                        return self.finish_top(catalog, FAMILY_INVALID, true);
                    }
                    ExtendedComponentPlanResult::StorageUnavailable => {
                        return DenseCompileAdvance::ResourceIncomplete;
                    }
                    ExtendedComponentPlanResult::Complete {
                        family,
                        expanded_nodes,
                    } => {
                        self.expanded_nodes = self.expanded_nodes.saturating_add(expanded_nodes);
                        self.component_compositions = self.component_compositions.saturating_add(1);
                        return self.finish_top(catalog, family, true);
                    }
                }
            }

            let Some(physical_remaining) = dense_catalog.decode(remaining) else {
                return DenseCompileAdvance::ResourceIncomplete;
            };
            let domain = match self.domain_workspace.compile(
                catalog,
                physical_remaining,
                self.used_counts,
                &self.target_counts,
                usize::from(depth),
            ) {
                ExtendedDomainResult::Supported(domain) => domain,
                ExtendedDomainResult::Empty => {
                    self.domain_pruned_states = self.domain_pruned_states.saturating_add(1);
                    return self.finish_top(catalog, FAMILY_INVALID, true);
                }
                ExtendedDomainResult::HallImpossible => {
                    self.hall_pruned_states = self.hall_pruned_states.saturating_add(1);
                    return self.finish_top(catalog, FAMILY_INVALID, true);
                }
                ExtendedDomainResult::ProjectionImpossible => {
                    self.column_pruned_states = self.column_pruned_states.saturating_add(1);
                    return self.finish_top(catalog, FAMILY_INVALID, true);
                }
            };

            let support = catalog.support(domain.pivot_cell);
            if self.support_scratch.try_reserve(support.len()).is_err() {
                return DenseCompileAdvance::ResourceIncomplete;
            }
            let start = self.support_scratch.len();
            for row_id in support.iter().copied() {
                if domain.row_allowed(catalog, row_id, physical_remaining, feasible_mask) {
                    self.support_scratch.push(row_id);
                }
            }
            if self.support_scratch.len() == start {
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }
            let frame = &mut self.stack[top_index];
            frame.support_cursor = start;
            frame.support_end = self.support_scratch.len();
            frame.entered = true;
            return DenseCompileAdvance::Pending;
        }

        let frame = self.stack[top_index];
        if frame.support_cursor < frame.support_end {
            let row_id = self.support_scratch[frame.support_cursor];
            self.stack[top_index].support_cursor += 1;
            let row = catalog.skeleton(row_id);
            let Some(row_cells) = dense_catalog.skeleton_cells(row_id) else {
                return DenseCompileAdvance::ResourceIncomplete;
            };
            if row_cells & frame.remaining != row_cells {
                return DenseCompileAdvance::ResourceIncomplete;
            }
            self.used_counts[piece_index(row.piece)] += 1;
            if self.stack.try_reserve(1).is_err() {
                self.used_counts[piece_index(row.piece)] -= 1;
                return DenseCompileAdvance::ResourceIncomplete;
            }
            self.stack.push(DenseCompileFrame::child(
                frame.remaining ^ row_cells,
                frame.depth + 1,
                row_id,
                self.support_scratch.len(),
            ));
            return DenseCompileAdvance::Pending;
        }

        let Some(family) = self.finalize_union(top_index) else {
            return DenseCompileAdvance::ResourceIncomplete;
        };
        self.finish_top(catalog, family, true)
    }

    fn finish_top(
        &mut self,
        catalog: &ExtendedInverseCatalog,
        suffix_family: u32,
        memoize: bool,
    ) -> DenseCompileAdvance {
        let frame = self.stack.pop().expect("dense extended frame exists");
        self.support_scratch
            .truncate(frame.support_scratch_checkpoint);
        if memoize {
            self.residual_memo.insert(frame.key, suffix_family);
        }
        if frame.chosen_row == NO_ROW {
            self.root_family = suffix_family;
            return DenseCompileAdvance::Complete;
        }
        let piece = piece_index(catalog.skeleton(frame.chosen_row).piece);
        self.used_counts[piece] -= 1;
        if suffix_family == FAMILY_INVALID {
            return DenseCompileAdvance::Pending;
        }
        let Some(branch) = self.family.append(frame.chosen_row, suffix_family) else {
            return DenseCompileAdvance::ResourceIncomplete;
        };
        if self.add_branch_to_parent(branch) {
            DenseCompileAdvance::Pending
        } else {
            DenseCompileAdvance::ResourceIncomplete
        }
    }

    fn add_branch_to_parent(&mut self, mut branch: u32) -> bool {
        let Some(parent_index) = self.stack.len().checked_sub(1) else {
            return false;
        };
        for level in 0..UNION_LEVEL_COUNT {
            let existing = self.stack[parent_index].union_levels[level];
            if existing == FAMILY_INVALID {
                self.stack[parent_index].union_levels[level] = branch;
                return true;
            }
            self.stack[parent_index].union_levels[level] = FAMILY_INVALID;
            let Some(union) = self.family.union(existing, branch) else {
                return false;
            };
            branch = union;
        }
        false
    }

    fn finalize_union(&mut self, frame_index: usize) -> Option<u32> {
        let mut result = FAMILY_INVALID;
        for level in (0..UNION_LEVEL_COUNT).rev() {
            let branch = self.stack[frame_index].union_levels[level];
            if branch != FAMILY_INVALID {
                result = self.family.union(result, branch)?;
            }
        }
        Some(result)
    }

    fn completed_target(&self) -> bool {
        self.target_counts.binary_search(&self.used_counts).is_ok()
    }

    pub fn into_parts(self) -> (Vec<GeometryTarget>, GeometrySolutionFamily, u32, u8) {
        (
            self.targets,
            self.family,
            self.root_family,
            self.target_depth,
        )
    }

    pub fn targets(&self) -> &[GeometryTarget] {
        &self.targets
    }

    pub fn into_targets(self) -> Vec<GeometryTarget> {
        self.targets
    }

    pub fn candidate_family_count(&self) -> Option<u128> {
        self.family.path_count(self.root_family)
    }

    pub const fn expanded_nodes(&self) -> usize {
        self.expanded_nodes
    }

    pub const fn peak_frontier(&self) -> usize {
        self.peak_frontier
    }

    pub const fn domain_pruned_states(&self) -> usize {
        self.domain_pruned_states
    }

    pub const fn hall_pruned_states(&self) -> usize {
        self.hall_pruned_states
    }

    pub const fn column_pruned_states(&self) -> usize {
        self.column_pruned_states
    }

    pub const fn component_pruned_states(&self) -> usize {
        self.component_pruned_states
    }

    pub const fn component_compositions(&self) -> usize {
        self.component_compositions
    }

    pub fn retained_bytes(&self) -> usize {
        self.targets.capacity() * core::mem::size_of::<GeometryTarget>()
            + self.target_counts.capacity() * core::mem::size_of::<[u8; 7]>()
            + self.stack.capacity() * core::mem::size_of::<DenseCompileFrame>()
            + self.support_scratch.capacity() * core::mem::size_of::<u32>()
            + self.residual_memo.retained_bytes()
            + self.domain_workspace.retained_bytes()
            + self.family.retained_bytes()
    }
}

fn pack_piece_counts(counts: [u8; 7]) -> u64 {
    counts
        .into_iter()
        .enumerate()
        .fold(0_u64, |packed, (piece, count)| {
            packed | (u64::from(count) << (piece * 8))
        })
}
