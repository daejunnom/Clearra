//! Producer-owned full-height queue-observation language.
//!
//! Geometry/BuildUp still defines the exact physical candidate family. The
//! observation evaluator only replaces its aggregate queue coverage, using the
//! same determinized language and VisibleSeven policy as the compact engine.
//! Worker-local policy coverages must not be ORed: workers transfer languages
//! into this root owner and the policy is solved once for the complete family.
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;
use clearra_supply::pattern_universe::PatternPiecePositionIndex;

use super::{
    buildup::BuildOrderGraph,
    piece_order_language::PieceOrderLanguageCache,
    queue_observation_policy::{
        QueueObservationCoverage, QueueObservationMetrics, QueueObservationPolicyEvaluator,
        RootedPieceLanguageUnion,
    },
    WasmExactSearchError,
};

#[derive(Default)]
pub(super) struct ExtendedPcObservationLanguage {
    language: PieceOrderLanguageCache,
    roots: Vec<u32>,
    metrics: QueueObservationMetrics,
    evaluation_peak_bytes: u128,
    evaluated: bool,
}

impl ExtendedPcObservationLanguage {
    pub fn for_problem(problem: &SearchProblem) -> Option<Self> {
        problem
            .queue_observation_policy()
            .requires_observation_policy()
            .then(Self::default)
    }

