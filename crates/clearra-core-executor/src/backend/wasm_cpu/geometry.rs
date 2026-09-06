use std::sync::Arc;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_domain::solution::normalized_tiling_solution::{
    PiecePlacementMask, StandardBoard64TilingIdentity,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_supply::pattern_universe::{
    MaterializedPatternUniverse, MaterializedPatternUniverseStructure, PackingMultisetFamily,
    PatternPiecePositionIndex, PatternPiecePositionIndexCompileAdvance,
    PatternPiecePositionIndexCompileSession, PieceMultisetKey,
};

use super::{
    catalog::GeometryCatalog,
    exact_collections::ExactHashMap,
    geometry_component::{compile_component_plan, ComponentFamilyEntry, ComponentPlanResult},
    geometry_domain::{hall_impossible, DomainPropagation, DomainStatus},
    geometry_family::{FamilyNodeKind, GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID},
    geometry_projection::ProjectionReachabilityCache,
    mix_digest,
    pc4_tablebase::{Pc4CompactTablebase, Pc4TablebaseLookup},
    piece_index, WasmExactSearchError, MAX_BOARD64_PIECES,
};

const NO_ROW: u32 = u32::MAX;
const RESIDUAL_ENTRY_CHUNK_CAPACITY: usize = 4096;
const UNION_LEVEL_COUNT: usize = 32;
const AVAILABLE_ROW_CACHE_MAX_ROWS: usize = 8 * 1024 * 1024;
const TARGET_PATTERN_SCAN_WORK_BUDGET: usize = 8_192;

#[derive(Clone, Copy, Debug)]
pub(super) struct GeometryCandidate {
    pub identity: StandardBoard64TilingIdentity,
    row_ids: [u32; MAX_BOARD64_PIECES],
    row_count: u8,
    pub target_index: u32,
}

impl GeometryCandidate {
    pub fn row_ids(&self) -> &[u32] {
        &self.row_ids[..usize::from(self.row_count)]
    }

    pub(super) fn from_rows(
        catalog: &GeometryCatalog,
        target_index: u32,
        rows: &[u32],
    ) -> Option<Self> {
        if rows.len() > MAX_BOARD64_PIECES {
            return None;
        }
        let mut row_ids = [0_u32; MAX_BOARD64_PIECES];
        row_ids[..rows.len()].copy_from_slice(rows);
        row_ids[..rows.len()].sort_unstable();
        for row_id in &row_ids[..rows.len()] {
            catalog.try_skeleton(*row_id)?;
        }
        let placements = row_ids[..rows.len()].iter().copied().map(|row_id| {
            let row = catalog.skeleton(row_id);
            PiecePlacementMask::new(row.piece, row.cells)
        });
        let identity =
            StandardBoard64TilingIdentity::from_placements(catalog.initial_board(), placements)
                .ok()?;
        Some(Self {
            identity,
            row_ids,
            row_count: rows.len() as u8,
            target_index,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ResidualKey {
    remaining: u64,
    packed_counts: u32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct AvailableRowKey {
    remaining: u64,
    packed_counts: u32,
    after_line_clear: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AvailableRowSlice {
    start: u32,
    len: u16,
}

impl AvailableRowSlice {
    pub(super) const fn len(self) -> usize {
        self.len as usize
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct ResidualEntry {
    remaining: u64,
    packed_counts: u32,
    suffix_family: u32,
}

const _: () = assert!(core::mem::size_of::<ResidualEntry>() == 16);

#[derive(Debug)]
struct ResidualChunk {
    entries: Vec<ResidualEntry>,
    next: Vec<u32>,
}

#[derive(Debug)]
struct ResidualMemo {
    bucket_heads: Vec<u32>,
    chunks: Vec<ResidualChunk>,
    entry_count: u32,
    insertion_disabled: bool,
}

impl ResidualMemo {
    fn disabled() -> Self {
        Self {
            bucket_heads: Vec::new(),
            chunks: Vec::new(),
            entry_count: 0,
            insertion_disabled: true,
        }
    }

    fn new(piece_count: u8) -> Self {
        let bucket_count = match piece_count {
            0..=5 => 1 << 12,
            6..=7 => 1 << 14,
            8..=10 => 1 << 18,
            _ => 1 << 20,
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

    fn lookup(&self, key: ResidualKey) -> Option<u32> {
        if self.bucket_heads.is_empty() {
            return None;
        }
        let bucket = residual_hash(key) as usize & (self.bucket_heads.len() - 1);
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

    // Every caller has just observed a miss. Residual recursion strictly removes cells,
    // so the same key cannot be inserted by a descendant while it is being evaluated.
    fn insert_after_miss(&mut self, key: ResidualKey, suffix_family: u32) {
        if self.insertion_disabled {
            return;
        }
        debug_assert!(self.lookup(key).is_none());
        if self.entry_count == u32::MAX {
            self.insertion_disabled = true;
            return;
        }
        let needs_chunk = self
            .chunks
            .last()
            .is_none_or(|chunk| chunk.entries.len() == RESIDUAL_ENTRY_CHUNK_CAPACITY);
        if needs_chunk && !self.push_chunk() {
            self.insertion_disabled = true;
            return;
        }
        self.grow_buckets_if_needed();
        let bucket = residual_hash(key) as usize & (self.bucket_heads.len() - 1);
        let previous = self.bucket_heads[bucket];
        let Some(chunk) = self.chunks.last_mut() else {
            self.insertion_disabled = true;
            return;
        };
        chunk.entries.push(ResidualEntry {
            remaining: key.remaining,
            packed_counts: key.packed_counts,
            suffix_family,
        });
        chunk.next.push(previous);
        self.entry_count += 1;
        self.bucket_heads[bucket] = self.entry_count;
    }

    fn grow_buckets_if_needed(&mut self) {
        const MAX_BUCKET_COUNT: usize = 1 << 24;
        if self.bucket_heads.len() >= MAX_BUCKET_COUNT
            || (self.entry_count as usize) < self.bucket_heads.len().saturating_mul(2)
        {
            return;
        }
        let new_len = self
            .bucket_heads
            .len()
            .saturating_mul(2)
            .min(MAX_BUCKET_COUNT);
        let mut new_heads = Vec::new();
        if new_heads.try_reserve_exact(new_len).is_err() {
            return;
        }
        new_heads.resize(new_len, 0_u32);
        for index in 0..self.entry_count {
            let entry = self
                .entry(index)
                .expect("residual memo entry index is valid");
            let bucket = residual_hash(ResidualKey {
                remaining: entry.remaining,
                packed_counts: entry.packed_counts,
            }) as usize
                & (new_len - 1);
            let previous = new_heads[bucket];
            self.set_next(index, previous);
            new_heads[bucket] = index + 1;
        }
        self.bucket_heads = new_heads;
    }

    fn push_chunk(&mut self) -> bool {
        let mut entries = Vec::new();
        let mut next = Vec::new();
        if entries
            .try_reserve_exact(RESIDUAL_ENTRY_CHUNK_CAPACITY)
            .is_err()
            || next
                .try_reserve_exact(RESIDUAL_ENTRY_CHUNK_CAPACITY)
                .is_err()
            || self.chunks.try_reserve(1).is_err()
        {
            return false;
        }
        self.chunks.push(ResidualChunk { entries, next });
        true
    }

    fn entry(&self, index: u32) -> Option<ResidualEntry> {
        let index = index as usize;
        self.chunks
            .get(index / RESIDUAL_ENTRY_CHUNK_CAPACITY)?
            .entries
            .get(index % RESIDUAL_ENTRY_CHUNK_CAPACITY)
            .copied()
    }

    fn next(&self, index: u32) -> Option<u32> {
        let index = index as usize;
        self.chunks
            .get(index / RESIDUAL_ENTRY_CHUNK_CAPACITY)?
            .next
            .get(index % RESIDUAL_ENTRY_CHUNK_CAPACITY)
            .copied()
    }

    fn set_next(&mut self, index: u32, next: u32) {
        let index = index as usize;
        let slot = self
            .chunks
            .get_mut(index / RESIDUAL_ENTRY_CHUNK_CAPACITY)
            .and_then(|chunk| chunk.next.get_mut(index % RESIDUAL_ENTRY_CHUNK_CAPACITY))
            .expect("residual memo link index is valid");
        *slot = next;
    }

    fn retained_bytes(&self) -> usize {
        self.bucket_heads.capacity() * core::mem::size_of::<u32>()
            + self.chunks.capacity() * core::mem::size_of::<ResidualChunk>()
            + self
                .chunks
                .iter()
                .map(|chunk| {
                    chunk.entries.capacity() * core::mem::size_of::<ResidualEntry>()
                        + chunk.next.capacity() * core::mem::size_of::<u32>()
                })
                .sum::<usize>()
    }
}

fn residual_hash(key: ResidualKey) -> u64 {
    mix_digest(mix_digest(0, key.remaining), u64::from(key.packed_counts))
}

#[cfg(test)]
mod residual_memo_tests {
    use super::{ResidualKey, ResidualMemo};

    #[test]
    fn growing_residual_memo_preserves_every_exact_entry() {
        let mut memo = ResidualMemo::new(0);
        let initial_bucket_count = memo.bucket_heads.len();
        let entry_count = initial_bucket_count * 2 + 1;

        for index in 0..entry_count {
            let key = ResidualKey {
                remaining: index as u64,
                packed_counts: (index as u32).rotate_left(11),
            };
            assert_eq!(memo.lookup(key), None);
            memo.insert_after_miss(key, index as u32);
        }

        assert!(memo.bucket_heads.len() > initial_bucket_count);
        for index in 0..entry_count {
            let key = ResidualKey {
                remaining: index as u64,
                packed_counts: (index as u32).rotate_left(11),
            };
            assert_eq!(memo.lookup(key), Some(index as u32));
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FamilyCompileDomain {
    PermutationClosedGeometry,
    PermutationClosedPostClearResidual,
}

impl FamilyCompileDomain {
    pub(super) const fn allows_component_composition(self) -> bool {
        matches!(
            self,
            Self::PermutationClosedGeometry | Self::PermutationClosedPostClearResidual
        )
    }
}

struct GeometryCompilerState {
    family: GeometrySolutionFamily,
    residual_memo: ResidualMemo,
    geometry_prefixes: Vec<u32>,
    projection_cache: ProjectionReachabilityCache,
    tablebase: Option<Arc<Pc4CompactTablebase>>,
    compile_domain: FamilyCompileDomain,
}

impl GeometryCompilerState {
    fn empty_post_clear_permutation_closed(target_depth: u8, geometry_prefixes: Vec<u32>) -> Self {
        Self {
            family: GeometrySolutionFamily::new(),
            residual_memo: ResidualMemo::new(target_depth),
            geometry_prefixes,
            projection_cache: ProjectionReachabilityCache::default(),
            tablebase: None,
            compile_domain: FamilyCompileDomain::PermutationClosedPostClearResidual,
        }
    }
}

pub(super) struct GeometryCompletionOracle {
    permutation_closed_state: Option<GeometryCompilerState>,
    post_clear_state: Option<GeometryCompilerState>,
    targets: Arc<[TargetGroup]>,
    execution_prefixes: Vec<u32>,
    target_depth: u8,
    traversal_marks: Vec<u32>,
    row_marks: Vec<u32>,
    traversal_generation: u32,
    row_generation: u32,
    traversal_stack: Vec<u32>,
    candidate_rows: Vec<u32>,
    available_row_cache: ExactHashMap<AvailableRowKey, AvailableRowSlice>,
    available_row_arena: Vec<u32>,
    available_row_cache_enabled: bool,
}

impl GeometryCompletionOracle {
    fn new(
        state: GeometryCompilerState,
        targets: Arc<[TargetGroup]>,
        execution_prefixes: Vec<u32>,
        target_depth: u8,
        skeleton_count: usize,
    ) -> Result<Self, WasmExactSearchError> {
        let family_value_count = state.family.node_count() as usize + 2;
        let mut traversal_marks = Vec::new();
        traversal_marks
            .try_reserve_exact(family_value_count)
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_completion_traversal_storage_unavailable",
                )
            })?;
        traversal_marks.resize(family_value_count, 0);
        let mut row_marks = Vec::new();
        row_marks.try_reserve_exact(skeleton_count).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_completion_row_storage_unavailable")
        })?;
        row_marks.resize(skeleton_count, 0);
        let post_clear_state = GeometryCompilerState::empty_post_clear_permutation_closed(
            target_depth,
            state.geometry_prefixes.clone(),
        );
        Ok(Self {
            permutation_closed_state: Some(state),
            post_clear_state: Some(post_clear_state),
            targets,
            execution_prefixes,
            target_depth,
            traversal_marks,
            row_marks,
            traversal_generation: 0,
            row_generation: 0,
            traversal_stack: Vec::new(),
            candidate_rows: Vec::new(),
            available_row_cache: ExactHashMap::default(),
            available_row_arena: Vec::new(),
            available_row_cache_enabled: true,
        })
    }

    pub(super) fn collect_available_rows(
        &mut self,
        remaining: u64,
        packed_counts: u32,
        after_line_clear: bool,
        catalog: &GeometryCatalog,
        output: &mut Vec<u32>,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if let Some(cached) = self.collect_available_rows_storage(
            remaining,
            packed_counts,
            after_line_clear,
            catalog,
            output,
            control,
        )? {
            let rows = self.available_rows(cached)?;
            output.try_reserve(rows.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_completion_output_storage_unavailable")
            })?;
            output.extend_from_slice(rows);
        }
        Ok(())
    }

    pub(super) fn collect_available_rows_storage(
        &mut self,
        remaining: u64,
        packed_counts: u32,
        after_line_clear: bool,
        catalog: &GeometryCatalog,
        output: &mut Vec<u32>,
        control: &ExecutionControl,
    ) -> Result<Option<AvailableRowSlice>, WasmExactSearchError> {
        output.clear();
        if remaining == 0
            || self
                .execution_prefixes
                .binary_search(&packed_counts)
                .is_err()
        {
            return Ok(None);
        }
        let cache_key = AvailableRowKey {
            remaining,
            packed_counts,
            after_line_clear,
        };
        if let Some(cached) = self.available_row_cache.get(&cache_key).copied() {
            return Ok(Some(cached));
        }
        let compile_domain = if after_line_clear {
            FamilyCompileDomain::PermutationClosedPostClearResidual
        } else {
            FamilyCompileDomain::PermutationClosedGeometry
        };
        let (root, _) = self.ensure_family_root(
            ResidualKey {
                remaining,
                packed_counts,
            },
            compile_domain,
            catalog,
            control,
        )?;
        if root == FAMILY_INVALID || root == FAMILY_EMPTY {
            return Ok(None);
        }
        self.ensure_traversal_capacity(compile_domain)?;
        self.collect_family_rows(root, compile_domain)?;

        let mut candidates = std::mem::take(&mut self.candidate_rows);
        candidates.sort_unstable();
        candidates.dedup();
        output.try_reserve(candidates.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_completion_output_storage_unavailable")
        })?;
        for row_id in candidates.iter().copied() {
            let row = catalog.skeleton(row_id);
            if row.cells & remaining != row.cells {
                continue;
            }
            let Some(next_counts) = add_packed_piece(packed_counts, piece_index(row.piece)) else {
                continue;
            };
            if self.execution_prefixes.binary_search(&next_counts).is_ok() {
                output.push(row_id);
            }
        }
        self.cache_available_rows(cache_key, output);
        candidates.clear();
        self.candidate_rows = candidates;
        Ok(None)
    }

    pub(super) fn available_rows(
        &self,
        cached: AvailableRowSlice,
    ) -> Result<&[u32], WasmExactSearchError> {
        let start = cached.start as usize;
        let end = start + cached.len();
        self.available_row_arena
            .get(start..end)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "setup_completion_available_row_cache_invalid",
            ))
    }

    fn cache_available_rows(&mut self, key: AvailableRowKey, rows: &[u32]) {
        if !self.available_row_cache_enabled
            || rows.len() > usize::from(u16::MAX)
            || self.available_row_arena.len().saturating_add(rows.len())
                > AVAILABLE_ROW_CACHE_MAX_ROWS
        {
            return;
        }
        let Ok(start) = u32::try_from(self.available_row_arena.len()) else {
            self.available_row_cache_enabled = false;
            return;
        };
        let Ok(len) = u16::try_from(rows.len()) else {
            return;
        };
        if self.available_row_arena.try_reserve(rows.len()).is_err()
            || self.available_row_cache.try_reserve(1).is_err()
        {
            self.available_row_cache_enabled = false;
            return;
        }
        self.available_row_arena.extend_from_slice(rows);
        self.available_row_cache
            .insert(key, AvailableRowSlice { start, len });
    }

    fn collect_family_rows(
        &mut self,
        root: u32,
        compile_domain: FamilyCompileDomain,
    ) -> Result<(), WasmExactSearchError> {
        self.candidate_rows.clear();
        self.advance_traversal_generation();
        self.advance_row_generation();
        self.traversal_stack.clear();
        self.traversal_stack.try_reserve(64).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_completion_stack_storage_unavailable")
        })?;
        self.traversal_stack.push(root);
        while let Some(reference) = self.traversal_stack.pop() {
            if reference == FAMILY_INVALID || reference == FAMILY_EMPTY {
                continue;
            }
            let index = reference as usize;
            let mark =
                self.traversal_marks
                    .get_mut(index)
                    .ok_or(WasmExactSearchError::InvalidProblem(
                        "setup_completion_family_reference_invalid",
                    ))?;
            if *mark == self.traversal_generation {
                continue;
            }
            *mark = self.traversal_generation;
            let node = self
                .compiler_state(compile_domain)
                .family
                .node(reference)
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "setup_completion_family_node_missing",
                ))?;
            match node.kind {
                FamilyNodeKind::Append => {
                    let row_index = node.row_id as usize;
                    let row_mark = self.row_marks.get_mut(row_index).ok_or(
                        WasmExactSearchError::InvalidProblem("setup_completion_family_row_invalid"),
                    )?;
                    if *row_mark != self.row_generation {
                        *row_mark = self.row_generation;
                        self.candidate_rows.push(node.row_id);
                    }
                    self.traversal_stack.push(node.left);
                }
                FamilyNodeKind::Union | FamilyNodeKind::Product => {
                    self.traversal_stack.push(node.left);
                    self.traversal_stack.push(node.right);
                }
            }
        }
        Ok(())
    }

    fn ensure_family_root(
        &mut self,
        key: ResidualKey,
        compile_domain: FamilyCompileDomain,
        catalog: &GeometryCatalog,
        control: &ExecutionControl,
    ) -> Result<(u32, bool), WasmExactSearchError> {
        if let Some(root) = self
            .compiler_state(compile_domain)
            .residual_memo
            .lookup(key)
        {
            return Ok((root, true));
        }

        let used_counts = unpack_piece_counts(key.packed_counts);
        let used_total = used_counts.iter().copied().sum::<u8>();
        let state = self.compiler_state(compile_domain);
        let valid = used_total <= self.target_depth
            && key.remaining.count_ones() as usize
                == usize::from(self.target_depth - used_total) * 4
            && state
                .geometry_prefixes
                .binary_search(&key.packed_counts)
                .is_ok();
        if !valid {
            self.compiler_state_mut(compile_domain)
                .residual_memo
                .insert_after_miss(key, FAMILY_INVALID);
            return Ok((FAMILY_INVALID, false));
        }

        let state = self
            .compiler_state_slot(compile_domain)
            .take()
            .expect("setup completion compiler state exists");
        let mut compiler = FamilyCompiler::from_residual(
            key.remaining,
            Arc::clone(&self.targets),
            used_counts,
            state,
        );
        let mut work = 0_usize;
        loop {
            if work & 1023 == 0 && control.is_cancelled() {
                *self.compiler_state_slot(compile_domain) = Some(compiler.into_completion_state());
                return Err(WasmExactSearchError::Cancelled);
            }
            work = work.saturating_add(1);
            match compiler.advance(catalog) {
                CompileAdvance::Pending => {}
                CompileAdvance::Complete => {
                    let root = compiler.root_family;
                    *self.compiler_state_slot(compile_domain) =
                        Some(compiler.into_completion_state());
                    return Ok((root, false));
                }
                CompileAdvance::ResourceIncomplete => {
                    *self.compiler_state_slot(compile_domain) =
                        Some(compiler.into_completion_state());
                    return Err(WasmExactSearchError::InvalidProblem(
                        "setup_completion_family_storage_unavailable",
                    ));
                }
            }
        }
    }

    fn ensure_traversal_capacity(
        &mut self,
        compile_domain: FamilyCompileDomain,
    ) -> Result<(), WasmExactSearchError> {
        let required = self.compiler_state(compile_domain).family.node_count() as usize + 2;
        if required <= self.traversal_marks.len() {
            return Ok(());
        }
        self.traversal_marks
            .try_reserve(required - self.traversal_marks.len())
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_completion_traversal_storage_unavailable",
                )
            })?;
        self.traversal_marks.resize(required, 0);
        Ok(())
    }

    fn advance_traversal_generation(&mut self) {
        self.traversal_generation = self.traversal_generation.wrapping_add(1);
        if self.traversal_generation == 0 {
            self.traversal_marks.fill(0);
            self.traversal_generation = 1;
        }
    }

    fn advance_row_generation(&mut self) {
        self.row_generation = self.row_generation.wrapping_add(1);
        if self.row_generation == 0 {
            self.row_marks.fill(0);
            self.row_generation = 1;
        }
    }

    fn compiler_state(&self, compile_domain: FamilyCompileDomain) -> &GeometryCompilerState {
        self.compiler_state_slot_ref(compile_domain)
            .as_ref()
            .expect("setup completion compiler state exists")
    }

    fn compiler_state_mut(
        &mut self,
        compile_domain: FamilyCompileDomain,
    ) -> &mut GeometryCompilerState {
        self.compiler_state_slot(compile_domain)
            .as_mut()
            .expect("setup completion compiler state exists")
    }

    fn compiler_state_slot_ref(
        &self,
        compile_domain: FamilyCompileDomain,
    ) -> &Option<GeometryCompilerState> {
        match compile_domain {
            FamilyCompileDomain::PermutationClosedGeometry => &self.permutation_closed_state,
            FamilyCompileDomain::PermutationClosedPostClearResidual => &self.post_clear_state,
        }
    }

    fn compiler_state_slot(
        &mut self,
        compile_domain: FamilyCompileDomain,
    ) -> &mut Option<GeometryCompilerState> {
        match compile_domain {
            FamilyCompileDomain::PermutationClosedGeometry => &mut self.permutation_closed_state,
            FamilyCompileDomain::PermutationClosedPostClearResidual => &mut self.post_clear_state,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct TargetGroup {
    pub key: PieceMultisetKey,
    pub pattern_index_id: u32,
    pub possible_patterns: Arc<PatternBitSet>,
    pub pattern_index: Option<Arc<PatternPiecePositionIndex>>,
}

impl TargetGroup {
    /// Witness-only verification preserves candidate membership only when the
    /// target group contains one concrete queue language.
    pub fn single_pattern_witness_is_exact(&self) -> bool {
        self.pattern_index.is_some() && self.possible_patterns.count_ones() == 1
    }
}

#[derive(Clone, Copy, Debug)]
struct CompileFrame {
    remaining: u64,
    component_cells: u64,
    component_remainder: u64,
    key: ResidualKey,
    support_cell: u8,
    support_cursor: usize,
    support_end: usize,
    feasible_piece_mask: u8,
    depth: u8,
    entered: bool,
    chosen_row: u32,
    chosen_component_family: u32,
    chosen_component_signature: u32,
    component_entry_cursor: usize,
    component_entry_end: usize,
    component_scratch_checkpoint: usize,
    tablebase_eligible: bool,
    domain: DomainPropagation,
    union_levels: [u32; UNION_LEVEL_COUNT],
}

impl CompileFrame {
    fn root(remaining: u64) -> Self {
        Self::child(remaining, 0, NO_ROW, 0, true)
    }

    fn child(
        remaining: u64,
        depth: u8,
        chosen_row: u32,
        component_scratch_checkpoint: usize,
        tablebase_eligible: bool,
    ) -> Self {
        Self {
            remaining,
            component_cells: 0,
            component_remainder: 0,
            key: ResidualKey {
                remaining,
                packed_counts: 0,
            },
            support_cell: 0,
            support_cursor: 0,
            support_end: 0,
            feasible_piece_mask: 0,
            depth,
            entered: false,
            chosen_row,
            chosen_component_family: FAMILY_INVALID,
            chosen_component_signature: 0,
            component_entry_cursor: 0,
            component_entry_end: 0,
            component_scratch_checkpoint,
            tablebase_eligible,
            domain: DomainPropagation::empty(),
            union_levels: [FAMILY_INVALID; UNION_LEVEL_COUNT],
        }
    }
}

enum CompileAdvance {
    Pending,
    Complete,
    ResourceIncomplete,
}

#[derive(Debug)]
struct FamilyCompiler {
    targets: Arc<[TargetGroup]>,
    tablebase: Option<Arc<Pc4CompactTablebase>>,
    admissible_prefixes: Vec<u32>,
    compile_domain: FamilyCompileDomain,
    used_counts: [u8; 7],
    stack: Vec<CompileFrame>,
    residual_memo: ResidualMemo,
    projection_cache: ProjectionReachabilityCache,
    family: GeometrySolutionFamily,
    component_entries: Vec<ComponentFamilyEntry>,
    target_depth: u8,
    root_family: u32,
    expanded_nodes: usize,
    peak_frontier: usize,
    domain_pruned_states: usize,
    hall_pruned_states: usize,
    column_pruned_states: usize,
    component_compositions: usize,
    tablebase_pruned_states: usize,
    resource_authoritative: bool,
}

impl FamilyCompiler {
    fn new(
        required_cells: u64,
        targets: Arc<[TargetGroup]>,
        tablebase: Option<Arc<Pc4CompactTablebase>>,
    ) -> Self {
        let admissible_prefixes = compile_admissible_prefixes(&targets);
        Self::new_with_admissible_prefixes(required_cells, targets, admissible_prefixes, tablebase)
    }

    fn try_new(
        required_cells: u64,
        targets: Arc<[TargetGroup]>,
        tablebase: Option<Arc<Pc4CompactTablebase>>,
    ) -> Result<Self, WasmExactSearchError> {
        let admissible_prefixes = compile_admissible_prefixes_checked(&targets)?;
        let target_depth = targets.first().map_or(0, |target| target.key.total_count());
        let mut stack = Vec::new();
        stack
            .try_reserve_exact(usize::from(target_depth).saturating_add(1))
            .map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "wasm_geometry_compile_stack_storage_unavailable",
                )
            })?;
        stack.push(CompileFrame::root(required_cells));
        Ok(Self {
            targets,
            tablebase,
            admissible_prefixes,
            compile_domain: FamilyCompileDomain::PermutationClosedGeometry,
            used_counts: [0; 7],
            stack,
            residual_memo: ResidualMemo::disabled(),
            projection_cache: ProjectionReachabilityCache::default(),
            family: GeometrySolutionFamily::new(),
            component_entries: Vec::new(),
            target_depth,
            root_family: FAMILY_INVALID,
            expanded_nodes: 0,
            peak_frontier: 1,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            tablebase_pruned_states: 0,
            resource_authoritative: true,
        })
    }

    fn new_with_admissible_prefixes(
        required_cells: u64,
        targets: Arc<[TargetGroup]>,
        admissible_prefixes: Vec<u32>,
        tablebase: Option<Arc<Pc4CompactTablebase>>,
    ) -> Self {
        let target_depth = targets.first().map_or(0, |target| target.key.total_count());
        Self {
            targets,
            tablebase,
            admissible_prefixes,
            compile_domain: FamilyCompileDomain::PermutationClosedGeometry,
            used_counts: [0; 7],
            stack: vec![CompileFrame::root(required_cells)],
            residual_memo: ResidualMemo::new(target_depth),
            projection_cache: ProjectionReachabilityCache::default(),
            family: GeometrySolutionFamily::new(),
            component_entries: Vec::new(),
            target_depth,
            root_family: FAMILY_INVALID,
            expanded_nodes: 0,
            peak_frontier: 1,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            tablebase_pruned_states: 0,
            resource_authoritative: false,
        }
    }

    fn from_residual(
        remaining: u64,
        targets: Arc<[TargetGroup]>,
        used_counts: [u8; 7],
        state: GeometryCompilerState,
    ) -> Self {
        let target_depth = targets.first().map_or(0, |target| target.key.total_count());
        let depth = used_counts.iter().copied().sum();
        Self {
            targets,
            tablebase: state.tablebase,
            admissible_prefixes: state.geometry_prefixes,
            compile_domain: state.compile_domain,
            used_counts,
            stack: vec![CompileFrame::child(remaining, depth, NO_ROW, 0, true)],
            residual_memo: state.residual_memo,
            projection_cache: state.projection_cache,
            family: state.family,
            component_entries: Vec::new(),
            target_depth,
            root_family: FAMILY_INVALID,
            expanded_nodes: 0,
            peak_frontier: 1,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            tablebase_pruned_states: 0,
            resource_authoritative: false,
        }
    }

    fn into_completion_state(self) -> GeometryCompilerState {
        GeometryCompilerState {
            family: self.family,
            residual_memo: self.residual_memo,
            geometry_prefixes: self.admissible_prefixes,
            projection_cache: self.projection_cache,
            tablebase: self.tablebase,
            compile_domain: self.compile_domain,
        }
    }

    fn advance(&mut self, catalog: &GeometryCatalog) -> CompileAdvance {
        if self.stack.is_empty() {
            return CompileAdvance::Complete;
        }
        let top_index = self.stack.len() - 1;
        if !self.stack[top_index].entered {
            let remaining = self.stack[top_index].remaining;
            let key = ResidualKey {
                remaining,
                packed_counts: pack_piece_counts(self.used_counts),
            };
            self.stack[top_index].key = key;
            if let Some(family) = self.residual_memo.lookup(key) {
                return self.finish_top(catalog, family, false);
            }
            if self.stack[top_index].tablebase_eligible
                && self.tablebase.as_ref().is_some_and(|tablebase| {
                    tablebase.lookup_placed_field(catalog.required_cells() ^ remaining)
                        == Pc4TablebaseLookup::ExactDead
                })
            {
                self.tablebase_pruned_states = self.tablebase_pruned_states.saturating_add(1);
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }

            self.expanded_nodes = self.expanded_nodes.saturating_add(1);
            self.peak_frontier = self.peak_frontier.max(self.stack.len());
            let depth = self.stack[top_index].depth;
            if remaining == 0 {
                let family = if depth == self.target_depth && self.completed_target().is_some() {
                    FAMILY_EMPTY
                } else {
                    FAMILY_INVALID
                };
                return self.finish_top(catalog, family, true);
            }
            if depth >= self.target_depth
                || remaining.count_ones() as usize != usize::from(self.target_depth - depth) * 4
            {
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }

            let feasible_piece_mask = self.feasible_piece_mask();
            if self.stack[top_index].tablebase_eligible
                && self.tablebase.as_ref().is_some_and(|tablebase| {
                    tablebase.lookup_placed_field_with_piece_mask(
                        catalog.required_cells() ^ remaining,
                        feasible_piece_mask,
                    ) == Pc4TablebaseLookup::ExactDead
                })
            {
                self.tablebase_pruned_states = self.tablebase_pruned_states.saturating_add(1);
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }
            if self.stack[top_index].tablebase_eligible
                && self.tablebase.as_ref().is_some_and(|tablebase| {
                    self.all_admissible_target_counts_are_dead(
                        tablebase,
                        catalog.required_cells() ^ remaining,
                    )
                })
            {
                self.tablebase_pruned_states = self.tablebase_pruned_states.saturating_add(1);
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }
            let advanced_analysis_enabled = !self.resource_authoritative && self.target_depth >= 7;
            let component_composition_enabled =
                advanced_analysis_enabled && self.compile_domain.allows_component_composition();
            let advanced_domain = advanced_analysis_enabled && catalog.initial_board() != 0;
            let (domain_status, domain, cell_piece_masks) = if advanced_domain {
                let compilation =
                    DomainPropagation::compile(catalog, remaining, feasible_piece_mask);
                (
                    compilation.status,
                    compilation.propagation,
                    compilation.cell_piece_masks,
                )
            } else {
                let (status, propagation) =
                    DomainPropagation::compile_minimum(catalog, remaining, feasible_piece_mask);
                (status, propagation, [0; 64])
            };
            if domain_status == DomainStatus::Empty {
                self.domain_pruned_states = self.domain_pruned_states.saturating_add(1);
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }
            if advanced_domain
                && (depth <= 2 || domain.pivot_support_count >= 5)
                && hall_impossible(
                    &self.targets,
                    self.used_counts,
                    remaining,
                    &cell_piece_masks,
                )
            {
                self.hall_pruned_states = self.hall_pruned_states.saturating_add(1);
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }
            let exact_projection_enabled = advanced_domain
                && remaining.count_ones() >= 24
                && (depth <= 2 || domain.pivot_support_count >= 5);
            if advanced_domain
                && self.projection_cache.residual_impossible(
                    catalog.projection_catalog(),
                    &self.targets,
                    self.used_counts,
                    remaining,
                    exact_projection_enabled,
                )
            {
                self.column_pruned_states = self.column_pruned_states.saturating_add(1);
                return self.finish_top(catalog, FAMILY_INVALID, true);
            }
            if component_composition_enabled {
                match compile_component_plan(
                    catalog,
                    remaining,
                    depth,
                    self.used_counts,
                    &self.targets,
                    &self.admissible_prefixes,
                    feasible_piece_mask,
                    &mut self.family,
                ) {
                    ComponentPlanResult::NotApplicable => {}
                    ComponentPlanResult::Impossible => {
                        self.domain_pruned_states = self.domain_pruned_states.saturating_add(1);
                        return self.finish_top(catalog, FAMILY_INVALID, true);
                    }
                    ComponentPlanResult::StorageUnavailable => {
                        return CompileAdvance::ResourceIncomplete;
                    }
                    ComponentPlanResult::Complete {
                        family,
                        expanded_nodes,
                    } => {
                        self.expanded_nodes = self.expanded_nodes.saturating_add(expanded_nodes);
                        self.component_compositions = self.component_compositions.saturating_add(1);
                        return self.finish_top(catalog, family, true);
                    }
                    ComponentPlanResult::Ready(mut plan) => {
                        self.expanded_nodes =
                            self.expanded_nodes.saturating_add(plan.expanded_nodes);
                        if self
                            .component_entries
                            .try_reserve(plan.entries.len())
                            .is_ok()
                        {
                            let start = self.component_entries.len();
                            self.component_entries.append(&mut plan.entries);
                            let frame = &mut self.stack[top_index];
                            frame.component_cells = plan.owner_cells;
                            frame.component_remainder = plan.remainder_cells;
                            frame.component_entry_cursor = start;
                            frame.component_entry_end = self.component_entries.len();
                            frame.entered = true;
                            self.component_compositions =
                                self.component_compositions.saturating_add(1);
                            return CompileAdvance::Pending;
                        }
                    }
                }
            }
            let frame = &mut self.stack[top_index];
            frame.support_cell = domain.pivot_cell;
            frame.support_cursor = 0;
            frame.support_end = catalog.support(domain.pivot_cell).len();
            frame.feasible_piece_mask = feasible_piece_mask;
            frame.domain = domain;
            frame.entered = true;
            return CompileAdvance::Pending;
        }

        let frame = self.stack[top_index];
        if frame.component_entry_cursor < frame.component_entry_end {
            let entry = self.component_entries[frame.component_entry_cursor];
            self.stack[top_index].component_entry_cursor += 1;
            let component_piece_count = signature_total_count(entry.piece_signature);
            add_signature(&mut self.used_counts, entry.piece_signature);
            if self.stack.try_reserve(1).is_err() {
                remove_signature(&mut self.used_counts, entry.piece_signature);
                return CompileAdvance::ResourceIncomplete;
            }
            let mut child = CompileFrame::child(
                frame.component_remainder,
                frame.depth + component_piece_count,
                NO_ROW,
                self.component_entries.len(),
                false,
            );
            child.chosen_component_family = entry.family;
            child.chosen_component_signature = entry.piece_signature;
            self.stack.push(child);
            return CompileAdvance::Pending;
        }
        if frame.support_cursor < frame.support_end {
            self.stack[top_index].support_cursor += 1;
            let row_id = catalog.support(frame.support_cell)[frame.support_cursor];
            let row = catalog.skeleton(row_id);
            if !frame.domain.row_allowed(
                catalog,
                row_id,
                frame.remaining,
                frame.feasible_piece_mask,
            ) {
                return CompileAdvance::Pending;
            }
            self.used_counts[piece_index(row.piece)] += 1;
            if self.stack.try_reserve(1).is_err() {
                self.used_counts[piece_index(row.piece)] -= 1;
                return CompileAdvance::ResourceIncomplete;
            }
            self.stack.push(CompileFrame::child(
                frame.remaining ^ row.cells,
                frame.depth + 1,
                row_id,
                self.component_entries.len(),
                frame.tablebase_eligible,
            ));
            return CompileAdvance::Pending;
        }

        let Some(family) = self.finalize_union(top_index) else {
            return CompileAdvance::ResourceIncomplete;
        };
        self.finish_top(catalog, family, true)
    }

    fn finish_top(
        &mut self,
        catalog: &GeometryCatalog,
        suffix_family: u32,
        memoize: bool,
    ) -> CompileAdvance {
        let frame = self.stack.pop().expect("geometry compile frame exists");
        self.component_entries
            .truncate(frame.component_scratch_checkpoint);
        if memoize {
            self.residual_memo
                .insert_after_miss(frame.key, suffix_family);
        }
        if frame.chosen_row == NO_ROW && frame.chosen_component_family == FAMILY_INVALID {
            self.root_family = suffix_family;
            return CompileAdvance::Complete;
        }

        if frame.chosen_component_family != FAMILY_INVALID {
            remove_signature(&mut self.used_counts, frame.chosen_component_signature);
            if suffix_family == FAMILY_INVALID {
                return CompileAdvance::Pending;
            }
            let Some(branch) = self
                .family
                .product(frame.chosen_component_family, suffix_family)
            else {
                return CompileAdvance::ResourceIncomplete;
            };
            return if self.add_branch_to_parent(branch) {
                CompileAdvance::Pending
            } else {
                CompileAdvance::ResourceIncomplete
            };
        }

        let piece = piece_index(catalog.skeleton(frame.chosen_row).piece);
        self.used_counts[piece] -= 1;
        if suffix_family == FAMILY_INVALID {
            return CompileAdvance::Pending;
        }
        let Some(branch) = self.family.append(frame.chosen_row, suffix_family) else {
            return CompileAdvance::ResourceIncomplete;
        };
        if self.add_branch_to_parent(branch) {
            CompileAdvance::Pending
        } else {
            CompileAdvance::ResourceIncomplete
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

    fn feasible_piece_mask(&self) -> u8 {
        feasible_piece_mask_for(&self.admissible_prefixes, self.used_counts)
    }

    fn all_admissible_target_counts_are_dead(
        &self,
        tablebase: &Pc4CompactTablebase,
        placed_field: u64,
    ) -> bool {
        let mut found_admissible_target = false;
        for target in self.targets.iter() {
            let target_counts = target.key.counts();
            if target_counts
                .iter()
                .zip(self.used_counts)
                .any(|(target, used)| *target < used)
            {
                continue;
            }
            found_admissible_target = true;
            let remaining_counts =
                std::array::from_fn(|piece| target_counts[piece] - self.used_counts[piece]);
            if tablebase.lookup_placed_field_with_remaining_counts(placed_field, remaining_counts)
                != Pc4TablebaseLookup::ExactDead
            {
                return false;
            }
        }
        found_admissible_target
    }

    fn completed_target(&self) -> Option<&TargetGroup> {
        let key = PieceMultisetKey::from_counts(self.used_counts);
        self.targets
            .binary_search_by_key(&key, |target| target.key)
            .ok()
            .map(|index| &self.targets[index])
    }

    fn retained_bytes(&self) -> usize {
        target_bytes(&self.targets)
            + self.admissible_prefixes.capacity() * core::mem::size_of::<u32>()
            + self.stack.capacity() * core::mem::size_of::<CompileFrame>()
            + self.residual_memo.retained_bytes()
            + self.projection_cache.retained_bytes()
            + self.family.retained_bytes()
            + self.component_entries.capacity() * core::mem::size_of::<ComponentFamilyEntry>()
    }

    fn set_retained_limit_bytes(&mut self, limit: u128) -> bool {
        let family_retained = self.family.retained_bytes() as u128;
        let Some(fixed_retained) = (self.retained_bytes() as u128).checked_sub(family_retained)
        else {
            return false;
        };
        let Some(family_limit) = limit.checked_sub(fixed_retained) else {
            return false;
        };
        self.family.set_retained_limit_bytes(Some(family_limit));
        self.retained_bytes() as u128 <= limit
    }

    fn into_enumerator(self) -> FamilyEnumerator {
        FamilyEnumerator::new(
            self.targets,
            self.family,
            self.root_family,
            self.target_depth,
        )
    }

    fn try_into_bounded_enumerator(self, limit: u128) -> Option<FamilyEnumerator> {
        FamilyEnumerator::try_new_bounded(
            self.targets,
            self.family,
            self.root_family,
            self.target_depth,
            limit,
        )
    }

    fn candidate_family_count(&self) -> Option<u128> {
        self.family.path_count(self.root_family)
    }
}

// The compiler transfers its completed family without an intermediate allocation.
#[allow(clippy::large_enum_variant)]
pub(super) enum GeometryFamilyCompileAdvance {
    Pending,
    Complete(CompiledGeometryFamily),
    ResourceIncomplete(&'static str),
    Cancelled,
}

pub(super) struct GeometryFamilyCompileSession {
    compiler: Option<FamilyCompiler>,
    execution_prefixes: Vec<u32>,
}

pub(super) struct CompiledGeometryFamily {
    pub completion_oracle: GeometryCompletionOracle,
    pub candidate_family_count: Option<u128>,
    pub expanded_nodes: usize,
    pub tablebase_pruned_states: usize,
}

impl GeometryFamilyCompileSession {
    pub fn new_with_tablebase(
        required_cells: u64,
        mut target_keys: Vec<PieceMultisetKey>,
        mut execution_prefixes: Vec<u32>,
        tablebase: Option<Arc<Pc4CompactTablebase>>,
    ) -> Result<Self, WasmExactSearchError> {
        target_keys.sort_unstable();
        target_keys.dedup();
        if target_keys.is_empty() {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_geometry_has_no_admissible_piece_multiset",
            ));
        }
        let target_depth = target_keys[0].total_count();
        if target_keys
            .iter()
            .any(|target| target.total_count() != target_depth)
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_geometry_target_depth_mismatch",
            ));
        }
        execution_prefixes.sort_unstable();
        execution_prefixes.dedup();
        if execution_prefixes.binary_search(&0).is_err()
            || target_keys.iter().any(|target| {
                execution_prefixes
                    .binary_search(&pack_piece_counts(target.counts()))
                    .is_err()
            })
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_geometry_admissible_prefix_domain_incomplete",
            ));
        }

        let possible_patterns = Arc::new(PatternBitSet::all(1));
        let targets = target_keys
            .into_iter()
            .enumerate()
            .map(|(index, key)| TargetGroup {
                key,
                pattern_index_id: index as u32,
                possible_patterns: Arc::clone(&possible_patterns),
                pattern_index: None,
            })
            .collect::<Vec<_>>();
        let compiler_prefixes = compile_admissible_prefixes(&targets);
        Ok(Self {
            compiler: Some(FamilyCompiler::new_with_admissible_prefixes(
                required_cells,
                targets.into(),
                compiler_prefixes,
                tablebase,
            )),
            execution_prefixes,
        })
    }

    pub fn advance(
        &mut self,
        catalog: &GeometryCatalog,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> GeometryFamilyCompileAdvance {
        let Some(compiler) = self.compiler.as_mut() else {
            return GeometryFamilyCompileAdvance::ResourceIncomplete(
                "setup_geometry_compile_session_already_finished",
            );
        };
        for work in 0..work_budget.max(1) {
            if work & 1023 == 0 && control.is_cancelled() {
                return GeometryFamilyCompileAdvance::Cancelled;
            }
            match compiler.advance(catalog) {
                CompileAdvance::Pending => {}
                CompileAdvance::ResourceIncomplete => {
                    return GeometryFamilyCompileAdvance::ResourceIncomplete(
                        "geometry_solution_family_storage_unavailable",
                    );
                }
                CompileAdvance::Complete => {
                    let compiler = self.compiler.take().expect("geometry compiler exists");
                    let candidate_family_count = compiler.candidate_family_count();
                    let targets = Arc::clone(&compiler.targets);
                    let target_depth = compiler.target_depth;
                    let expanded_nodes = compiler.expanded_nodes;
                    let tablebase_pruned_states = compiler.tablebase_pruned_states;
                    let state = compiler.into_completion_state();
                    let execution_prefixes = std::mem::take(&mut self.execution_prefixes);
                    let completion_oracle = match GeometryCompletionOracle::new(
                        state,
                        targets,
                        execution_prefixes,
                        target_depth,
                        catalog.skeleton_count(),
                    ) {
                        Ok(oracle) => oracle,
                        Err(WasmExactSearchError::InvalidProblem(reason)) => {
                            return GeometryFamilyCompileAdvance::ResourceIncomplete(reason);
                        }
                        // Admission is owned by the enclosing session and cannot
                        // originate in this pure completion-oracle constructor.
                        // Keep the compatibility result fail-closed if that
                        // invariant is ever violated.
                        Err(error @ WasmExactSearchError::ResourceAdmission(_)) => {
                            return GeometryFamilyCompileAdvance::ResourceIncomplete(
                                error.reason(),
                            );
                        }
                        Err(WasmExactSearchError::Cancelled) => {
                            return GeometryFamilyCompileAdvance::Cancelled;
                        }
                    };
                    return GeometryFamilyCompileAdvance::Complete(CompiledGeometryFamily {
                        completion_oracle,
                        candidate_family_count,
                        expanded_nodes,
                        tablebase_pruned_states,
                    });
                }
            }
        }
        GeometryFamilyCompileAdvance::Pending
    }

    pub(super) fn progress_nodes(&self) -> usize {
        self.compiler.as_ref().map_or(0, |compiler| {
            compiler
                .expanded_nodes
                .saturating_add(compiler.family.node_count() as usize)
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct TraversalTask {
    family: u32,
    continuations: [u32; MAX_BOARD64_PIECES],
    depth: u8,
    continuation_count: u8,
}

#[derive(Clone, Copy, Debug)]
#[cfg(feature = "parallel")]
struct TraversalSeed {
    task: TraversalTask,
    rows: [u32; MAX_BOARD64_PIECES],
    weight: u128,
    splittable: bool,
}

#[derive(Debug)]
struct FamilyEnumerator {
    targets: Arc<[TargetGroup]>,
    family: Arc<GeometrySolutionFamily>,
    tasks: Vec<TraversalTask>,
    rows: [u32; MAX_BOARD64_PIECES],
    target_depth: u8,
    retained_limit_bytes: Option<u128>,
    #[cfg(feature = "parallel")]
    candidate_count: Option<u128>,
}

impl FamilyEnumerator {
    fn new(
        targets: Arc<[TargetGroup]>,
        family: GeometrySolutionFamily,
        root_family: u32,
        target_depth: u8,
    ) -> Self {
        #[cfg(feature = "parallel")]
        let candidate_count = family.path_count(root_family);
        let mut tasks = Vec::new();
        if root_family != FAMILY_INVALID {
            tasks.push(TraversalTask {
                family: root_family,
                continuations: [FAMILY_INVALID; MAX_BOARD64_PIECES],
                depth: 0,
                continuation_count: 0,
            });
        }
        Self {
            targets,
            family: Arc::new(family),
            tasks,
            rows: [0; MAX_BOARD64_PIECES],
            target_depth,
            retained_limit_bytes: None,
            #[cfg(feature = "parallel")]
            candidate_count,
        }
    }

    fn try_new_bounded(
        targets: Arc<[TargetGroup]>,
        family: GeometrySolutionFamily,
        root_family: u32,
        target_depth: u8,
        retained_limit_bytes: u128,
    ) -> Option<Self> {
        let fixed = (target_bytes(&targets) as u128)
            .checked_add(core::mem::size_of::<GeometrySolutionFamily>() as u128)?
            .checked_add(family.retained_bytes() as u128)?;
        if fixed > retained_limit_bytes {
            return None;
        }
        let mut tasks = Vec::new();
        if root_family != FAMILY_INVALID {
            let task_bytes = core::mem::size_of::<TraversalTask>() as u128;
            if fixed.checked_add(task_bytes)? > retained_limit_bytes {
                return None;
            }
            tasks.try_reserve_exact(1).ok()?;
            tasks.push(TraversalTask {
                family: root_family,
                continuations: [FAMILY_INVALID; MAX_BOARD64_PIECES],
                depth: 0,
                continuation_count: 0,
            });
        }
        let enumerator = Self {
            targets,
            family: Arc::new(family),
            tasks,
            rows: [0; MAX_BOARD64_PIECES],
            target_depth,
            retained_limit_bytes: Some(retained_limit_bytes),
            #[cfg(feature = "parallel")]
            candidate_count: None,
        };
        (enumerator.retained_bytes() as u128 <= retained_limit_bytes).then_some(enumerator)
    }

    #[cfg(feature = "parallel")]
    // Failure returns ownership of the enumerator so the caller can continue serially.
    #[allow(clippy::result_large_err)]
    fn into_parallel_enumerators(
        self,
        desired_partition_count: usize,
    ) -> Result<(Vec<Self>, usize), Self> {
        if self.tasks.len() != 1 {
            return Err(self);
        }
        let Some(path_counts) = self.family.path_count_table() else {
            return Err(self);
        };
        let task = self.tasks[0];
        let mut seeds = vec![TraversalSeed {
            task,
            rows: self.rows,
            weight: traversal_weight(task, &path_counts),
            splittable: true,
        }];
        while seeds.len() < desired_partition_count {
            let Some((index, seed)) = seeds
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, seed)| seed.splittable)
                .max_by_key(|(_, seed)| seed.weight)
            else {
                break;
            };
            match split_traversal_seed(&self.family, &path_counts, seed) {
                Some((left, right)) => {
                    if seeds.try_reserve(1).is_err() {
                        return Err(self);
                    }
                    seeds[index] = left;
                    seeds.push(right);
                }
                None => seeds[index].splittable = false,
            }
        }
        let shared_family_bytes = self.family.retained_bytes();
        let mut enumerators = Vec::new();
        if enumerators.try_reserve_exact(seeds.len()).is_err() {
            return Err(self);
        }
        for seed in seeds {
            enumerators.push(Self {
                targets: Arc::clone(&self.targets),
                family: Arc::clone(&self.family),
                tasks: vec![seed.task],
                rows: seed.rows,
                target_depth: self.target_depth,
                retained_limit_bytes: None,
                #[cfg(feature = "parallel")]
                candidate_count: Some(seed.weight),
            });
        }
        Ok((enumerators, shared_family_bytes))
    }

    fn next_candidate(
        &mut self,
        catalog: &GeometryCatalog,
    ) -> Result<Option<GeometryCandidate>, ()> {
        while let Some(mut task) = self.tasks.pop() {
            loop {
                if task.family == FAMILY_INVALID {
                    break;
                }
                if task.family == FAMILY_EMPTY {
                    if task.continuation_count != 0 {
                        task.continuation_count -= 1;
                        task.family = task.continuations[task.continuation_count as usize];
                        continue;
                    }
                    if task.depth != self.target_depth {
                        return Err(());
                    }
                    return self.candidate(catalog, task.depth).map(Some).ok_or(());
                }

                let node = self.family.node(task.family).ok_or(())?;
                match node.kind {
                    FamilyNodeKind::Append => {
                        if task.depth >= self.target_depth
                            || node.row_id as usize >= catalog.skeleton_count()
                        {
                            return Err(());
                        }
                        self.rows[task.depth as usize] = node.row_id;
                        task.depth += 1;
                        task.family = node.left;
                    }
                    FamilyNodeKind::Union => {
                        let mut right = task;
                        right.family = node.right;
                        self.push_task(right)?;
                        task.family = node.left;
                    }
                    FamilyNodeKind::Product => {
                        if task.continuation_count as usize >= MAX_BOARD64_PIECES {
                            return Err(());
                        }
                        task.continuations[task.continuation_count as usize] = node.right;
                        task.continuation_count += 1;
                        task.family = node.left;
                    }
                }
            }
        }
        Ok(None)
    }

    fn push_task(&mut self, task: TraversalTask) -> Result<(), ()> {
        // The family is immutable during enumeration. Recounting all its
        // chunks at every Union made candidate emission O(unions * chunks),
        // even without a retained-memory limit. Only a stack allocation can
        // increase retained bytes here; reuse already-admitted capacity.
        if self.tasks.len() == self.tasks.capacity() {
            if let Some(limit) = self.retained_limit_bytes {
                let projected = (self.retained_bytes() as u128)
                    .checked_add(core::mem::size_of::<TraversalTask>() as u128)
                    .ok_or(())?;
                if projected > limit {
                    return Err(());
                }
            }
            self.tasks.try_reserve_exact(1).map_err(|_| ())?;
            if self
                .retained_limit_bytes
                .is_some_and(|limit| self.retained_bytes() as u128 > limit)
            {
                return Err(());
            }
        }
        self.tasks.push(task);
        Ok(())
    }

    fn candidate(&self, catalog: &GeometryCatalog, row_count: u8) -> Option<GeometryCandidate> {
        let rows = &self.rows[..row_count as usize];
        let mut counts = [0_u8; 7];
        for row_id in rows {
            counts[piece_index(catalog.skeleton(*row_id).piece)] += 1;
        }
        let key = PieceMultisetKey::from_counts(counts);
        let target = self
            .targets
            .binary_search_by_key(&key, |target| target.key)
            .ok()
            .and_then(|index| self.targets.get(index))?;
        GeometryCandidate::from_rows(catalog, target.pattern_index_id, rows)
    }

    fn retained_bytes(&self) -> usize {
        usize::from(Arc::strong_count(&self.targets) == 1) * target_bytes(&self.targets)
            + usize::from(Arc::strong_count(&self.family) == 1)
                * (core::mem::size_of::<GeometrySolutionFamily>() + self.family.retained_bytes())
            + self.tasks.capacity() * core::mem::size_of::<TraversalTask>()
    }
}

#[cfg(test)]
mod family_enumerator_stack_tests {
    use super::*;

    fn enumerator() -> FamilyEnumerator {
        FamilyEnumerator::new(
            Arc::from([]),
            GeometrySolutionFamily::new(),
            FAMILY_EMPTY,
            0,
        )
    }

    #[test]
    fn admitted_stack_capacity_is_reused_at_exact_retained_limit() {
        let mut enumerator = enumerator();
        let task = enumerator.tasks.pop().unwrap();
        let capacity = enumerator.tasks.capacity();
        let admitted = enumerator.retained_bytes() as u128;
        enumerator.retained_limit_bytes = Some(admitted);
        for _ in 0..capacity {
            enumerator
                .push_task(task)
                .expect("already admitted stack storage");
        }
        assert_eq!(enumerator.tasks.len(), capacity);
        assert!(enumerator.push_task(task).is_err());
        assert_eq!(enumerator.tasks.capacity(), capacity);
        assert_eq!(enumerator.retained_bytes() as u128, admitted);
    }

    #[test]
    fn stack_growth_needs_only_its_actual_additional_capacity() {
        let mut enumerator = enumerator();
        let task = enumerator.tasks[0];
        while enumerator.tasks.len() < enumerator.tasks.capacity() {
            enumerator.tasks.push(task);
        }
        let admitted = enumerator.retained_bytes() as u128;
        enumerator.retained_limit_bytes =
            Some(admitted + core::mem::size_of::<TraversalTask>() as u128);
        enumerator
            .push_task(task)
            .expect("one exact extra stack slot");
        assert!(enumerator.retained_bytes() as u128 <= enumerator.retained_limit_bytes.unwrap());
    }
}

#[cfg(feature = "parallel")]
fn split_traversal_seed(
    family: &GeometrySolutionFamily,
    path_counts: &[u128],
    mut seed: TraversalSeed,
) -> Option<(TraversalSeed, TraversalSeed)> {
    loop {
        if seed.task.family == FAMILY_INVALID {
            return None;
        }
        if seed.task.family == FAMILY_EMPTY {
            if seed.task.continuation_count == 0 {
                return None;
            }
            seed.task.continuation_count -= 1;
            seed.task.family = seed.task.continuations[seed.task.continuation_count as usize];
            continue;
        }
        let node = family.node(seed.task.family)?;
        match node.kind {
            FamilyNodeKind::Append => {
                let depth = usize::from(seed.task.depth);
                if depth >= seed.rows.len() {
                    return None;
                }
                seed.rows[depth] = node.row_id;
                seed.task.depth += 1;
                seed.task.family = node.left;
            }
            FamilyNodeKind::Union => {
                let mut left = seed;
                let mut right = seed;
                left.task.family = node.left;
                right.task.family = node.right;
                left.weight = traversal_weight(left.task, path_counts);
                right.weight = traversal_weight(right.task, path_counts);
                return Some((left, right));
            }
            FamilyNodeKind::Product => {
                let index = usize::from(seed.task.continuation_count);
                if index >= seed.task.continuations.len() {
                    return None;
                }
                seed.task.continuations[index] = node.right;
                seed.task.continuation_count += 1;
                seed.task.family = node.left;
            }
        }
    }
}

#[cfg(feature = "parallel")]
fn traversal_weight(task: TraversalTask, path_counts: &[u128]) -> u128 {
    let mut weight = path_counts.get(task.family as usize).copied().unwrap_or(0);
    for family in task.continuations[..usize::from(task.continuation_count)]
        .iter()
        .copied()
    {
        weight = weight.saturating_mul(path_counts.get(family as usize).copied().unwrap_or(0));
    }
    weight
}

fn target_bytes(targets: &[TargetGroup]) -> usize {
    core::mem::size_of_val(targets)
}

pub(super) fn compile_admissible_prefixes(targets: &[TargetGroup]) -> Vec<u32> {
    let mut prefixes = Vec::new();
    let mut counts = [0_u8; 7];
    for target in targets {
        enumerate_count_prefixes(target.key.counts(), 0, &mut counts, &mut prefixes);
    }
    prefixes.sort_unstable();
    prefixes.dedup();
    prefixes
}

fn compile_admissible_prefixes_checked(
    targets: &[TargetGroup],
) -> Result<Vec<u32>, WasmExactSearchError> {
    let capacity = targets
        .iter()
        .try_fold(0_usize, |total, target| {
            target
                .key
                .counts()
                .into_iter()
                .try_fold(1_usize, |count, piece_count| {
                    count.checked_mul(usize::from(piece_count).checked_add(1)?)
                })
                .and_then(|count| total.checked_add(count))
        })
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_geometry_prefix_projection_overflow",
        ))?;
    let mut prefixes = Vec::new();
    prefixes.try_reserve_exact(capacity).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_geometry_prefix_storage_unavailable")
    })?;
    let mut counts = [0_u8; 7];
    for target in targets {
        enumerate_count_prefixes(target.key.counts(), 0, &mut counts, &mut prefixes);
    }
    prefixes.sort_unstable();
    prefixes.dedup();
    Ok(prefixes)
}

