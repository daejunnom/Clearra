// SRP rationale: this module has one change reason: exact T-Spin coverage-only score materialization.
use std::{ops::Range, sync::Arc};

use clearra_core_domain::solution::normalized_tiling_solution::NormalizedTilingSolutionKey;
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_replay::{
    ExactScoringExecutionBatch, ExactScoringExecutionGraph, HoldDecision, ScoringExecutionEdge,
    ScoringExecutionNode, SpinCoverageExecutionBatch, SpinCoverageExecutionGraph,
};
use clearra_scoring::{
    event::SpinDetector,
    profile::{SpinProfile, SpinProfileId},
};

use super::{
    exact_scoring_execution_materializer::ExactScoringExecutionCancelled,
    execution_supply::{
        first_standard_bag_lookahead, for_each_supply_successor, terminal_supply_state_is_accepted,
        ExecutionSupplyBatch, SupplyState,
    },
};

#[derive(Clone, Debug)]
pub struct TSpinCoverageOnlyMaterialization {
    covered_patterns: PatternBitSet,
    candidate_ids: Vec<u64>,
    candidate_keys: Vec<String>,
    candidate_coverages: Vec<CandidatePatternCoverage>,
    witnessed_pattern_count: u128,
    complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TSpinCoverageMemoryProjection {
    pub pattern_count: usize,
    pub graph_count: usize,
    pub max_graph_node_count: usize,
    pub max_sequence_len: usize,
    pub max_product_states_per_node: usize,
    pub max_transition_key_count: usize,
    pub max_terminal_key_count: usize,
    pub word_storage_bytes: u128,
    pub candidate_storage_bytes: u128,
    pub transition_cache_bytes: u128,
    pub branch_workspace_bytes: u128,
    pub graph_workspace_bytes: u128,
    pub required_peak_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TSpinCoverageMemoryReport {
    pub projection: TSpinCoverageMemoryProjection,
    pub retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TSpinCoverageMaterializationError {
    Cancelled,
    ProjectionOverflow,
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
}

#[derive(Clone, Debug)]
pub struct CandidatePatternCoverage {
    candidate_id: u64,
    candidate_key: String,
    covered_patterns: PatternBitSet,
}

impl CandidatePatternCoverage {
    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub fn candidate_key(&self) -> &str {
        &self.candidate_key
    }

    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinCoverageTarget {
    spin_profile: SpinProfile,
    cleared_lines: Option<u8>,
    full_t_spin_only: bool,
}

impl SpinCoverageTarget {
    pub const T_SPIN_SINGLE: Self = Self {
        spin_profile: SpinProfile::builtin(SpinProfileId::TSpins),
        cleared_lines: Some(1),
        full_t_spin_only: true,
    };
    pub const SRS_PLUS_ALL_MINI_SINGLE: Self = Self::single(SpinProfileId::AllMiniPlus);

    #[allow(non_upper_case_globals)]
    pub const TSpinSingle: Self = Self::T_SPIN_SINGLE;

    #[allow(non_upper_case_globals)]
    pub const SrsPlusAllMiniSingle: Self = Self::SRS_PLUS_ALL_MINI_SINGLE;

    pub const fn single(spin_profile: SpinProfileId) -> Self {
        Self {
            spin_profile: SpinProfile::builtin(spin_profile),
            cleared_lines: Some(1),
            full_t_spin_only: false,
        }
    }

    pub const fn any_line_clear(spin_profile: SpinProfileId) -> Self {
        Self {
            spin_profile: SpinProfile::builtin(spin_profile),
            cleared_lines: None,
            full_t_spin_only: false,
        }
    }

    pub const fn spin_profile(self) -> SpinProfile {
        self.spin_profile
    }

    fn matches(self, edge: clearra_replay::ScoringExecutionEdge) -> bool {
        edge.cleared_lines() > 0
            && self
                .cleared_lines
                .is_none_or(|cleared_lines| edge.cleared_lines() == cleared_lines)
            && if self.full_t_spin_only {
                SpinDetector::is_exact_t_spin_single_edge(edge)
            } else {
                SpinDetector::detect_scoring_edge_with_profile(edge, self.spin_profile).is_some()
            }
    }
}

impl TSpinCoverageOnlyMaterialization {
    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }

    pub fn candidate_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.candidate_ids.iter().copied()
    }

    pub fn candidate_keys(&self) -> impl Iterator<Item = &str> + '_ {
        self.candidate_keys.iter().map(String::as_str)
    }

    pub fn candidate_coverages(&self) -> &[CandidatePatternCoverage] {
        &self.candidate_coverages
    }

    pub const fn witnessed_pattern_count(&self) -> u128 {
        self.witnessed_pattern_count
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn into_summary_parts(self) -> (PatternBitSet, Vec<String>, u128, bool) {
        (
            self.covered_patterns,
            self.candidate_keys,
            self.witnessed_pattern_count,
            self.complete,
        )
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        let mut bytes = core::mem::size_of::<Self>() as u128;
        bytes = bytes.checked_add(self.covered_patterns.retained_bytes() as u128)?;
        bytes = bytes.checked_add(
            (self.candidate_ids.capacity() as u128)
                .checked_mul(core::mem::size_of::<u64>() as u128)?,
        )?;
        bytes = bytes.checked_add(
            (self.candidate_keys.capacity() as u128)
                .checked_mul(core::mem::size_of::<String>() as u128)?,
        )?;
        for key in &self.candidate_keys {
            bytes = bytes.checked_add(key.capacity() as u128)?;
        }
        bytes = bytes.checked_add(
            (self.candidate_coverages.capacity() as u128)
                .checked_mul(core::mem::size_of::<CandidatePatternCoverage>() as u128)?,
        )?;
        for coverage in &self.candidate_coverages {
            bytes = bytes
                .checked_add(coverage.candidate_key.capacity() as u128)?
                .checked_add(coverage.covered_patterns.retained_bytes() as u128)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Copy)]
enum SpinBatchRef<'a> {
    Scoring(&'a ExactScoringExecutionBatch),
    Coverage(&'a SpinCoverageExecutionBatch),
}

impl<'a> SpinBatchRef<'a> {
    fn patterns(self) -> &'a [Vec<PieceKind>] {
        match self {
            Self::Scoring(batch) => batch.patterns(),
            Self::Coverage(batch) => batch.patterns(),
        }
    }

    const fn initial_cursor(self) -> u16 {
        match self {
            Self::Scoring(batch) => batch.initial_cursor(),
            Self::Coverage(batch) => batch.initial_cursor(),
        }
    }

    const fn initial_hold(self) -> Option<PieceKind> {
        match self {
            Self::Scoring(batch) => batch.initial_hold(),
            Self::Coverage(batch) => batch.initial_hold(),
        }
    }

    const fn complete(self) -> bool {
        match self {
            Self::Scoring(batch) => batch.complete(),
            Self::Coverage(batch) => batch.complete(),
        }
    }

    fn graph_count(self) -> usize {
        match self {
            Self::Scoring(batch) => batch.graphs().len(),
            Self::Coverage(batch) => batch.graphs().len(),
        }
    }

    fn graph(self, index: usize) -> Option<SpinGraphRef<'a>> {
        match self {
            Self::Scoring(batch) => batch.graphs().get(index).map(SpinGraphRef::Scoring),
            Self::Coverage(batch) => batch.graphs().get(index).map(SpinGraphRef::Coverage),
        }
    }

    fn checked_memory_projection(self) -> Option<TSpinCoverageMemoryProjection> {
        let pattern_count = self.patterns().len();
        let graph_count = self.graph_count();
        let word_count = pattern_count.div_ceil(u64::BITS as usize) as u128;
        let word_bytes = word_count.checked_mul(core::mem::size_of::<u64>() as u128)?;
        let max_sequence_len = self.patterns().iter().map(Vec::len).max().unwrap_or(0);
        let max_graph_node_count = (0..graph_count)
            .filter_map(|index| self.graph(index))
            .map(SpinGraphRef::node_count)
            .max()
            .unwrap_or(0);
        let candidate_key_bytes = (0..graph_count).try_fold(0_u128, |bytes, index| {
            bytes.checked_add(self.graph(index)?.checked_candidate_key_capacity()?)
        })?;

        // Initial, accumulated, per-graph, intersection, and final-conversion
        // word buffers can coexist with the returned shared bitset.
        let word_storage_bytes = word_bytes.checked_mul(6)?;
        let graph_count_u128 = graph_count as u128;
        let all_candidate_pattern_ids = graph_count_u128.checked_mul(pattern_count as u128)?;
        let candidate_bitset_owners =
            graph_count_u128.checked_mul(core::mem::size_of::<PatternBitSet>() as u128)?;
        let candidate_bitsets = PatternBitSet::checked_shared_construction_upper_bound(
            pattern_count,
            graph_count_u128,
            all_candidate_pattern_ids,
        )?
        .checked_sub(candidate_bitset_owners)?;
        let candidate_storage_bytes = graph_count_u128
            .checked_mul(core::mem::size_of::<(String, (u64, PatternBitSet))>() as u128)?
            .checked_add(candidate_key_bytes)?
            .checked_add(candidate_bitsets)?
            .checked_add(graph_count_u128.checked_mul(core::mem::size_of::<u64>() as u128)?)?
            .checked_add(graph_count_u128.checked_mul(core::mem::size_of::<String>() as u128)?)?
            .checked_add(candidate_key_bytes)?
            .checked_add(
                graph_count_u128
                    .checked_mul(core::mem::size_of::<CandidatePatternCoverage>() as u128)?,
            )?;

        let cursor_variants = max_sequence_len.checked_add(2)?;
        let max_product_states_per_node = cursor_variants.checked_mul(8)?.checked_mul(2)?;
        let max_transition_key_count = cursor_variants
            .checked_mul(8)?
            .checked_mul(PieceKind::STANDARD_TETROMINOES.len())?
            .checked_mul(2)?;
        let max_terminal_key_count = cursor_variants.checked_mul(8)?;
        let transition_key_count = max_transition_key_count as u128;
        let terminal_key_count = max_terminal_key_count as u128;
        // Current contributes one key; swap and store can each distinguish all
        // seven current pieces across a pattern family; terminal hold release
        // contributes one more key.
        let transition_cache_bytes = transition_key_count
            .checked_mul(core::mem::size_of::<(TransitionCacheKey, Vec<TransitionMask>)>() as u128)?
            .checked_add(
                transition_key_count
                    .checked_mul(MAX_MASKS_PER_TRANSITION_KEY as u128)?
                    .checked_mul(core::mem::size_of::<TransitionMask>() as u128)?,
            )?
            .checked_add(
                transition_key_count
                    .checked_mul(MAX_MASKS_PER_TRANSITION_KEY as u128)?
                    .checked_mul(word_bytes)?,
            )?
            .checked_add(
                terminal_key_count
                    .checked_mul(
                        core::mem::size_of::<((u16, Option<PieceKind>), Arc<[u64]>)>() as u128,
                    )?,
            )?
            .checked_add(terminal_key_count.checked_mul(word_bytes)?)?;

        type Branch = ((HoldDecision, u16, Option<PieceKind>), Vec<u64>);
        let branch_workspace_bytes = (MAX_MASKS_PER_TRANSITION_KEY as u128)
            .checked_mul(core::mem::size_of::<Branch>() as u128)?
            // One transition build owns all mutable branch word buffers while
            // the destination TransitionMask vector and converted Arc payloads
            // are created.
            .checked_add((MAX_MASKS_PER_TRANSITION_KEY as u128).checked_mul(word_bytes)?)?
            .checked_add(
                (MAX_MASKS_PER_TRANSITION_KEY as u128)
                    .checked_mul(core::mem::size_of::<TransitionMask>() as u128)?,
            )?;

        let graph_node_count = max_graph_node_count as u128;
        let state_entry_count =
            graph_node_count.checked_mul(max_product_states_per_node as u128)?;
        let graph_workspace_bytes = graph_node_count
            .checked_mul(core::mem::size_of::<Vec<(ProductState, PatternSet)>>() as u128)?
            .checked_add(
                state_entry_count
                    .checked_mul(core::mem::size_of::<(ProductState, PatternSet)>() as u128)?,
            )?
            .checked_add(state_entry_count.checked_mul(word_bytes)?)?
            // One cloned transition list is live in the graph walker while the
            // authoritative cached list remains owned by the cache.
            .checked_add(
                (MAX_MASKS_PER_TRANSITION_KEY as u128)
                    .checked_mul(core::mem::size_of::<TransitionMask>() as u128)?,
            )?
            .checked_add(word_bytes)?;
        let required_peak_bytes = word_storage_bytes
            .checked_add(candidate_storage_bytes)?
            .checked_add(transition_cache_bytes)?
            .checked_add(branch_workspace_bytes)?
            .checked_add(graph_workspace_bytes)?;
        Some(TSpinCoverageMemoryProjection {
            pattern_count,
            graph_count,
            max_graph_node_count,
            max_sequence_len,
            max_product_states_per_node,
            max_transition_key_count,
            max_terminal_key_count,
            word_storage_bytes,
            candidate_storage_bytes,
            transition_cache_bytes,
            branch_workspace_bytes,
            graph_workspace_bytes,
            required_peak_bytes,
        })
    }
}

impl ExecutionSupplyBatch for SpinBatchRef<'_> {
    fn hold_enabled(&self) -> bool {
        match self {
            Self::Scoring(batch) => batch.hold_enabled(),
            Self::Coverage(batch) => batch.hold_enabled(),
        }
    }

    fn projects_unplaced_lookahead(&self) -> bool {
        match self {
            Self::Scoring(batch) => batch.projects_unplaced_lookahead(),
            Self::Coverage(batch) => batch.projects_unplaced_lookahead(),
        }
    }

    fn projects_standard_bag_lookahead(&self) -> bool {
        match self {
            Self::Scoring(batch) => batch.projects_standard_bag_lookahead(),
            Self::Coverage(batch) => batch.projects_standard_bag_lookahead(),
        }
    }
}

#[derive(Clone, Copy)]
enum SpinGraphRef<'a> {
    Scoring(&'a ExactScoringExecutionGraph),
    Coverage(&'a SpinCoverageExecutionGraph),
}

impl<'a> SpinGraphRef<'a> {
    const fn candidate_id(self) -> u64 {
        match self {
            Self::Scoring(graph) => graph.candidate_id(),
            Self::Coverage(graph) => graph.candidate_id(),
        }
    }

    fn candidate_key(self) -> String {
        match self {
            Self::Scoring(graph) => {
                NormalizedTilingSolutionKey::from_standard_board64_identity(graph.identity())
                    .as_str()
                    .to_owned()
            }
            Self::Coverage(graph) => graph.candidate_key().to_owned(),
        }
    }

    const fn root(self) -> u32 {
        match self {
            Self::Scoring(graph) => graph.root(),
            Self::Coverage(graph) => graph.root(),
        }
    }

    fn node(self, index: u32) -> Option<ScoringExecutionNode> {
        match self {
            Self::Scoring(graph) => graph.node(index),
            Self::Coverage(graph) => graph.node(index),
        }
    }

    fn node_count(self) -> usize {
        match self {
            Self::Scoring(graph) => graph.node_count(),
            Self::Coverage(graph) => graph.node_count(),
        }
    }

    fn checked_candidate_key_capacity(self) -> Option<u128> {
        match self {
            Self::Scoring(graph) => (graph.identity().placement_count() as u128)
                .checked_mul(20)?
                .checked_add(42),
            Self::Coverage(graph) => Some(graph.candidate_key().len() as u128),
        }
    }

    fn edges(self, node: ScoringExecutionNode) -> &'a [ScoringExecutionEdge] {
        match self {
            Self::Scoring(graph) => graph.edges(node),
            Self::Coverage(graph) => graph.edges(node),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TransitionCacheKey {
    cursor: u16,
    hold: Option<PieceKind>,
    required_piece: PieceKind,
    release_held_at_terminal: bool,
}

#[derive(Clone, Debug)]
struct TransitionMask {
    next_cursor: u16,
    next_hold: Option<PieceKind>,
    pattern_words: Arc<[u64]>,
}

const MAX_MASKS_PER_TRANSITION_KEY: usize = 16;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ProductState {
    cursor: u16,
    hold: Option<PieceKind>,
    has_target_spin: bool,
}

#[derive(Clone, Debug, Default)]
struct PatternSet {
    words: Vec<u64>,
}

impl PatternSet {
    fn single(words: Vec<u64>) -> Self {
        Self { words }
    }

    fn add_filtered(&mut self, source: &Self, mask: &[u64], already_covered: &[u64]) {
        if self.words.is_empty() {
            self.words = zeroed_words(source.words.len());
        }
        for (((target, source), mask), covered) in self
            .words
            .iter_mut()
            .zip(&source.words)
            .zip(mask)
            .zip(already_covered)
        {
            *target |= source & mask & !covered;
        }
    }
}

type TransitionCacheEntry = (TransitionCacheKey, Vec<TransitionMask>);
type TerminalMaskEntry = ((u16, Option<PieceKind>), Arc<[u64]>);
type SupplyBranchKey = (HoldDecision, u16, Option<PieceKind>);
type SupplyBranch = (SupplyBranchKey, Vec<u64>);

struct SupplyTransitionCache<'a> {
    batch: SpinBatchRef<'a>,
    word_count: usize,
    transitions: Vec<TransitionCacheEntry>,
    terminal_masks: Vec<TerminalMaskEntry>,
}

impl<'a> SupplyTransitionCache<'a> {
    fn new(batch: SpinBatchRef<'a>, projection: &TSpinCoverageMemoryProjection) -> Self {
        Self {
            batch,
            word_count: batch.patterns().len().div_ceil(u64::BITS as usize),
            transitions: Vec::with_capacity(projection.max_transition_key_count),
            terminal_masks: Vec::with_capacity(projection.max_terminal_key_count),
        }
    }

    fn transitions(
        &mut self,
        state: ProductState,
        required_piece: PieceKind,
        release_held_at_terminal: bool,
    ) -> Vec<TransitionMask> {
        let key = TransitionCacheKey {
            cursor: state.cursor,
            hold: state.hold,
            required_piece,
            release_held_at_terminal,
        };
        if let Some((_, masks)) = self.transitions.iter().find(|(cached, _)| *cached == key) {
            return clone_transition_masks(masks);
        }
        let masks = self.build_transition_masks(key);
        let output = clone_transition_masks(&masks);
        debug_assert!(self.transitions.len() < self.transitions.capacity());
        self.transitions.push((key, masks));
        output
    }

    fn terminal_mask(&mut self, state: ProductState) -> Arc<[u64]> {
        let key = (state.cursor, state.hold);
        if let Some((_, mask)) = self
            .terminal_masks
            .iter()
            .find(|(cached, _)| *cached == key)
        {
            return Arc::clone(mask);
        }
        {
            let mut words = zeroed_words(self.word_count);
            for (pattern_id, sequence) in self.batch.patterns().iter().enumerate() {
                if terminal_supply_state_is_accepted(
                    &self.batch,
                    sequence,
                    SupplyState {
                        node: 0,
                        cursor: state.cursor,
                        hold: state.hold,
                    },
                ) {
                    set_pattern(&mut words, pattern_id);
                }
            }
            let mask = Arc::<[u64]>::from(words);
            debug_assert!(self.terminal_masks.len() < self.terminal_masks.capacity());
            self.terminal_masks.push((key, Arc::clone(&mask)));
            mask
        }
    }

    fn build_transition_masks(&self, key: TransitionCacheKey) -> Vec<TransitionMask> {
        let mut branches = Vec::<((HoldDecision, u16, Option<PieceKind>), Vec<u64>)>::with_capacity(
            MAX_MASKS_PER_TRANSITION_KEY,
        );
        for (pattern_id, sequence) in self.batch.patterns().iter().enumerate() {
            let state = SupplyState {
                node: 0,
                cursor: key.cursor,
                hold: key.hold,
            };
            if key.release_held_at_terminal
                && state.cursor as usize == sequence.len()
                && state.hold == Some(key.required_piece)
                && (!self.batch.projects_standard_bag_lookahead()
                    || first_standard_bag_lookahead(sequence).is_none())
            {
                let Some(next_cursor) = state.cursor.checked_add(1) else {
                    continue;
                };
                let branch_key = (
                    HoldDecision::ReleaseHeldAtTerminal {
                        held_piece: key.required_piece,
                    },
                    next_cursor,
                    state.hold,
                );
                let words = branch_words(&mut branches, branch_key, self.word_count);
                set_pattern(words, pattern_id);
            }
            for_each_supply_successor(
                &self.batch,
                sequence,
                state,
                key.required_piece,
                |decision, next| {
                    let words = branch_words(
                        &mut branches,
                        (decision, next.cursor, next.hold),
                        self.word_count,
                    );
                    set_pattern(words, pattern_id);
                    Ok(())
                },
            )
            .expect("supply mask construction cannot be cancelled");
        }
        branches.sort_unstable_by_key(|(key, _)| *key);
        let mut masks = Vec::with_capacity(branches.len());
        for ((_decision, next_cursor, next_hold), pattern_words) in branches {
            masks.push(TransitionMask {
                next_cursor,
                next_hold,
                pattern_words: pattern_words.into(),
            });
        }
        masks
    }
}

fn clone_transition_masks(source: &[TransitionMask]) -> Vec<TransitionMask> {
    let mut cloned = Vec::with_capacity(source.len());
    cloned.extend(source.iter().cloned());
    cloned
}

fn branch_words(
    branches: &mut Vec<SupplyBranch>,
    key: SupplyBranchKey,
    word_count: usize,
) -> &mut Vec<u64> {
    if let Some(index) = branches.iter().position(|(branch, _)| *branch == key) {
        return &mut branches[index].1;
    }
    debug_assert!(branches.len() < MAX_MASKS_PER_TRANSITION_KEY);
    branches.push((key, zeroed_words(word_count)));
    &mut branches.last_mut().expect("branch was appended").1
}

pub struct TSpinCoverageOnlyMaterializer;

impl TSpinCoverageOnlyMaterializer {
    pub fn checked_target_memory_projection(
        batch: &ExactScoringExecutionBatch,
    ) -> Option<TSpinCoverageMemoryProjection> {
        SpinBatchRef::Scoring(batch).checked_memory_projection()
    }

    pub fn checked_spin_batch_memory_projection(
        batch: &SpinCoverageExecutionBatch,
    ) -> Option<TSpinCoverageMemoryProjection> {
        SpinBatchRef::Coverage(batch).checked_memory_projection()
    }

    pub fn materialize(
        batch: &ExactScoringExecutionBatch,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_target(
            batch,
            SpinCoverageTarget::TSpinSingle,
            0..batch.patterns().len(),
            control,
        )
    }

    pub fn materialize_pattern_range(
        batch: &ExactScoringExecutionBatch,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_target(
            batch,
            SpinCoverageTarget::TSpinSingle,
            pattern_range,
            control,
        )
    }

    pub fn materialize_target(
        batch: &ExactScoringExecutionBatch,
        target: SpinCoverageTarget,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_batch(
            SpinBatchRef::Scoring(batch),
            Some(target),
            pattern_range,
            control,
        )
    }

    pub fn materialize_target_with_memory_limit(
        batch: &ExactScoringExecutionBatch,
        target: SpinCoverageTarget,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<
        (TSpinCoverageOnlyMaterialization, TSpinCoverageMemoryReport),
        TSpinCoverageMaterializationError,
    > {
        Self::materialize_batch_with_memory_limit(
            SpinBatchRef::Scoring(batch),
            Some(target),
            pattern_range,
            control,
            already_retained_bytes,
            max_memory_bytes,
        )
    }

    pub fn materialize_spin_batch(
        batch: &SpinCoverageExecutionBatch,
        target: SpinCoverageTarget,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_batch(
            SpinBatchRef::Coverage(batch),
            Some(target),
            pattern_range,
            control,
        )
    }

    pub fn materialize_spin_batch_with_memory_limit(
        batch: &SpinCoverageExecutionBatch,
        target: SpinCoverageTarget,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<
        (TSpinCoverageOnlyMaterialization, TSpinCoverageMemoryReport),
        TSpinCoverageMaterializationError,
    > {
        Self::materialize_batch_with_memory_limit(
            SpinBatchRef::Coverage(batch),
            Some(target),
            pattern_range,
            control,
            already_retained_bytes,
            max_memory_bytes,
        )
    }

    pub fn materialize_all_paths(
        batch: &ExactScoringExecutionBatch,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_batch(SpinBatchRef::Scoring(batch), None, pattern_range, control)
    }

    pub fn materialize_all_paths_with_memory_limit(
        batch: &ExactScoringExecutionBatch,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<
        (TSpinCoverageOnlyMaterialization, TSpinCoverageMemoryReport),
        TSpinCoverageMaterializationError,
    > {
        Self::materialize_batch_with_memory_limit(
            SpinBatchRef::Scoring(batch),
            None,
            pattern_range,
            control,
            already_retained_bytes,
            max_memory_bytes,
        )
    }

    pub fn materialize_all_spin_paths(
        batch: &SpinCoverageExecutionBatch,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_batch(SpinBatchRef::Coverage(batch), None, pattern_range, control)
    }

    pub fn materialize_all_spin_paths_with_memory_limit(
        batch: &SpinCoverageExecutionBatch,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<
        (TSpinCoverageOnlyMaterialization, TSpinCoverageMemoryReport),
        TSpinCoverageMaterializationError,
    > {
        Self::materialize_batch_with_memory_limit(
            SpinBatchRef::Coverage(batch),
            None,
            pattern_range,
            control,
            already_retained_bytes,
            max_memory_bytes,
        )
    }

    fn materialize_batch(
        batch: SpinBatchRef<'_>,
        target: Option<SpinCoverageTarget>,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        let projection = batch
            .checked_memory_projection()
            .expect("materializable batch has a checked address-space projection");
        Self::materialize_batch_with_projection(batch, target, pattern_range, control, &projection)
    }

    fn materialize_batch_with_projection(
        batch: SpinBatchRef<'_>,
        target: Option<SpinCoverageTarget>,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
        projection: &TSpinCoverageMemoryProjection,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        let pattern_count = batch.patterns().len();
        let start = pattern_range.start.min(pattern_count);
        let end = pattern_range.end.min(pattern_count).max(start);
        let word_count = pattern_count.div_ceil(u64::BITS as usize);
        let mut initial_words = zeroed_words(word_count);
        for pattern_id in start..end {
            set_pattern(&mut initial_words, pattern_id);
        }
        let mut covered_words = zeroed_words(word_count);
        let mut coverage_by_candidate =
            Vec::<(String, (u64, PatternBitSet))>::with_capacity(batch.graph_count());
        let mut cache = SupplyTransitionCache::new(batch, projection);
        let mut complete = batch.complete();

        for graph_index in 0..batch.graph_count() {
            let graph = batch
                .graph(graph_index)
                .expect("spin execution graph index is in range");
            if control.is_cancelled() {
                return Err(ExactScoringExecutionCancelled);
            }
            control.report_progress(
                "t-spin-coverage",
                graph_index as u64,
                Some(batch.graph_count() as u64),
            );
            let mut graph_words = zeroed_words(word_count);
            let graph_complete = evaluate_graph_product(
                batch,
                graph,
                &initial_words,
                &mut cache,
                &mut graph_words,
                target,
                control,
                projection.max_product_states_per_node,
            )?;
            complete &= graph_complete;
            if words_are_nonempty(&graph_words) {
                let candidate_key = graph.candidate_key();
                let graph_coverage = PatternBitSet::from_words(pattern_count, graph_words.clone())
                    .expect("candidate coverage preserves the pattern universe");
                if let Some((_, (candidate_id, coverage))) = coverage_by_candidate
                    .iter_mut()
                    .find(|(key, _)| *key == candidate_key)
                {
                    *candidate_id = (*candidate_id).min(graph.candidate_id());
                    coverage
                        .union_with(&graph_coverage)
                        .expect("candidate coverage preserves the pattern universe");
                } else {
                    coverage_by_candidate
                        .push((candidate_key, (graph.candidate_id(), graph_coverage)));
                }
                union_words(&mut covered_words, &graph_words);
            }
        }
        control.report_progress(
            "t-spin-coverage",
            batch.graph_count() as u64,
            Some(batch.graph_count() as u64),
        );
        coverage_by_candidate.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let mut candidate_ids = Vec::with_capacity(coverage_by_candidate.len());
        candidate_ids.extend(
            coverage_by_candidate
                .iter()
                .map(|(_, (candidate_id, _))| *candidate_id),
        );
        candidate_ids.sort_unstable();
        candidate_ids.dedup();
        let mut candidate_keys = Vec::with_capacity(coverage_by_candidate.len());
        candidate_keys.extend(coverage_by_candidate.iter().map(|(key, _)| key.clone()));
        let witnessed_pattern_count = coverage_by_candidate
            .iter()
            .map(|(_, (_, coverage))| u128::from(coverage.count_ones()))
            .sum();
        let mut candidate_coverages = Vec::with_capacity(coverage_by_candidate.len());
        candidate_coverages.extend(coverage_by_candidate.into_iter().map(
            |(candidate_key, (candidate_id, covered_patterns))| CandidatePatternCoverage {
                candidate_id,
                candidate_key,
                covered_patterns,
            },
        ));
        Ok(TSpinCoverageOnlyMaterialization {
            covered_patterns: PatternBitSet::from_words(pattern_count, covered_words)
                .expect("coverage product preserves the pattern universe"),
            candidate_ids,
            candidate_keys,
            candidate_coverages,
            witnessed_pattern_count,
            complete,
        })
    }

    fn materialize_batch_with_memory_limit(
        batch: SpinBatchRef<'_>,
        target: Option<SpinCoverageTarget>,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<
        (TSpinCoverageOnlyMaterialization, TSpinCoverageMemoryReport),
        TSpinCoverageMaterializationError,
    > {
        let projection = batch
            .checked_memory_projection()
            .ok_or(TSpinCoverageMaterializationError::ProjectionOverflow)?;
        let required_memory_bytes = already_retained_bytes
            .checked_add(projection.required_peak_bytes)
            .ok_or(TSpinCoverageMaterializationError::ProjectionOverflow)?;
        if required_memory_bytes > max_memory_bytes {
            return Err(TSpinCoverageMaterializationError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            });
        }
        let materialized = Self::materialize_batch_with_projection(
            batch,
            target,
            pattern_range,
            control,
            &projection,
        )
        .map_err(|_| TSpinCoverageMaterializationError::Cancelled)?;
        let retained_bytes = materialized
            .checked_retained_bytes()
            .ok_or(TSpinCoverageMaterializationError::ProjectionOverflow)?;
        let required_memory_bytes = already_retained_bytes
            .checked_add(retained_bytes)
            .ok_or(TSpinCoverageMaterializationError::ProjectionOverflow)?;
        if required_memory_bytes > max_memory_bytes {
            return Err(TSpinCoverageMaterializationError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            });
        }
        Ok((
            materialized,
            TSpinCoverageMemoryReport {
                projection,
                retained_bytes,
            },
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn evaluate_graph_product(
    batch: SpinBatchRef<'_>,
    graph: SpinGraphRef<'_>,
    initial_words: &[u64],
    cache: &mut SupplyTransitionCache<'_>,
    graph_words: &mut [u64],
    target: Option<SpinCoverageTarget>,
    control: &ExecutionControl,
    max_product_states_per_node: usize,
) -> Result<bool, ExactScoringExecutionCancelled> {
    let mut states = Vec::with_capacity(graph.node_count());
    states
        .extend((0..graph.node_count()).map(|_| {
            Vec::<(ProductState, PatternSet)>::with_capacity(max_product_states_per_node)
        }));
    let Some(root_states) = states.get_mut(graph.root() as usize) else {
        return Ok(false);
    };
    root_states.push((
        ProductState {
            cursor: batch.initial_cursor(),
            hold: batch.initial_hold(),
            has_target_spin: false,
        },
        PatternSet::single(clone_words(initial_words)),
    ));

    let mut complete = true;
    for node_index in 0..states.len() {
        if control.is_cancelled() {
            return Err(ExactScoringExecutionCancelled);
        }
        let Some(node) = graph.node(node_index as u32) else {
            complete = false;
            continue;
        };
        let current_states = core::mem::take(&mut states[node_index]);
        if node.accepting() {
            for (state, patterns) in current_states {
                if target.is_some() && !state.has_target_spin {
                    continue;
                }
                let terminal_mask = cache.terminal_mask(state);
                if let Some(accepted) = intersect_words(&patterns.words, &terminal_mask) {
                    union_words(graph_words, &accepted);
                }
            }
            continue;
        }

        for edge in graph.edges(node).iter().copied() {
            let child_index = edge.to() as usize;
            let Some(child) = graph.node(edge.to()) else {
                complete = false;
                continue;
            };
            if child_index <= node_index || child_index >= states.len() {
                complete = false;
                continue;
            }
            let release_held_at_terminal =
                batch.projects_unplaced_lookahead() && batch.hold_enabled() && child.accepting();
            let edge_has_target_spin = target.is_some_and(|target| target.matches(edge));
            for (state, patterns) in &current_states {
                let transitions = cache.transitions(*state, edge.piece(), release_held_at_terminal);
                for transition in transitions {
                    let next = ProductState {
                        cursor: transition.next_cursor,
                        hold: transition.next_hold,
                        has_target_spin: state.has_target_spin || edge_has_target_spin,
                    };
                    add_product_state_patterns(
                        &mut states[child_index],
                        next,
                        patterns,
                        &transition.pattern_words,
                        graph_words,
                    );
                }
            }
        }
    }
    Ok(complete)
}

fn add_product_state_patterns(
    states: &mut Vec<(ProductState, PatternSet)>,
    key: ProductState,
    source: &PatternSet,
    mask: &[u64],
    already_covered: &[u64],
) {
    match states.binary_search_by_key(&key, |(state, _)| *state) {
        Ok(index) => states[index].1.add_filtered(source, mask, already_covered),
        Err(index) => {
            let mut patterns = PatternSet::default();
            patterns.add_filtered(source, mask, already_covered);
            debug_assert!(states.len() < states.capacity());
            states.insert(index, (key, patterns));
        }
    }
}

fn set_pattern(words: &mut [u64], pattern_id: usize) {
    words[pattern_id / u64::BITS as usize] |= 1_u64 << (pattern_id % u64::BITS as usize);
}

fn intersect_words(left: &[u64], right: &[u64]) -> Option<Vec<u64>> {
    let mut nonempty = false;
    let mut words = Vec::with_capacity(left.len().min(right.len()));
    for (left, right) in left.iter().zip(right) {
        let word = left & right;
        nonempty |= word != 0;
        words.push(word);
    }
    nonempty.then_some(words)
}

fn zeroed_words(word_count: usize) -> Vec<u64> {
    vec![0; word_count]
}

fn clone_words(source: &[u64]) -> Vec<u64> {
    let mut words = Vec::with_capacity(source.len());
    words.extend_from_slice(source);
    words
}

fn union_words(target: &mut [u64], source: &[u64]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target |= source;
    }
}

fn words_are_nonempty(words: &[u64]) -> bool {
    words.iter().any(|word| *word != 0)
}

#[cfg(test)]
mod memory_projection_tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
    };
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_replay::{
        ExactScoringExecutionBatch, ExactScoringExecutionGraph, ScoringExecutionNode,
    };

    use super::*;

    fn batch() -> ExactScoringExecutionBatch {
        let identity = StandardBoard64TilingIdentity::from_placements(
            0,
            [PiecePlacementMask::new(PieceKind::I, 0xf)],
        )
        .expect("identity");
        ExactScoringExecutionBatch::new(
            Board64Layout::standard_10_by_lines(4).expect("layout"),
            0,
            vec![vec![PieceKind::I]],
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            vec![ExactScoringExecutionGraph::new(
                1,
                identity,
                0,
                vec![ScoringExecutionNode::new(0, 0, true)],
                Vec::new(),
            )],
            true,
        )
    }

    #[test]
    fn owner_projection_is_fieldwise_checked_and_one_byte_short_fails_before_work() {
        let batch = batch();
        let projection = TSpinCoverageOnlyMaterializer::checked_target_memory_projection(&batch)
            .expect("checked owner projection");
        assert_eq!(projection.pattern_count, 1);
        assert_eq!(projection.graph_count, 1);
        assert_eq!(projection.max_graph_node_count, 1);
        assert_eq!(projection.max_sequence_len, 1);
        assert_eq!(projection.max_product_states_per_node, 48);
        assert_eq!(projection.max_transition_key_count, 336);
        assert_eq!(projection.max_terminal_key_count, 24);
        assert!(projection.branch_workspace_bytes > 0);
        assert_eq!(
            projection.required_peak_bytes,
            projection.word_storage_bytes
                + projection.candidate_storage_bytes
                + projection.transition_cache_bytes
                + projection.branch_workspace_bytes
                + projection.graph_workspace_bytes
        );
        let error = TSpinCoverageOnlyMaterializer::materialize_target_with_memory_limit(
            &batch,
            SpinCoverageTarget::T_SPIN_SINGLE,
            0..1,
            &ExecutionControl::default(),
            7,
            projection.required_peak_bytes + 6,
        )
        .expect_err("one-byte-short materializer cap must fail closed");
        assert!(matches!(
            error,
            TSpinCoverageMaterializationError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            } if required_memory_bytes == max_memory_bytes + 1
        ));
    }

    #[test]
    fn bounded_and_compatibility_materialization_are_fieldwise_identical() {
        let batch = batch();
        let projection = TSpinCoverageOnlyMaterializer::checked_target_memory_projection(&batch)
            .expect("checked owner projection");
        let compatible = TSpinCoverageOnlyMaterializer::materialize_target(
            &batch,
            SpinCoverageTarget::T_SPIN_SINGLE,
            0..1,
            &ExecutionControl::default(),
        )
        .expect("compatibility materialization");
        let (bounded, report) =
            TSpinCoverageOnlyMaterializer::materialize_target_with_memory_limit(
                &batch,
                SpinCoverageTarget::T_SPIN_SINGLE,
                0..1,
                &ExecutionControl::default(),
                0,
                projection.required_peak_bytes,
            )
            .expect("bounded materialization");
        assert_eq!(compatible.covered_patterns(), bounded.covered_patterns());
        assert_eq!(
            compatible.candidate_ids().collect::<Vec<_>>(),
            bounded.candidate_ids().collect::<Vec<_>>()
        );
        assert_eq!(
            compatible.candidate_keys().collect::<Vec<_>>(),
            bounded.candidate_keys().collect::<Vec<_>>()
        );
        assert_eq!(
            compatible.witnessed_pattern_count(),
            bounded.witnessed_pattern_count()
        );
        assert_eq!(compatible.complete(), bounded.complete());
        assert_eq!(report.projection, projection);
        assert_eq!(
            report.retained_bytes,
            bounded.checked_retained_bytes().expect("retained bytes")
        );
        assert!(report.retained_bytes <= projection.required_peak_bytes);
    }

    #[test]
    fn all_paths_guarded_entrypoint_accepts_exact_cap_and_rejects_peak_minus_one() {
        let batch = batch();
        let projection = TSpinCoverageOnlyMaterializer::checked_target_memory_projection(&batch)
            .expect("all-path projection");
        let already_retained_bytes = 13_u128;
        let exact_cap = already_retained_bytes
            .checked_add(projection.required_peak_bytes)
            .expect("exact cap");
        let (bounded, report) =
            TSpinCoverageOnlyMaterializer::materialize_all_paths_with_memory_limit(
                &batch,
                0..1,
                &ExecutionControl::default(),
                already_retained_bytes,
                exact_cap,
            )
            .expect("exact all-path cap");
        let compatible = TSpinCoverageOnlyMaterializer::materialize_all_paths(
            &batch,
            0..1,
            &ExecutionControl::default(),
        )
        .expect("compatibility all paths");
        assert_eq!(bounded.covered_patterns(), compatible.covered_patterns());
        assert_eq!(report.projection, projection);
        assert!(report.retained_bytes <= projection.required_peak_bytes);

        assert!(matches!(
            TSpinCoverageOnlyMaterializer::materialize_all_paths_with_memory_limit(
                &batch,
                0..1,
                &ExecutionControl::default(),
                already_retained_bytes,
                exact_cap - 1,
            ),
            Err(TSpinCoverageMaterializationError::MemoryCapacityExceeded {
                required_memory_bytes,
                max_memory_bytes,
            }) if required_memory_bytes == exact_cap && max_memory_bytes == exact_cap - 1
        ));
    }
}