    pub fn insert_graph(&mut self, graph: &BuildOrderGraph) -> Result<(), WasmExactSearchError> {
        if !graph.is_live() {
            return Ok(());
        }
        let root = self.language.canonicalize(graph)?;
        self.roots.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("extended_pc_observation_root_storage_unavailable")
        })?;
        self.roots.push(root);
        Ok(())
    }

    /// Called only after the parent verified the exact shared problem/catalog
    /// binding of the worker. Root IDs are rebound by exact edge comparison.
    pub fn merge_from(&mut self, source: &Self) -> Result<(), WasmExactSearchError> {
        let imported = self
            .language
            .import_roots(&source.language, &source.roots)?;
        self.roots.try_reserve(imported.len()).map_err(|_| {
            WasmExactSearchError::InvalidProblem("extended_pc_observation_root_storage_unavailable")
        })?;
        self.roots.extend(imported);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.language.clear_retain_capacity();
        self.roots.clear();
        self.metrics = QueueObservationMetrics::default();
        self.evaluation_peak_bytes = 0;
        self.evaluated = false;
    }

    fn determinized_union(
        &mut self,
    ) -> Result<Option<RootedPieceLanguageUnion>, WasmExactSearchError> {
        if self.roots.is_empty() {
            return Ok(None);
        }
        self.roots.sort_unstable();
        self.roots.dedup();
        RootedPieceLanguageUnion::new(&self.language, &self.roots).map(Some)
    }

    pub fn evaluate_with_memory_guard<F>(
        &mut self,
        problem: &SearchProblem,
        control: &ExecutionControl,
        mut memory_guard: F,
    ) -> Result<Option<QueueObservationCoverage>, WasmExactSearchError>
    where
        F: FnMut(u128) -> Result<(), WasmExactSearchError>,
    {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if self.roots.is_empty() {
            self.evaluated = true;
            return Ok(None);
        }
        self.roots.sort_unstable();
        self.roots.dedup();
        let scratch_bytes = (self.language.node_count() as u128)
            .checked_mul(core::mem::size_of::<u128>() as u128)
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_memory_projection_overflow",
            ))?;
        // Admit the DP itself, then the proved complete construction credit.
        // No union/index/trie/policy allocation precedes either guard.
        memory_guard(scratch_bytes)?;
        let prefix_nodes = match self.language.checked_prefix_path_upper_bound(&self.roots) {
            Some(bound) => bound,
            None => {
                memory_guard(u128::MAX)?;
                return Err(WasmExactSearchError::InvalidProblem(
                    "extended_pc_observation_memory_projection_overflow",
                ));
            }
        };
        let universe = problem.piece_source().materialized_universe().ok_or(
            WasmExactSearchError::InvalidProblem("wasm_piece_source_not_materialized"),
        )?;
        let sequence_len = (0..universe.pattern_count())
            .map(|pattern| universe.sequence_len_at(pattern))
            .max()
            .unwrap_or(0);
        let future = self.checked_evaluation_future_bytes(problem, prefix_nodes, sequence_len);
        let Some(future) = future else {
            memory_guard(u128::MAX)?;
            return Err(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_memory_projection_overflow",
            ));
        };
        memory_guard(future)?;
        let language = self
            .determinized_union()?
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_language_missing_after_admission",
            ))?;
        let index = PatternPiecePositionIndex::compile(universe).map_err(|_| {
            WasmExactSearchError::InvalidProblem("wasm_observation_pattern_index_compile_failed")
        })?;
        let mut evaluator = QueueObservationPolicyEvaluator::new(
            universe,
            &index,
            problem.queue_observation_policy(),
            problem.initial_hold().cursor(),
            problem.initial_hold().hold_piece(),
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead(),
            problem.supply().projects_standard_bag_lookahead(),
            None,
        )?;
        let mut coverage = evaluator.evaluate(&language, control)?;
        coverage.metrics.retained_bytes = coverage
            .metrics
            .retained_bytes
            .saturating_add(language.retained_bytes())
            .saturating_add(index.retained_bytes())
            .saturating_add(coverage.covered_patterns.retained_bytes());
        self.metrics = coverage.metrics;
        self.evaluation_peak_bytes = self
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(coverage.metrics.retained_bytes as u128))
            .ok_or(WasmExactSearchError::InvalidProblem(
                "extended_pc_observation_memory_projection_overflow",
            ))?;
        self.evaluated = true;
        Ok(Some(coverage))
    }

    fn checked_evaluation_future_bytes(
        &self,
        problem: &SearchProblem,
        prefix_nodes: u128,
        sequence_len: usize,
    ) -> Option<u128> {
        let universe = problem.piece_source().materialized_universe()?;
        let source_nodes = self.language.node_count() as u128;
        let source_edges = self.language.edge_count() as u128;
        let depth = u128::from(self.language.maximum_root_depth(&self.roots)?);
        // A union-builder key contains at most every source node once. Include
        // all interned keys, output nodes and in-flight recursive child sets.
        let union_entry = source_nodes.checked_mul(4)?.checked_add(128)?;
        let union_bytes = prefix_nodes.checked_mul(union_entry)?.checked_mul(8)?;
        // Before deduplication a child set contains all matching source edges,
        // not merely unique source nodes (piece-order DAGs are nondeterministic).
        let recursive_union_bytes = source_edges
            .checked_mul(4)?
            .checked_mul(depth.checked_add(1)?)?
            .checked_mul(8)?;
        let words = universe.pattern_count().div_ceil(64) as u128;
        // Index compilation temporarily clones the materialized universe. Its
        // explicit/factorized buffers are not all Arc-backed, so account for
        // their complete clone before compiling rather than only the index.
        let universe_clone_bytes = universe
            .checked_retained_capacity_bytes()?
            .checked_add(core::mem::size_of_val(universe) as u128)?
            .checked_mul(2)?;
        let index_bytes = (sequence_len as u128)
            .checked_mul(7)?
            .checked_mul(words)?
            .checked_mul(core::mem::size_of::<u64>() as u128)?
            .checked_add((universe.pattern_count() as u128).checked_mul(4)?)?
            .checked_add((sequence_len as u128).checked_mul(4)?)?
            .checked_mul(2)?;
        let policy_sequence_len = sequence_len.checked_add(usize::from(
            problem.supply().projects_standard_bag_lookahead(),
        ))?;
        let policy_bytes = QueueObservationPolicyEvaluator::checked_evaluation_peak_upper_bound(
            universe.pattern_count(),
            policy_sequence_len,
            prefix_nodes,
        )?;
        union_bytes
            .checked_add(recursive_union_bytes)?
            .checked_add(index_bytes)?
            .checked_add(universe_clone_bytes)?
            .checked_add(policy_bytes)?
            .checked_add(32 * 1024)
    }

    pub fn evaluated(&self) -> bool {
        self.evaluated
    }

    pub fn metrics(&self) -> QueueObservationMetrics {
        self.metrics
    }

    pub fn evaluation_peak_bytes(&self) -> u128 {
        self.evaluation_peak_bytes
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        (self.language.retained_bytes() as u128).checked_add(
            (self.roots.capacity() as u128).checked_mul(core::mem::size_of::<u32>() as u128)?,
        )
    }

    /// Conservative allocation credit before a graph is interned. Existing
    /// owner capacity, bucket growth, node/edge scratch and root growth coexist.
    pub fn checked_graph_insert_future_bytes(&self, graph: &BuildOrderGraph) -> Option<u128> {
        self.checked_retained_bytes()?
            .checked_mul(4)?
            .checked_add((graph.retained_bytes() as u128).checked_mul(8)?)?
            .checked_add(32 * 1024)
    }

    pub fn checked_merge_future_bytes(&self, source: &Self) -> Option<u128> {
        self.checked_retained_bytes()?
            .checked_add(source.checked_retained_bytes()?)?
            .checked_mul(8)?
            .checked_add(32 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask,
        piece::{piece_kind::PieceKind, rotation::RotationState},
        probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::{
        pattern_universe::MaterializedPatternUniverse, queue::fixed_sequence::FixedSequence,
        QueueObservationPolicy,
    };

    use crate::backend::wasm_cpu::buildup::BuildOrderNodeSpec;

    fn chain(sequence: &[PieceKind]) -> BuildOrderGraph {
        let specs = (0..=sequence.len())
            .map(|depth| BuildOrderNodeSpec {
                edge_start: depth as u32,
                edge_count: u32::from(depth < sequence.len()),
                depth: depth as u8,
                accepting: depth == sequence.len(),
            })
            .collect();
        let edges = sequence
            .iter()
            .copied()
            .enumerate()
            .map(|(depth, piece)| {
                BuildOrderGraph::edge(
                    depth as u32 + 1,
                    depth as u8,
                    piece,
                    RotationState::Zero,
                    0,
                    0,
                    0,
                )
            })
            .collect();
        BuildOrderGraph::from_topological_parts(specs, edges, 0, 0, true).unwrap()
    }

    fn coverage(
        owner: &mut ExtendedPcObservationLanguage,
        universe: &MaterializedPatternUniverse,
    ) -> QueueObservationCoverage {
        let index = PatternPiecePositionIndex::compile(universe).unwrap();
        let language = owner.determinized_union().unwrap().unwrap();
        QueueObservationPolicyEvaluator::new(
            universe,
            &index,
            QueueObservationPolicy::VisibleSeven,
            0,
            None,
            true,
            true,
            false,
            None,
        )
        .unwrap()
        .evaluate(&language, &ExecutionControl::default())
        .unwrap()
    }

    fn one_piece_problem() -> SearchProblem {
        let holes = (0..4).fold(Board256Mask::EMPTY, |holes, cell| {
            holes.union(Board256Mask::singleton(cell).unwrap())
        });
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10_from_words(
                4,
                Board256Mask::all_cells(40).unwrap().without(holes).words(),
            )
            .unwrap(),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_allow_hold(false)
        .with_min_remaining_queue(0)
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
        ProblemCompiler::compile_scenario_pc(&query).unwrap()
    }

    #[test]
    fn evaluation_admission_rejects_before_union_index_or_policy_allocation() {
        let problem = one_piece_problem();
        let mut owner = ExtendedPcObservationLanguage::for_problem(&problem).unwrap();
        owner.insert_graph(&chain(&[PieceKind::I])).unwrap();
        let retained = owner.checked_retained_bytes().unwrap();
        let mut guards = 0;
        let error = owner
            .evaluate_with_memory_guard(&problem, &ExecutionControl::default(), |_| {
                guards += 1;
                if guards == 2 {
                    return Err(WasmExactSearchError::InvalidProblem(
                        "test_memory_credit_rejected",
                    ));
                }
                Ok(())
            })
            .err()
            .expect("construction must stop at the pre-allocation guard");
        assert_eq!(error.reason(), "test_memory_credit_rejected");
        assert_eq!(guards, 2, "scratch then construction must both be admitted");
        assert!(!owner.evaluated());
        assert_eq!(owner.metrics().action_checks, 0);
        assert_eq!(owner.evaluation_peak_bytes(), 0);
        assert_eq!(owner.checked_retained_bytes(), Some(retained));
    }

    #[test]
    fn admitted_evaluation_credit_covers_its_reported_live_evidence() {
        let problem = one_piece_problem();
        let mut owner = ExtendedPcObservationLanguage::for_problem(&problem).unwrap();
        owner.insert_graph(&chain(&[PieceKind::I])).unwrap();
        let retained = owner.checked_retained_bytes().unwrap();
        let mut construction_credit = 0;
        let result = owner
            .evaluate_with_memory_guard(&problem, &ExecutionControl::default(), |future| {
                construction_credit = construction_credit.max(future);
                Ok(())
            })
            .unwrap()
            .unwrap();
        assert!(owner.evaluated());
        assert_eq!(result.covered_pattern_count, 1);
        assert_eq!(result.covered_weight, 1.0);
        assert!(construction_credit >= owner.evaluation_peak_bytes() - retained);
    }

    #[test]
    fn worker_language_union_never_promotes_hidden_queue_coverage_to_oracle() {
        use PieceKind::{I, J, L, O, S, T, Z};
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(1),
            PatternWeightModelId::new(1),
            vec![vec![I, O, T, S, Z, J, L, I], vec![I, O, T, S, Z, J, L, O]],
            vec![ProbabilityValue::new(0.5).unwrap(); 2],
            2,
            true,
            None,
        )
        .unwrap();
        let mut left = ExtendedPcObservationLanguage::default();
        left.insert_graph(&chain(&[I, O, T, S, Z, J, L, I]))
            .unwrap();
        let mut right = ExtendedPcObservationLanguage::default();
        right
            .insert_graph(&chain(&[O, I, T, S, Z, J, L, O]))
            .unwrap();
        let mut local_union = coverage(&mut left, &universe).covered_patterns;
        local_union
            .union_with(&coverage(&mut right, &universe).covered_patterns)
            .unwrap();
        assert_eq!(
            local_union.count_ones(),
            2,
            "worker coverage OR is not the policy proof"
        );

        let mut root = ExtendedPcObservationLanguage::default();
        root.merge_from(&right).unwrap();
        root.merge_from(&left).unwrap();
        root.merge_from(&left).unwrap();
        let result = coverage(&mut root, &universe);
        assert_eq!(result.covered_pattern_count, 1);
        assert_eq!(result.covered_weight, 0.5);
        assert!(result.metrics.action_checks > 0);
        assert_eq!(result.min_accepted_depth, Some(8));
        assert_eq!(result.max_accepted_depth, Some(8));

        let mut serial = ExtendedPcObservationLanguage::default();
        serial
            .insert_graph(&chain(&[I, O, T, S, Z, J, L, I]))
            .unwrap();
        serial
            .insert_graph(&chain(&[O, I, T, S, Z, J, L, O]))
            .unwrap();
        assert_eq!(
            result.covered_patterns,
            coverage(&mut serial, &universe).covered_patterns
        );
    }
}