fn feasible_piece_mask_for(admissible_prefixes: &[u32], used_counts: [u8; 7]) -> u8 {
    let mut mask = 0_u8;
    for piece in 0..used_counts.len() {
        let mut counts = used_counts;
        counts[piece] = counts[piece].saturating_add(1);
        if admissible_prefixes
            .binary_search(&pack_piece_counts(counts))
            .is_ok()
        {
            mask |= 1_u8 << piece;
        }
    }
    mask
}

fn enumerate_count_prefixes(
    target: [u8; 7],
    piece_index: usize,
    counts: &mut [u8; 7],
    output: &mut Vec<u32>,
) {
    if piece_index == counts.len() {
        output.push(pack_piece_counts(*counts));
        return;
    }
    for count in 0..=target[piece_index] {
        counts[piece_index] = count;
        enumerate_count_prefixes(target, piece_index + 1, counts, output);
    }
}

pub(super) fn pack_piece_counts(counts: [u8; 7]) -> u32 {
    counts
        .iter()
        .copied()
        .enumerate()
        .fold(0_u32, |packed, (index, count)| {
            packed | (u32::from(count) << (index * 4))
        })
}

pub(super) fn unpack_piece_counts(packed: u32) -> [u8; 7] {
    std::array::from_fn(|index| ((packed >> (index * 4)) & 0x0f) as u8)
}

