use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};

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
    candidate_ids: BTreeSet<u64>,
    candidate_keys: BTreeSet<String>,
    candidate_coverages: Vec<CandidatePatternCoverage>,
    witnessed_pattern_count: u128,
    complete: bool,
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
            self.words.resize(source.words.len(), 0);
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

struct SupplyTransitionCache<'a> {
    batch: SpinBatchRef<'a>,
    word_count: usize,
    transitions: BTreeMap<TransitionCacheKey, Vec<TransitionMask>>,
    terminal_masks: BTreeMap<(u16, Option<PieceKind>), Arc<[u64]>>,
}

impl<'a> SupplyTransitionCache<'a> {
    fn new(batch: SpinBatchRef<'a>) -> Self {
        Self {
            batch,
            word_count: batch.patterns().len().div_ceil(u64::BITS as usize),
            transitions: BTreeMap::new(),
            terminal_masks: BTreeMap::new(),
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
        if !self.transitions.contains_key(&key) {
            let masks = self.build_transition_masks(key);
            self.transitions.insert(key, masks);
        }
        self.transitions
            .get(&key)
            .expect("transition cache entry was inserted")
            .clone()
    }

    fn terminal_mask(&mut self, state: ProductState) -> Arc<[u64]> {
        let key = (state.cursor, state.hold);
        if !self.terminal_masks.contains_key(&key) {
            let mut words = vec![0_u64; self.word_count];
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
            self.terminal_masks.insert(key, words.into());
        }
        Arc::clone(
            self.terminal_masks
                .get(&key)
                .expect("terminal cache entry was inserted"),
        )
    }

    fn build_transition_masks(&self, key: TransitionCacheKey) -> Vec<TransitionMask> {
        let mut branches = BTreeMap::<(HoldDecision, u16, Option<PieceKind>), Vec<u64>>::new();
        for (pattern_id, sequence) in self.batch.patterns().iter().enumerate() {
            let state = SupplyState {
                node: 0,
                cursor: key.cursor,
                hold: key.hold,
            };
            if key.release_held_at_terminal
                && state.cursor as usize == sequence.len()
                && state.hold == Some(key.required_piece)
                && first_standard_bag_lookahead(sequence).is_none()
            {
                let words = branches
                    .entry((
                        HoldDecision::ReleaseHeldAtTerminal {
                            held_piece: key.required_piece,
                        },
                        state.cursor.saturating_add(1),
                        state.hold,
                    ))
                    .or_insert_with(|| vec![0_u64; self.word_count]);
                set_pattern(words, pattern_id);
            }
            for_each_supply_successor(
                &self.batch,
                sequence,
                state,
                key.required_piece,
                |decision, next| {
                    let words = branches
                        .entry((decision, next.cursor, next.hold))
                        .or_insert_with(|| vec![0_u64; self.word_count]);
                    set_pattern(words, pattern_id);
                    Ok(())
                },
            )
            .expect("supply mask construction cannot be cancelled");
        }
        branches
            .into_iter()
            .map(
                |((_decision, next_cursor, next_hold), pattern_words)| TransitionMask {
                    next_cursor,
                    next_hold,
                    pattern_words: pattern_words.into(),
                },
            )
            .collect()
    }
}

pub struct TSpinCoverageOnlyMaterializer;

impl TSpinCoverageOnlyMaterializer {
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

    pub fn materialize_all_paths(
        batch: &ExactScoringExecutionBatch,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_batch(SpinBatchRef::Scoring(batch), None, pattern_range, control)
    }

    pub fn materialize_all_spin_paths(
        batch: &SpinCoverageExecutionBatch,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_batch(SpinBatchRef::Coverage(batch), None, pattern_range, control)
    }

    fn materialize_batch(
        batch: SpinBatchRef<'_>,
        target: Option<SpinCoverageTarget>,
        pattern_range: Range<usize>,
        control: &ExecutionControl,
    ) -> Result<TSpinCoverageOnlyMaterialization, ExactScoringExecutionCancelled> {
        let pattern_count = batch.patterns().len();
        let start = pattern_range.start.min(pattern_count);
        let end = pattern_range.end.min(pattern_count).max(start);
        let word_count = pattern_count.div_ceil(u64::BITS as usize);
        let mut initial_words = vec![0_u64; word_count];
        for pattern_id in start..end {
            set_pattern(&mut initial_words, pattern_id);
        }
        let mut covered_words = vec![0_u64; word_count];
        let mut coverage_by_candidate = BTreeMap::<String, (u64, PatternBitSet)>::new();
        let mut cache = SupplyTransitionCache::new(batch);
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
            let mut graph_words = vec![0_u64; word_count];
            let graph_complete = evaluate_graph_product(
                batch,
                graph,
                &initial_words,
                &mut cache,
                &mut graph_words,
                target,
                control,
            )?;
            complete &= graph_complete;
            if words_are_nonempty(&graph_words) {
                let candidate_key = graph.candidate_key();
                let graph_coverage = PatternBitSet::from_words(pattern_count, graph_words.clone())
                    .expect("candidate coverage preserves the pattern universe");
                match coverage_by_candidate.entry(candidate_key) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert((graph.candidate_id(), graph_coverage));
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        let (candidate_id, coverage) = entry.get_mut();
                        *candidate_id = (*candidate_id).min(graph.candidate_id());
                        coverage
                            .union_with(&graph_coverage)
                            .expect("candidate coverage preserves the pattern universe");
                    }
                }
                union_words(&mut covered_words, &graph_words);
            }
        }
        control.report_progress(
            "t-spin-coverage",
            batch.graph_count() as u64,
            Some(batch.graph_count() as u64),
        );
        let candidate_ids = coverage_by_candidate
            .values()
            .map(|(candidate_id, _)| *candidate_id)
            .collect();
        let candidate_keys = coverage_by_candidate.keys().cloned().collect();
        let witnessed_pattern_count = coverage_by_candidate
            .values()
            .map(|(_, coverage)| u128::from(coverage.count_ones()))
            .sum();
        let candidate_coverages = coverage_by_candidate
            .into_iter()
            .map(
                |(candidate_key, (candidate_id, covered_patterns))| CandidatePatternCoverage {
                    candidate_id,
                    candidate_key,
                    covered_patterns,
                },
            )
            .collect();
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
) -> Result<bool, ExactScoringExecutionCancelled> {
    let mut states = (0..graph.node_count())
        .map(|_| BTreeMap::<ProductState, PatternSet>::new())
        .collect::<Vec<_>>();
    let Some(root_states) = states.get_mut(graph.root() as usize) else {
        return Ok(false);
    };
    root_states.insert(
        ProductState {
            cursor: batch.initial_cursor(),
            hold: batch.initial_hold(),
            has_target_spin: false,
        },
        PatternSet::single(initial_words.to_vec()),
    );

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
                    states[child_index].entry(next).or_default().add_filtered(
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

fn set_pattern(words: &mut [u64], pattern_id: usize) {
    words[pattern_id / u64::BITS as usize] |= 1_u64 << (pattern_id % u64::BITS as usize);
}

fn intersect_words(left: &[u64], right: &[u64]) -> Option<Vec<u64>> {
    let mut nonempty = false;
    let words = left
        .iter()
        .zip(right)
        .map(|(left, right)| {
            let word = left & right;
            nonempty |= word != 0;
            word
        })
        .collect();
    nonempty.then_some(words)
}

fn union_words(target: &mut [u64], source: &[u64]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target |= source;
    }
}

fn words_are_nonempty(words: &[u64]) -> bool {
    words.iter().any(|word| *word != 0)
}