pub(super) fn add_packed_piece(packed: u32, piece: usize) -> Option<u32> {
    let shift = piece.checked_mul(4)?;
    let count = (packed >> shift) & 0x0f;
    (count < 0x0f).then_some(packed + (1_u32 << shift))
}

fn signature_total_count(signature: u32) -> u8 {
    (0..7)
        .map(|piece| ((signature >> (piece * 4)) & 0x0f) as u8)
        .sum()
}

fn add_signature(counts: &mut [u8; 7], signature: u32) {
    for (piece, count) in counts.iter_mut().enumerate() {
        *count += ((signature >> (piece * 4)) & 0x0f) as u8;
    }
}

fn remove_signature(counts: &mut [u8; 7], signature: u32) {
    for (piece, count) in counts.iter_mut().enumerate() {
        *count -= ((signature >> (piece * 4)) & 0x0f) as u8;
    }
}

// Geometry completion owns the result and moves it directly to the next stage.
#[allow(clippy::large_enum_variant)]
pub(super) enum GeometryAdvance {
    Pending,
    Candidate(GeometryCandidate),
    Complete,
    ResourceIncomplete(&'static str),
}

pub(super) struct GeometrySearch {
    group_pattern_index_bytes: usize,
    shared_family_bytes: usize,
    target_preparation: Option<TargetGroupCompileSession>,
    target_preparation_mode: Option<TargetGroupPreparationMode>,
    compiler: Option<FamilyCompiler>,
    enumerator: Option<FamilyEnumerator>,
    expanded_nodes: usize,
    peak_frontier: usize,
    candidate_family_count: Option<u128>,
    domain_pruned_states: usize,
    hall_pruned_states: usize,
    column_pruned_states: usize,
    component_compositions: usize,
    tablebase_pruned_states: usize,
    external_targets: Option<Arc<[TargetGroup]>>,
    resource_authoritative: bool,
}

#[derive(Clone, Debug)]
pub(super) struct SharedTargetGroups {
    targets: Arc<[TargetGroup]>,
    group_pattern_index_bytes: usize,
}

enum TargetGroupPreparationMode {
    Internal {
        required_cells: u64,
        tablebase: Option<Arc<Pc4CompactTablebase>>,
    },
    External,
}

// This internal advance enum transfers prepared target storage exactly once.
#[allow(clippy::large_enum_variant)]
enum TargetGroupCompileAdvance {
    Pending,
    Complete(SharedTargetGroups),
}

// Index preparation retains one active stage in place to avoid per-transition allocation.
#[allow(clippy::large_enum_variant)]
enum TargetGroupIndexStage {
    SelectPatternIds {
        target_index: usize,
        next_pattern_id: usize,
        pattern_ids: Vec<u32>,
    },
    Compile {
        target_index: usize,
        session: PatternPiecePositionIndexCompileSession,
    },
    Finished,
}

/// Cooperative compiler for the queue-language indexes attached to exact-PC
/// target groups. Target identity and ordering are established up front, while
/// the potentially multi-million-pattern scan and bit-slice population remain
/// behind an explicit cursor. This keeps browser cancellation/event delivery
/// responsive without changing ILC geometry, target IDs, or coverage meaning.
struct TargetGroupCompileSession {
    universe: MaterializedPatternUniverse,
    targets: Vec<TargetGroup>,
    next_target: usize,
    stage: TargetGroupIndexStage,
    retained_upper_bound_bytes: usize,
}

impl TargetGroupCompileSession {
    fn new(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        retained_upper_bound_bytes: usize,
    ) -> Result<Self, WasmExactSearchError> {
        let mut targets = target_groups_without_pattern_indexes(family)?;
        targets.sort_unstable_by_key(|target| target.key);
        for (target_index, target) in targets.iter_mut().enumerate() {
            target.pattern_index_id = u32::try_from(target_index).map_err(|_| {
                WasmExactSearchError::InvalidProblem("wasm_pattern_index_identity_overflow")
            })?;
        }
        Ok(Self {
            universe: universe.clone(),
            targets,
            next_target: 0,
            stage: TargetGroupIndexStage::Finished,
            retained_upper_bound_bytes,
        })
    }

    fn advance(&mut self) -> Result<TargetGroupCompileAdvance, WasmExactSearchError> {
        loop {
            let stage = core::mem::replace(&mut self.stage, TargetGroupIndexStage::Finished);
            match stage {
                TargetGroupIndexStage::Finished => {
                    if self.next_target == self.targets.len() {
                        let targets = core::mem::take(&mut self.targets);
                        let target_nested_bytes = checked_target_nested_retained_bytes(&targets)
                            .and_then(|bytes| usize::try_from(bytes).ok())
                            .ok_or(WasmExactSearchError::InvalidProblem(
                                "wasm_pattern_index_retained_projection_overflow",
                            ))?;
                        return Ok(TargetGroupCompileAdvance::Complete(SharedTargetGroups {
                            targets: targets.into(),
                            group_pattern_index_bytes: target_nested_bytes,
                        }));
                    }
                    if let Some(index) = self.targets[..self.next_target].iter().find_map(|prior| {
                        Arc::ptr_eq(
                            &prior.possible_patterns,
                            &self.targets[self.next_target].possible_patterns,
                        )
                        .then(|| prior.pattern_index.as_ref().map(Arc::clone))
                        .flatten()
                    }) {
                        self.targets[self.next_target].pattern_index = Some(index);
                        self.next_target += 1;
                        return Ok(TargetGroupCompileAdvance::Pending);
                    }
                    let member_count = self.targets[self.next_target]
                        .possible_patterns
                        .count_ones() as usize;
                    let mut pattern_ids = Vec::new();
                    pattern_ids.try_reserve_exact(member_count).map_err(|_| {
                        WasmExactSearchError::InvalidProblem(
                            "wasm_pattern_index_storage_unavailable",
                        )
                    })?;
                    self.stage = TargetGroupIndexStage::SelectPatternIds {
                        target_index: self.next_target,
                        next_pattern_id: 0,
                        pattern_ids,
                    };
                }
                TargetGroupIndexStage::SelectPatternIds {
                    target_index,
                    mut next_pattern_id,
                    mut pattern_ids,
                } => {
                    let pattern_count = self.universe.pattern_count();
                    let end = next_pattern_id
                        .saturating_add(TARGET_PATTERN_SCAN_WORK_BUDGET)
                        .min(pattern_count);
                    for pattern in self.targets[target_index]
                        .possible_patterns
                        .covered_patterns_in_range(next_pattern_id, end)
                    {
                        pattern_ids.push(u32::try_from(pattern.index()).map_err(|_| {
                            WasmExactSearchError::InvalidProblem(
                                "wasm_pattern_index_identity_overflow",
                            )
                        })?);
                    }
                    next_pattern_id = end;
                    if next_pattern_id != pattern_count {
                        self.stage = TargetGroupIndexStage::SelectPatternIds {
                            target_index,
                            next_pattern_id,
                            pattern_ids,
                        };
                        return Ok(TargetGroupCompileAdvance::Pending);
                    }
                    let session = PatternPiecePositionIndexCompileSession::new_for_pattern_ids(
                        self.universe.clone(),
                        pattern_ids,
                    )
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem("wasm_pattern_index_compile_failed")
                    })?;
                    self.stage = TargetGroupIndexStage::Compile {
                        target_index,
                        session,
                    };
                }
                TargetGroupIndexStage::Compile {
                    target_index,
                    mut session,
                } => match session
                    .advance(TARGET_PATTERN_SCAN_WORK_BUDGET)
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem("wasm_pattern_index_compile_failed")
                    })? {
                    PatternPiecePositionIndexCompileAdvance::Pending => {
                        self.stage = TargetGroupIndexStage::Compile {
                            target_index,
                            session,
                        };
                        return Ok(TargetGroupCompileAdvance::Pending);
                    }
                    PatternPiecePositionIndexCompileAdvance::Complete(index) => {
                        self.targets[target_index].pattern_index = Some(Arc::new(index));
                        self.next_target = target_index + 1;
                        self.stage = TargetGroupIndexStage::Finished;
                        return Ok(TargetGroupCompileAdvance::Pending);
                    }
                },
            }
        }
    }

    const fn retained_upper_bound_bytes(&self) -> usize {
        self.retained_upper_bound_bytes
    }
}

fn target_groups_without_pattern_indexes(
    family: &PackingMultisetFamily,
) -> Result<Vec<TargetGroup>, WasmExactSearchError> {
    let mut targets = Vec::<TargetGroup>::new();
    targets.try_reserve_exact(family.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_pattern_index_storage_unavailable")
    })?;
    for group in family.groups() {
        targets.push(TargetGroup {
            key: group.key(),
            pattern_index_id: 0,
            possible_patterns: group.shared_pattern_bits(),
            pattern_index: None,
        });
    }
    Ok(targets)
}

impl SharedTargetGroups {
    pub fn compile(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        compile_pattern_indexes: bool,
    ) -> Result<Self, WasmExactSearchError> {
        let (targets, group_pattern_index_bytes) =
            compile_target_groups(universe, family, compile_pattern_indexes)?;
        Ok(Self {
            targets: targets.into(),
            group_pattern_index_bytes,
        })
    }

    pub(super) fn targets(&self) -> &[TargetGroup] {
        self.targets.as_ref()
    }
}

#[cfg(feature = "parallel")]
pub(super) struct ParallelGeometryPlan {
    pub targets: Arc<[TargetGroup]>,
    pub searches: Vec<GeometrySearch>,
    pub group_pattern_index_bytes: usize,
    pub shared_family_bytes: usize,
}

pub(super) fn compile_target_groups(
    universe: &MaterializedPatternUniverse,
    family: &PackingMultisetFamily,
    compile_pattern_indexes: bool,
) -> Result<(Vec<TargetGroup>, usize), WasmExactSearchError> {
    let mut targets = Vec::<TargetGroup>::new();
    targets.try_reserve_exact(family.len()).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_pattern_index_storage_unavailable")
    })?;
    for group in family.groups() {
        let membership = group.shared_pattern_bits();
        let pattern_index = if compile_pattern_indexes {
            if let Some(index) = targets.iter().find_map(|target| {
                Arc::ptr_eq(&target.possible_patterns, &membership)
                    .then(|| target.pattern_index.as_ref().map(Arc::clone))
                    .flatten()
            }) {
                Some(index)
            } else {
                let index = Arc::new(
                    PatternPiecePositionIndex::compile_subset_before(
                        universe,
                        membership.as_ref(),
                        universe.pattern_count(),
                    )
                    .map_err(|_| {
                        WasmExactSearchError::InvalidProblem("wasm_pattern_index_compile_failed")
                    })?,
                );
                Some(index)
            }
        } else {
            None
        };
        targets.push(TargetGroup {
            key: group.key(),
            pattern_index_id: 0,
            possible_patterns: membership,
            pattern_index,
        });
    }
    targets.sort_unstable_by_key(|target| target.key);
    for (target_index, target) in targets.iter_mut().enumerate() {
        target.pattern_index_id = u32::try_from(target_index).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_pattern_index_identity_overflow")
        })?;
    }
    let target_nested_bytes = checked_target_nested_retained_bytes(&targets)
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_pattern_index_retained_projection_overflow",
        ))?;
    Ok((targets, target_nested_bytes))
}

/// Builds target groups only after their full constructor peak fits the same
/// memory surface as the surrounding exact-search session. The family and its
/// membership bitsets are already included in `already_retained_bytes`; this
/// projection therefore counts target slots, new position indexes, conversion
/// scratch, and the Vec-to-Arc target copy without double-counting membership.
// Retained as the bounded constructor for embedders that do not own a full session.
#[allow(dead_code)]
pub(super) fn compile_target_groups_with_memory_limit(
    universe: &MaterializedPatternUniverse,
    family: &PackingMultisetFamily,
    compile_pattern_indexes: bool,
    already_retained_bytes: u128,
    max_memory_bytes: u128,
) -> Result<(Vec<TargetGroup>, usize), WasmExactSearchError> {
    let peak =
        checked_target_group_build_peak_additional_bytes(universe, family, compile_pattern_indexes)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_pattern_index_projection_overflow",
            ))?;
    let required =
        already_retained_bytes
            .checked_add(peak)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_pattern_index_projection_overflow",
            ))?;
    if required > max_memory_bytes {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_pattern_index_memory_capacity_exceeded",
        ));
    }
    compile_target_groups(universe, family, compile_pattern_indexes)
}

pub(super) fn checked_target_group_build_peak_additional_bytes(
    universe: &MaterializedPatternUniverse,
    family: &PackingMultisetFamily,
    compile_pattern_indexes: bool,
) -> Option<u128> {
    let target_slot_bytes =
        (family.len() as u128).checked_mul(core::mem::size_of::<TargetGroup>() as u128)?;
    // Vec -> Arc<[TargetGroup]> may keep both payloads live during conversion.
    let mut bytes = target_slot_bytes.checked_mul(2)?;
    let admissible_prefix_count = checked_admissible_prefix_count_upper_bound(family)?;
    let target_depth = family.envelope().total_count();
    bytes = bytes
        .checked_add(admissible_prefix_count.checked_mul(core::mem::size_of::<u32>() as u128)?)?
        .checked_add(
            (u128::from(target_depth).checked_add(1)?)
                .checked_mul(core::mem::size_of::<CompileFrame>() as u128)?,
        )?
        .checked_add(
            (FAMILY_INTERN_INITIAL_SLOT_COUNT as u128)
                .checked_mul(core::mem::size_of::<u32>() as u128)?,
        )?;
    let residual_bucket_count = match target_depth {
        0..=5 => 1_u128 << 12,
        6..=7 => 1_u128 << 14,
        8..=10 => 1_u128 << 18,
        _ => 1_u128 << 20,
    };
    bytes = bytes
        .checked_add(residual_bucket_count.checked_mul(core::mem::size_of::<u32>() as u128)?)?;
    let mut max_sequence_scratch_bytes = 0_u128;
    for (group_index, group) in family.groups().iter().enumerate() {
        let membership = group.shared_pattern_bits();
        let already_compiled = family.groups()[..group_index].iter().any(|prior| {
            let prior = prior.shared_pattern_bits();
            Arc::ptr_eq(&prior, &membership)
        });
        if already_compiled || !compile_pattern_indexes {
            continue;
        }
        let member_count = usize::try_from(membership.count_ones()).ok()?;
        let sequence_len = max_covered_sequence_len(universe, membership.as_ref());
        let word_count = member_count.div_ceil(u64::BITS as usize);
        let local_id_bytes =
            (member_count as u128).checked_mul(core::mem::size_of::<u32>() as u128)?;
        let position_word_bytes = (sequence_len as u128)
            .checked_mul(7)?
            .checked_mul(word_count as u128)?
            .checked_mul(core::mem::size_of::<u64>() as u128)?;
        bytes = bytes
            .checked_add(core::mem::size_of::<PatternPiecePositionIndex>() as u128)?
            .checked_add(local_id_bytes)?
            .checked_add(position_word_bytes)?;
        max_sequence_scratch_bytes =
            max_sequence_scratch_bytes.max((sequence_len as u128).checked_mul(
                core::mem::size_of::<clearra_core_domain::piece::piece_kind::PieceKind>() as u128,
            )?);
    }
    bytes.checked_add(max_sequence_scratch_bytes)
}

fn max_covered_sequence_len(
    universe: &MaterializedPatternUniverse,
    membership: &PatternBitSet,
) -> usize {
    match universe.structure() {
        MaterializedPatternUniverseStructure::Standard7BagLexicographic { sequence_len }
        | MaterializedPatternUniverseStructure::ObservedStandard7BagLexicographic {
            sequence_len,
            ..
        }
        | MaterializedPatternUniverseStructure::FactorizedQueueExpression { sequence_len } => {
            usize::from(sequence_len) * usize::from(!membership.is_empty())
        }
        MaterializedPatternUniverseStructure::Explicit => membership
            .covered_patterns_before(universe.pattern_count())
            .map(|pattern| universe.sequence_len_at(pattern.index()))
            .max()
            .unwrap_or(0),
    }
}

#[cfg(test)]
mod target_group_memory_admission_tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::{
        pattern::pattern_bitset::PatternBitSet,
        universe::{
            pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
        },
    };
    use clearra_supply::{
        hold_automaton::{HoldAutomatonState, SupplyProvenanceId},
        pattern_universe::{
            MaterializedPatternUniverse, PackingHoldProjection, PatternUniverseMaterializer,
        },
        piece_source::PieceSourceId,
    };

    use super::{
        checked_target_group_build_peak_additional_bytes, compile_target_groups,
        compile_target_groups_with_memory_limit, max_covered_sequence_len,
        TargetGroupCompileAdvance, TargetGroupCompileSession, WasmExactSearchError,
    };

    #[test]
    fn sequence_length_projection_iterates_dense_sparse_and_high_cardinality_membership() {
        let universe = PatternUniverseMaterializer::standard_7_bag(14, 262_145, 91)
            .expect("lazy high-cardinality universe");
        assert_eq!(universe.pattern_count(), 262_145);

        let dense = PatternBitSet::all(universe.pattern_count());
        assert_eq!(max_covered_sequence_len(&universe, &dense), 14);

        let sparse =
            PatternBitSet::from_pattern_indices(universe.pattern_count(), vec![0, 65_537, 262_144])
                .expect("sparse membership");
        assert_eq!(max_covered_sequence_len(&universe, &sparse), 14);
    }

    #[test]
    fn target_group_constructor_accepts_exact_cap_and_rejects_one_byte_short() {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(92),
            PatternWeightModelId::new(92),
            vec![
                vec![PieceKind::I, PieceKind::O],
                vec![PieceKind::O, PieceKind::I],
                vec![PieceKind::T, PieceKind::T],
            ],
            vec![
                ProbabilityValue::new(1.0 / 3.0).expect("probability"),
                ProbabilityValue::new(1.0 / 3.0).expect("probability"),
                ProbabilityValue::new(1.0 / 3.0).expect("probability"),
            ],
            3,
            true,
            None,
        )
        .expect("mixed dense and sparse universe");
        let initial_hold = HoldAutomatonState::new(
            PieceSourceId::new(92),
            0,
            None,
            0,
            0,
            SupplyProvenanceId(92),
        );
        let family = universe.packing_multiset_family_for_execution(
            2,
            initial_hold,
            false,
            PackingHoldProjection::PreserveFinalHoldLanguage,
        );
        let peak = checked_target_group_build_peak_additional_bytes(&universe, &family, true)
            .expect("checked target construction peak");
        let already_retained = 37_u128;

        let below_cap = compile_target_groups_with_memory_limit(
            &universe,
            &family,
            true,
            already_retained,
            already_retained + peak - 1,
        );
        assert!(matches!(
            below_cap,
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_pattern_index_memory_capacity_exceeded"
            ))
        ));

        let (targets, retained_bytes) = compile_target_groups_with_memory_limit(
            &universe,
            &family,
            true,
            already_retained,
            already_retained + peak,
        )
        .expect("exact cap admits target construction");
        assert_eq!(targets.len(), family.len());
        assert!(retained_bytes > 0);
    }

    #[test]
    fn target_group_pattern_indexes_compile_in_bounded_steps_with_eager_parity() {
        let universe = PatternUniverseMaterializer::standard_7_bag(14, 16_385, 93)
            .expect("bounded target-index universe");
        assert_eq!(universe.pattern_count(), 16_385);
        let initial_hold = HoldAutomatonState::new(
            PieceSourceId::new(93),
            0,
            None,
            0,
            0,
            SupplyProvenanceId(93),
        );
        let family = universe.packing_multiset_family_for_execution(
            2,
            initial_hold,
            false,
            PackingHoldProjection::PreserveFinalHoldLanguage,
        );
        let eager = compile_target_groups(&universe, &family, true)
            .expect("eager target groups remain the parity oracle");
        let peak = checked_target_group_build_peak_additional_bytes(&universe, &family, true)
            .and_then(|bytes| usize::try_from(bytes).ok())
            .expect("bounded target-index projection");
        let mut session = TargetGroupCompileSession::new(&universe, &family, peak)
            .expect("deferred target groups");
        assert!(matches!(
            session.advance().expect("first bounded step"),
            TargetGroupCompileAdvance::Pending
        ));

        let mut calls = 1usize;
        let deferred = loop {
            calls += 1;
            match session.advance().expect("deferred target-index step") {
                TargetGroupCompileAdvance::Pending => {}
                TargetGroupCompileAdvance::Complete(shared) => break shared,
            }
        };
        assert!(calls > 4, "large index must not complete in one host call");
        assert_eq!(deferred.group_pattern_index_bytes, eager.1);
        assert_eq!(deferred.targets.len(), eager.0.len());
        for (deferred, eager) in deferred.targets.iter().zip(&eager.0) {
            assert_eq!(deferred.key, eager.key);
            assert_eq!(deferred.pattern_index_id, eager.pattern_index_id);
            assert_eq!(deferred.possible_patterns, eager.possible_patterns);
            assert_eq!(deferred.pattern_index, eager.pattern_index);
        }
    }
}

// Mirrors the initial intern table in geometry_family without exposing its
// implementation-private constant as public API.
const FAMILY_INTERN_INITIAL_SLOT_COUNT: usize = 4096;

fn checked_admissible_prefix_count_upper_bound(family: &PackingMultisetFamily) -> Option<u128> {
    family.groups().iter().try_fold(0_u128, |total, group| {
        group
            .key()
            .counts()
            .into_iter()
            .try_fold(1_u128, |count, piece_count| {
                count.checked_mul(u128::from(piece_count).checked_add(1)?)
            })
            .and_then(|count| total.checked_add(count))
    })
}

fn checked_target_nested_retained_bytes(targets: &[TargetGroup]) -> Option<u128> {
    let mut bytes = 0_u128;
    for (index, target) in targets.iter().enumerate() {
        if !targets[..index]
            .iter()
            .any(|prior| Arc::ptr_eq(&prior.possible_patterns, &target.possible_patterns))
        {
            bytes = bytes
                .checked_add(core::mem::size_of::<PatternBitSet>() as u128)?
                .checked_add(target.possible_patterns.retained_bytes() as u128)?;
        }
        let Some(pattern_index) = target.pattern_index.as_ref() else {
            continue;
        };
        if targets[..index].iter().any(|prior| {
            prior
                .pattern_index
                .as_ref()
                .is_some_and(|prior| Arc::ptr_eq(prior, pattern_index))
        }) {
            continue;
        }
        bytes = bytes
            .checked_add(core::mem::size_of::<PatternPiecePositionIndex>() as u128)?
            .checked_add(pattern_index.retained_bytes() as u128)?;
    }
    Some(bytes)
}

impl GeometrySearch {
    #[cfg(feature = "parallel")]
    pub fn placeholder() -> Self {
        Self {
            group_pattern_index_bytes: 0,
            shared_family_bytes: 0,
            target_preparation: None,
            target_preparation_mode: None,
            compiler: None,
            enumerator: None,
            expanded_nodes: 0,
            peak_frontier: 0,
            candidate_family_count: Some(0),
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            tablebase_pruned_states: 0,
            external_targets: None,
            resource_authoritative: false,
        }
    }

    pub fn new(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        required_cells: u64,
        compile_pattern_indexes: bool,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_deferred(
            universe,
            family,
            required_cells,
            compile_pattern_indexes,
            None,
            None,
        )
    }

    pub fn new_with_memory_limit(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        required_cells: u64,
        compile_pattern_indexes: bool,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_deferred(
            universe,
            family,
            required_cells,
            compile_pattern_indexes,
            None,
            Some((already_retained_bytes, max_memory_bytes)),
        )
    }

    pub fn new_with_tablebase(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        required_cells: u64,
        compile_pattern_indexes: bool,
        tablebase: Arc<Pc4CompactTablebase>,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_deferred(
            universe,
            family,
            required_cells,
            compile_pattern_indexes,
            Some(tablebase),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_tablebase_and_memory_limit(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        required_cells: u64,
        compile_pattern_indexes: bool,
        tablebase: Arc<Pc4CompactTablebase>,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<Self, WasmExactSearchError> {
        Self::new_deferred(
            universe,
            family,
            required_cells,
            compile_pattern_indexes,
            Some(tablebase),
            Some((already_retained_bytes, max_memory_bytes)),
        )
    }

    fn new_deferred(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        required_cells: u64,
        compile_pattern_indexes: bool,
        tablebase: Option<Arc<Pc4CompactTablebase>>,
        memory_limit: Option<(u128, u128)>,
    ) -> Result<Self, WasmExactSearchError> {
        let mut preparation_peak = checked_target_group_build_peak_additional_bytes(
            universe,
            family,
            compile_pattern_indexes,
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_pattern_index_projection_overflow",
        ))?;
        if compile_pattern_indexes {
            let universe_clone_peak = universe
                .checked_retained_capacity_bytes()
                .and_then(|bytes| bytes.checked_mul(2))
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_pattern_index_projection_overflow",
                ))?;
            preparation_peak = preparation_peak.checked_add(universe_clone_peak).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_pattern_index_projection_overflow"),
            )?;
        }
        if let Some((already_retained_bytes, max_memory_bytes)) = memory_limit {
            let required = already_retained_bytes.checked_add(preparation_peak).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_pattern_index_projection_overflow"),
            )?;
            if required > max_memory_bytes {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_pattern_index_memory_capacity_exceeded",
                ));
            }
        }
        if !compile_pattern_indexes {
            let mut targets = target_groups_without_pattern_indexes(family)?;
            targets.sort_unstable_by_key(|target| target.key);
            for (target_index, target) in targets.iter_mut().enumerate() {
                target.pattern_index_id = u32::try_from(target_index).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_pattern_index_identity_overflow")
                })?;
            }
            let target_nested_bytes = checked_target_nested_retained_bytes(&targets)
                .and_then(|bytes| usize::try_from(bytes).ok())
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_pattern_index_retained_projection_overflow",
                ))?;
            let shared = SharedTargetGroups {
                targets: targets.into(),
                group_pattern_index_bytes: target_nested_bytes,
            };
            return if memory_limit.is_some() {
                Self::try_new_shared_with_tablebase(required_cells, &shared, tablebase)
            } else {
                Ok(Self::new_shared_with_tablebase(
                    required_cells,
                    &shared,
                    tablebase,
                ))
            };
        }
        let retained_upper_bound_bytes = usize::try_from(preparation_peak).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_pattern_index_projection_overflow")
        })?;
        Ok(Self {
            group_pattern_index_bytes: 0,
            shared_family_bytes: 0,
            target_preparation: Some(TargetGroupCompileSession::new(
                universe,
                family,
                retained_upper_bound_bytes,
            )?),
            target_preparation_mode: Some(TargetGroupPreparationMode::Internal {
                required_cells,
                tablebase,
            }),
            compiler: None,
            enumerator: None,
            expanded_nodes: 0,
            peak_frontier: 0,
            candidate_family_count: None,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            tablebase_pruned_states: 0,
            external_targets: None,
            resource_authoritative: memory_limit.is_some(),
        })
    }

    pub fn new_shared(required_cells: u64, shared: &SharedTargetGroups) -> Self {
        Self::new_shared_with_tablebase(required_cells, shared, None)
    }

    fn new_shared_with_tablebase(
        required_cells: u64,
        shared: &SharedTargetGroups,
        tablebase: Option<Arc<Pc4CompactTablebase>>,
    ) -> Self {
        Self {
            group_pattern_index_bytes: shared.group_pattern_index_bytes,
            shared_family_bytes: 0,
            target_preparation: None,
            target_preparation_mode: None,
            compiler: Some(FamilyCompiler::new(
                required_cells,
                Arc::clone(&shared.targets),
                tablebase,
            )),
            enumerator: None,
            expanded_nodes: 0,
            peak_frontier: 0,
            candidate_family_count: None,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            tablebase_pruned_states: 0,
            external_targets: None,
            resource_authoritative: false,
        }
    }

    fn try_new_shared_with_tablebase(
        required_cells: u64,
        shared: &SharedTargetGroups,
        tablebase: Option<Arc<Pc4CompactTablebase>>,
    ) -> Result<Self, WasmExactSearchError> {
        Ok(Self {
            group_pattern_index_bytes: shared.group_pattern_index_bytes,
            shared_family_bytes: 0,
            target_preparation: None,
            target_preparation_mode: None,
            compiler: Some(FamilyCompiler::try_new(
                required_cells,
                Arc::clone(&shared.targets),
                tablebase,
            )?),
            enumerator: None,
            expanded_nodes: 0,
            peak_frontier: 0,
            candidate_family_count: None,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            tablebase_pruned_states: 0,
            external_targets: None,
            resource_authoritative: true,
        })
    }

    pub fn external(targets: Vec<TargetGroup>, group_pattern_index_bytes: usize) -> Self {
        let shared = SharedTargetGroups {
            targets: targets.into(),
            group_pattern_index_bytes,
        };
        Self::external_shared(&shared)
    }

    pub fn external_deferred(
        universe: &MaterializedPatternUniverse,
        family: &PackingMultisetFamily,
        compile_pattern_indexes: bool,
        memory_limit: Option<(u128, u128)>,
    ) -> Result<Self, WasmExactSearchError> {
        if !compile_pattern_indexes {
            let mut targets = target_groups_without_pattern_indexes(family)?;
            targets.sort_unstable_by_key(|target| target.key);
            for (target_index, target) in targets.iter_mut().enumerate() {
                target.pattern_index_id = u32::try_from(target_index).map_err(|_| {
                    WasmExactSearchError::InvalidProblem("wasm_pattern_index_identity_overflow")
                })?;
            }
            let target_nested_bytes = checked_target_nested_retained_bytes(&targets)
                .and_then(|bytes| usize::try_from(bytes).ok())
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_pattern_index_retained_projection_overflow",
                ))?;
            return Ok(Self::external(targets, target_nested_bytes));
        }
        let mut preparation_peak = checked_target_group_build_peak_additional_bytes(
            universe,
            family,
            compile_pattern_indexes,
        )
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_pattern_index_projection_overflow",
        ))?;
        let universe_clone_peak = universe
            .checked_retained_capacity_bytes()
            .and_then(|bytes| bytes.checked_mul(2))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "wasm_pattern_index_projection_overflow",
            ))?;
        preparation_peak = preparation_peak.checked_add(universe_clone_peak).ok_or(
            WasmExactSearchError::InvalidProblem("wasm_pattern_index_projection_overflow"),
        )?;
        if let Some((already_retained_bytes, max_memory_bytes)) = memory_limit {
            let required = already_retained_bytes.checked_add(preparation_peak).ok_or(
                WasmExactSearchError::InvalidProblem("wasm_pattern_index_projection_overflow"),
            )?;
            if required > max_memory_bytes {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_pattern_index_memory_capacity_exceeded",
                ));
            }
        }
        let retained_upper_bound_bytes = usize::try_from(preparation_peak).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_pattern_index_projection_overflow")
        })?;
        Ok(Self {
            group_pattern_index_bytes: 0,
            shared_family_bytes: 0,
            target_preparation: Some(TargetGroupCompileSession::new(
                universe,
                family,
                retained_upper_bound_bytes,
            )?),
            target_preparation_mode: Some(TargetGroupPreparationMode::External),
            compiler: None,
            enumerator: None,
            expanded_nodes: 0,
            peak_frontier: 0,
            candidate_family_count: None,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            tablebase_pruned_states: 0,
            external_targets: None,
            resource_authoritative: memory_limit.is_some(),
        })
    }

    pub fn external_shared(shared: &SharedTargetGroups) -> Self {
        Self {
            group_pattern_index_bytes: shared.group_pattern_index_bytes,
            shared_family_bytes: 0,
            target_preparation: None,
            target_preparation_mode: None,
            compiler: None,
            enumerator: None,
            expanded_nodes: 0,
            peak_frontier: 0,
            candidate_family_count: None,
            domain_pruned_states: 0,
            hall_pruned_states: 0,
            column_pruned_states: 0,
            component_compositions: 0,
            tablebase_pruned_states: 0,
            external_targets: Some(Arc::clone(&shared.targets)),
            resource_authoritative: false,
        }
    }

    pub fn advance(&mut self, catalog: &GeometryCatalog) -> GeometryAdvance {
        self.advance_internal(catalog, None)
    }

    pub fn advance_with_retained_limit(
        &mut self,
        catalog: &GeometryCatalog,
        retained_limit_bytes: u128,
    ) -> GeometryAdvance {
        if !self.resource_authoritative {
            return GeometryAdvance::ResourceIncomplete("geometry_memory_authority_not_connected");
        }
        self.advance_internal(catalog, Some(retained_limit_bytes))
    }

    fn advance_internal(
        &mut self,
        catalog: &GeometryCatalog,
        retained_limit_bytes: Option<u128>,
    ) -> GeometryAdvance {
        if let Some(mut preparation) = self.target_preparation.take() {
            if retained_limit_bytes
                .is_some_and(|limit| preparation.retained_upper_bound_bytes() as u128 > limit)
            {
                self.target_preparation = Some(preparation);
                return GeometryAdvance::ResourceIncomplete(
                    "wasm_pattern_index_memory_capacity_exceeded",
                );
            }
            match preparation.advance() {
                Ok(TargetGroupCompileAdvance::Pending) => {
                    self.target_preparation = Some(preparation);
                    return GeometryAdvance::Pending;
                }
                Err(error) => {
                    return GeometryAdvance::ResourceIncomplete(error.reason());
                }
                Ok(TargetGroupCompileAdvance::Complete(shared)) => {
                    self.group_pattern_index_bytes = shared.group_pattern_index_bytes;
                    let Some(mode) = self.target_preparation_mode.take() else {
                        return GeometryAdvance::ResourceIncomplete(
                            "geometry_target_preparation_mode_missing",
                        );
                    };
                    match mode {
                        TargetGroupPreparationMode::Internal {
                            required_cells,
                            tablebase,
                        } => {
                            self.compiler = if self.resource_authoritative {
                                match FamilyCompiler::try_new(
                                    required_cells,
                                    Arc::clone(&shared.targets),
                                    tablebase,
                                ) {
                                    Ok(compiler) => Some(compiler),
                                    Err(error) => {
                                        return GeometryAdvance::ResourceIncomplete(error.reason());
                                    }
                                }
                            } else {
                                Some(FamilyCompiler::new(
                                    required_cells,
                                    Arc::clone(&shared.targets),
                                    tablebase,
                                ))
                            };
                        }
                        TargetGroupPreparationMode::External => {
                            self.external_targets = Some(Arc::clone(&shared.targets));
                        }
                    }
                    // Do not enter the recursive family compiler in the same
                    // host call that commits a potentially large index.
                    return GeometryAdvance::Pending;
                }
            }
        }
        let internal_limit = match retained_limit_bytes {
            Some(total_limit) => {
                let live_targets = self
                    .compiler
                    .as_ref()
                    .map(|compiler| compiler.targets.as_ref())
                    .or_else(|| {
                        self.enumerator
                            .as_ref()
                            .map(|enumerator| enumerator.targets.as_ref())
                    })
                    .or(self.external_targets.as_deref());
                let fixed = live_targets
                    .map_or(Some(0_u128), checked_target_nested_retained_bytes)
                    .and_then(|bytes| bytes.checked_add(self.shared_family_bytes as u128))
                    .and_then(|bytes| {
                        bytes.checked_add(
                            self.external_targets
                                .as_ref()
                                .map_or(0_u128, |targets| target_bytes(targets) as u128),
                        )
                    });
                let Some(limit) = fixed.and_then(|fixed| total_limit.checked_sub(fixed)) else {
                    return GeometryAdvance::ResourceIncomplete(
                        "geometry_memory_capacity_exceeded",
                    );
                };
                Some(limit)
            }
            None => None,
        };
        if let Some(compiler) = self.compiler.as_mut() {
            if internal_limit.is_some_and(|limit| !compiler.set_retained_limit_bytes(limit)) {
                return GeometryAdvance::ResourceIncomplete(
                    "geometry_solution_family_memory_capacity_exceeded",
                );
            }
            let compile_advance = compiler.advance(catalog);
            if internal_limit.is_some_and(|limit| compiler.retained_bytes() as u128 > limit) {
                return GeometryAdvance::ResourceIncomplete(
                    "geometry_solution_family_memory_capacity_exceeded",
                );
            }
            match compile_advance {
                CompileAdvance::Pending => {
                    self.observe_compiler_metrics();
                    return GeometryAdvance::Pending;
                }
                CompileAdvance::ResourceIncomplete => {
                    self.observe_compiler_metrics();
                    return GeometryAdvance::ResourceIncomplete(
                        "geometry_solution_family_storage_unavailable",
                    );
                }
                CompileAdvance::Complete => {
                    self.observe_compiler_metrics();
                    let compiler = self.compiler.take().expect("geometry compiler exists");
                    if let Some(limit) = internal_limit {
                        self.candidate_family_count = None;
                        let Some(enumerator) = compiler.try_into_bounded_enumerator(limit) else {
                            return GeometryAdvance::ResourceIncomplete(
                                "geometry_solution_family_traversal_memory_capacity_exceeded",
                            );
                        };
                        self.enumerator = Some(enumerator);
                    } else {
                        self.candidate_family_count = compiler.candidate_family_count();
                        self.enumerator = Some(compiler.into_enumerator());
                    }
                    if retained_limit_bytes
                        .is_some_and(|limit| self.retained_bytes() as u128 > limit)
                    {
                        return GeometryAdvance::ResourceIncomplete(
                            "geometry_solution_family_traversal_memory_capacity_exceeded",
                        );
                    }
                    return GeometryAdvance::Pending;
                }
            }
        }

        let Some(enumerator) = self.enumerator.as_mut() else {
            return GeometryAdvance::Complete;
        };
        let advance = match enumerator.next_candidate(catalog) {
            Ok(Some(candidate)) => {
                if retained_limit_bytes.is_some_and(|limit| self.retained_bytes() as u128 > limit) {
                    GeometryAdvance::ResourceIncomplete(
                        "geometry_solution_family_traversal_memory_capacity_exceeded",
                    )
                } else {
                    GeometryAdvance::Candidate(candidate)
                }
            }
            Ok(None) => {
                self.enumerator = None;
                GeometryAdvance::Complete
            }
            Err(()) => GeometryAdvance::ResourceIncomplete(
                "geometry_solution_family_traversal_unavailable",
            ),
        };
        if retained_limit_bytes.is_some_and(|limit| self.retained_bytes() as u128 > limit) {
            GeometryAdvance::ResourceIncomplete("geometry_memory_capacity_exceeded")
        } else {
            advance
        }
    }

    #[cfg(feature = "parallel")]
    pub fn compile_for_parallel(
        &mut self,
        catalog: &GeometryCatalog,
        control: &clearra_core_domain::execution_cancellation::ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        while self.target_preparation.is_some() || self.compiler.is_some() {
            if control.is_cancelled() {
                return Err(WasmExactSearchError::Cancelled);
            }
            match self.advance(catalog) {
                GeometryAdvance::Pending => {}
                GeometryAdvance::ResourceIncomplete(reason) => {
                    return Err(WasmExactSearchError::InvalidProblem(reason));
                }
                GeometryAdvance::Candidate(_) | GeometryAdvance::Complete => {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_parallel_geometry_compile_state_invalid",
                    ));
                }
            }
        }
        Ok(())
    }

    #[cfg(feature = "parallel")]
    // Failure returns ownership of the prepared search so serial fallback loses no state.
    #[allow(clippy::result_large_err)]
    pub fn into_parallel_plan(
        mut self,
        requested_workers: usize,
    ) -> Result<ParallelGeometryPlan, Self> {
        if self.target_preparation.is_some() || self.compiler.is_some() {
            return Err(self);
        }
        let desired_partition_count = requested_workers.saturating_mul(8).max(2);
        let mut searches = Vec::new();
        if searches.try_reserve_exact(desired_partition_count).is_err() {
            return Err(self);
        }
        let Some(enumerator) = self.enumerator.take() else {
            return Err(self);
        };
        let (enumerators, shared_family_bytes) =
            match enumerator.into_parallel_enumerators(desired_partition_count) {
                Ok(partitions) if partitions.0.len() >= 2 => partitions,
                Ok((mut partitions, _)) => {
                    self.enumerator = partitions.pop();
                    return Err(self);
                }
                Err(enumerator) => {
                    self.enumerator = Some(enumerator);
                    return Err(self);
                }
            };
        let targets = Arc::clone(&enumerators[0].targets);
        for (index, enumerator) in enumerators.into_iter().enumerate() {
            searches.push(Self {
                group_pattern_index_bytes: 0,
                shared_family_bytes: 0,
                target_preparation: None,
                target_preparation_mode: None,
                compiler: None,
                candidate_family_count: enumerator.candidate_count,
                enumerator: Some(enumerator),
                expanded_nodes: usize::from(index == 0) * self.expanded_nodes,
                peak_frontier: usize::from(index == 0) * self.peak_frontier,
                domain_pruned_states: usize::from(index == 0) * self.domain_pruned_states,
                hall_pruned_states: usize::from(index == 0) * self.hall_pruned_states,
                column_pruned_states: usize::from(index == 0) * self.column_pruned_states,
                component_compositions: usize::from(index == 0) * self.component_compositions,
                tablebase_pruned_states: usize::from(index == 0) * self.tablebase_pruned_states,
                external_targets: None,
                resource_authoritative: false,
            });
        }
        Ok(ParallelGeometryPlan {
            targets,
            searches,
            group_pattern_index_bytes: self.group_pattern_index_bytes,
            shared_family_bytes,
        })
    }

    #[cfg(feature = "parallel")]
    pub fn parallel_priority(&self) -> usize {
        self.candidate_family_count
            .unwrap_or(u128::MAX)
            .min(usize::MAX as u128) as usize
    }

    #[cfg(feature = "parallel")]
    pub fn from_parallel_searches(
        searches: &[GeometrySearch],
        group_pattern_index_bytes: usize,
        shared_family_bytes: usize,
    ) -> Self {
        let mut geometry = Self::placeholder();
        geometry.group_pattern_index_bytes = group_pattern_index_bytes;
        geometry.shared_family_bytes = shared_family_bytes;
        geometry.candidate_family_count = Some(0);
        for search in searches {
            geometry.expanded_nodes = geometry
                .expanded_nodes
                .saturating_add(search.expanded_nodes);
            geometry.peak_frontier = geometry.peak_frontier.max(search.peak_frontier);
            geometry.domain_pruned_states = geometry
                .domain_pruned_states
                .saturating_add(search.domain_pruned_states);
            geometry.hall_pruned_states = geometry
                .hall_pruned_states
                .saturating_add(search.hall_pruned_states);
            geometry.column_pruned_states = geometry
                .column_pruned_states
                .saturating_add(search.column_pruned_states);
            geometry.component_compositions = geometry
                .component_compositions
                .saturating_add(search.component_compositions);
            geometry.tablebase_pruned_states = geometry
                .tablebase_pruned_states
                .saturating_add(search.tablebase_pruned_states);
            geometry.candidate_family_count = geometry
                .candidate_family_count
                .and_then(|total| search.candidate_family_count?.checked_add(total));
        }
        geometry
    }

    fn observe_compiler_metrics(&mut self) {
        if let Some(compiler) = self.compiler.as_ref() {
            self.expanded_nodes = compiler.expanded_nodes;
            self.peak_frontier = compiler.peak_frontier;
            self.domain_pruned_states = compiler.domain_pruned_states;
            self.hall_pruned_states = compiler.hall_pruned_states;
            self.column_pruned_states = compiler.column_pruned_states;
            self.component_compositions = compiler.component_compositions;
            self.tablebase_pruned_states = compiler.tablebase_pruned_states;
        }
    }

    pub const fn expanded_nodes(&self) -> usize {
        self.expanded_nodes
    }

    pub const fn peak_frontier(&self) -> usize {
        self.peak_frontier
    }

    pub const fn candidate_family_count(&self) -> Option<u128> {
        self.candidate_family_count
    }

    pub const fn target_preparation_pending(&self) -> bool {
        self.target_preparation.is_some()
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

    pub const fn component_compositions(&self) -> usize {
        self.component_compositions
    }

    pub const fn tablebase_pruned_states(&self) -> usize {
        self.tablebase_pruned_states
    }

    pub fn target(&self, target_index: u32) -> Option<&TargetGroup> {
        self.enumerator
            .as_ref()
            .map(|enumerator| enumerator.targets.as_ref())
            .or(self.external_targets.as_deref())?
            .get(target_index as usize)
    }

    pub fn targets(&self) -> Option<&[TargetGroup]> {
        self.enumerator
            .as_ref()
            .map(|enumerator| enumerator.targets.as_ref())
            .or(self.external_targets.as_deref())
    }

    pub fn finish_external(
        &mut self,
        candidate_count: usize,
        expanded_nodes: usize,
        peak_frontier: usize,
    ) {
        // A zero-candidate distributed worker does not need to materialize its
        // queue-language index. Finishing discards that unused preparation
        // cursor while preserving the same empty worker result.
        self.target_preparation = None;
        self.target_preparation_mode = None;
        self.candidate_family_count = Some(candidate_count as u128);
        self.expanded_nodes = expanded_nodes;
        self.peak_frontier = peak_frontier;
    }

    pub fn finish_external_summary(
        &mut self,
        summary: &super::distributed::WasmDistributedGeometrySummary,
    ) {
        self.candidate_family_count = summary.candidate_family_count;
        self.expanded_nodes = summary.expanded_nodes;
        self.peak_frontier = summary.peak_frontier;
        self.domain_pruned_states = summary.domain_pruned_states;
        self.hall_pruned_states = summary.hall_pruned_states;
        self.column_pruned_states = summary.column_pruned_states;
        self.component_compositions = summary.component_compositions;
    }

    pub fn retained_bytes(&self) -> usize {
        if let Some(preparation) = self.target_preparation.as_ref() {
            return preparation
                .retained_upper_bound_bytes()
                .saturating_add(self.shared_family_bytes);
        }
        let current_target_nested_bytes = self
            .compiler
            .as_ref()
            .map(|compiler| compiler.targets.as_ref())
            .or_else(|| {
                self.enumerator
                    .as_ref()
                    .map(|enumerator| enumerator.targets.as_ref())
            })
            .or(self.external_targets.as_deref())
            .and_then(checked_target_nested_retained_bytes)
            .and_then(|bytes| usize::try_from(bytes).ok())
            .unwrap_or_else(|| {
                usize::from(
                    self.compiler.is_some()
                        || self.enumerator.is_some()
                        || self.external_targets.is_some(),
                ) * usize::MAX
            });
        current_target_nested_bytes.saturating_add(self.shared_family_bytes)
            + self
                .compiler
                .as_ref()
                .map_or(0, FamilyCompiler::retained_bytes)
            + self
                .enumerator
                .as_ref()
                .map_or(0, FamilyEnumerator::retained_bytes)
            + self
                .external_targets
                .as_ref()
                .map_or(0, |targets| target_bytes(targets))
    }
}
// SRP rationale: this module has one behavior-level change reason: exact WASM geometry search session transitions.
