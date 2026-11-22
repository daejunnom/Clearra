//! Request-local registration of completed worker proofs.
//!
//! Workers retain their own movement/source compiler and suffix arena. A
//! contribution borrows that immutable result until its reachable suffixes
//! have been rebound into this owner's one kind arena. The lock never runs
//! movement search. Pending H groups are not executable policy nodes, and
//! only the complete native-family barrier may turn them into exact classes.

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex, MutexGuard,
};

use crate::backend::wasm_cpu::piece_order_language::PieceLanguageIncrementalImport;

use super::super::PhysicalActionLanguage;
use super::*;

struct SharedProducerState {
    family: Option<BoardActionFamily>,
    kinds: Option<PieceOrderLanguageCache>,
    kind_peak_extra: usize,
}

pub(in crate::backend::wasm_cpu) struct SharedObservationOutput {
    pub(in crate::backend::wasm_cpu) physical: PhysicalActionLanguage,
    pub(in crate::backend::wasm_cpu) kinds: PieceOrderLanguageCache,
    pub(in crate::backend::wasm_cpu) metrics: [usize; 8],
}

pub(in crate::backend::wasm_cpu) struct SharedObservationProducer {
    plan: Arc<BoardObservationPlan>,
    state: Mutex<SharedProducerState>,
    failed: AtomicBool,
    retained: AtomicUsize,
    peak: AtomicUsize,
    #[cfg(feature = "local-search-ab")]
    failure_reported: AtomicBool,
    #[cfg(feature = "local-search-ab")]
    board_metrics: [AtomicUsize; 9],
    // Successful registrations, registration/finalization lock entries,
    // peak pending groups/root references, final classes/finalization work,
    // final producer retained/peak snapshots. Shared snapshots are not summed
    // once per worker; they belong to this single request owner.
    metrics: [AtomicUsize; 8],
}

impl SharedObservationProducer {
    #[cfg(any(test, all(feature = "local-search-ab", not(target_family = "wasm"))))]
    pub(in crate::backend::wasm_cpu) fn new(
        plan: Arc<BoardObservationPlan>,
    ) -> Result<Arc<Self>, WasmExactSearchError> {
        if !plan.boundary_classes_selected() || plan.hidden_horizon > 5 {
            return Err(invalid("wasm_observation_shared_producer_scope_invalid"));
        }
        let kinds = PieceOrderLanguageCache::default();
        let mut family = BoardActionFamily::new(Arc::clone(&plan));
        family.pending_boundaries = Some(PendingBoundaries::default());
        family.kind_arena = Some(Arc::clone(kinds.arena_identity()));
        let initial = Self::header_bytes()
            .saturating_add(family.retained_bytes())
            .saturating_add(kinds.retained_bytes());
        Ok(Arc::new(Self {
            plan,
            state: Mutex::new(SharedProducerState {
                family: Some(family),
                kinds: Some(kinds),
                kind_peak_extra: 0,
            }),
            failed: AtomicBool::new(false),
            retained: AtomicUsize::new(initial),
            peak: AtomicUsize::new(initial),
            #[cfg(feature = "local-search-ab")]
            failure_reported: AtomicBool::new(false),
            #[cfg(feature = "local-search-ab")]
            board_metrics: std::array::from_fn(|_| AtomicUsize::new(0)),
            metrics: std::array::from_fn(|index| {
                AtomicUsize::new(if index >= 6 { initial } else { 0 })
            }),
        }))
    }

    pub(in crate::backend::wasm_cpu) fn plan(&self) -> &BoardObservationPlan {
        &self.plan
    }

    pub(in crate::backend::wasm_cpu) fn abort(&self) {
        self.failed.store(true, Ordering::Release);
    }

    pub(in crate::backend::wasm_cpu) fn abort_with_error(
        &self,
        stage: &'static str,
        error: &WasmExactSearchError,
    ) {
        #[cfg(feature = "local-search-ab")]
        if !matches!(
            error,
            WasmExactSearchError::InvalidProblem("wasm_observation_shared_producer_failed")
        ) && !self.failure_reported.swap(true, Ordering::AcqRel)
        {
            let board: [usize; 9] =
                std::array::from_fn(|index| self.board_metrics[index].load(Ordering::Acquire));
            eprintln!(
                "clearra_observation_shared_producer_failed stage={stage} error={error:?} board={board:?} shared={:?} retained_bytes={} peak_bytes={}",
                self.metrics(),
                self.retained_bytes(),
                self.peak_bytes(),
            );
        }
        #[cfg(not(feature = "local-search-ab"))]
        let _ = (stage, error);
        self.abort();
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    pub(in crate::backend::wasm_cpu) fn retained_bytes(&self) -> usize {
        self.retained.load(Ordering::Acquire)
    }

    #[cfg(any(test, feature = "local-search-ab"))]
    pub(in crate::backend::wasm_cpu) fn peak_bytes(&self) -> usize {
        self.peak.load(Ordering::Acquire)
    }

    pub(in crate::backend::wasm_cpu) fn metrics(&self) -> [usize; 8] {
        std::array::from_fn(|index| self.metrics[index].load(Ordering::Acquire))
    }

    fn header_bytes() -> usize {
        core::mem::size_of::<Self>() + 2 * core::mem::size_of::<usize>()
    }

    fn observe(&self, state: &SharedProducerState) {
        let (Some(family), Some(kinds)) = (&state.family, &state.kinds) else {
            self.retained.store(Self::header_bytes(), Ordering::Release);
            return;
        };
        let retained = Self::header_bytes()
            .saturating_add(family.retained_bytes())
            .saturating_add(kinds.retained_bytes());
        let peak = Self::header_bytes()
            .saturating_add(family.peak_bytes().max(family.retained_bytes()))
            .saturating_add(kinds.retained_bytes())
            .saturating_add(state.kind_peak_extra);
        self.retained.store(retained, Ordering::Release);
        self.peak.fetch_max(peak.max(retained), Ordering::AcqRel);
        self.metrics[0].store(family.metrics.candidate_count, Ordering::Release);
        if let Some(pending) = &family.pending_boundaries {
            self.metrics[2].fetch_max(pending.groups.len(), Ordering::AcqRel);
            self.metrics[3].fetch_max(family.metrics.suffix_roots, Ordering::AcqRel);
        }
        self.metrics[4].store(family.boundary_class_metrics[2], Ordering::Release);
        self.metrics[5].store(family.boundary_class_metrics[6], Ordering::Release);
        self.metrics[6].store(retained, Ordering::Release);
        self.metrics[7].store(self.peak.load(Ordering::Acquire), Ordering::Release);
        #[cfg(feature = "local-search-ab")]
        for (counter, value) in self.board_metrics.iter().zip([
            family.metrics.candidate_count,
            family.metrics.nodes,
            family.metrics.edges,
            family.metrics.suffix_roots,
            family.metrics.prefix_visits,
            family.metrics.edge_attempts,
            family.metrics.work,
            family.metrics.source_pruned_prefixes,
            family.metrics.dead_source_prefixes,
        ]) {
            counter.store(value, Ordering::Release);
        }
    }

    fn check(&self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if self.failed.load(Ordering::Acquire) {
            return Err(invalid("wasm_observation_shared_producer_failed"));
        }
        Ok(())
    }

    fn lock(
        &self,
        control: &ExecutionControl,
    ) -> Result<MutexGuard<'_, SharedProducerState>, WasmExactSearchError> {
        if let Err(error) = self.check(control) {
            self.abort_with_error("lock", &error);
            return Err(error);
        }
        let state = self.state.lock().map_err(|_| {
            let error = lock_poisoned();
            self.abort_with_error("lock", &error);
            error
        })?;
        self.metrics[1].fetch_add(1, Ordering::AcqRel);
        if let Err(error) = self.check(control) {
            self.abort_with_error("lock", &error);
            return Err(error);
        }
        Ok(state)
    }

    pub(in crate::backend::wasm_cpu) fn register(
        &self,
        orders: &SourceQualifiedPrefixOrders<'_>,
        suffixes: &[u32],
        source: &PieceOrderLanguageCache,
        importer: &mut PieceLanguageIncrementalImport,
        control: &ExecutionControl,
    ) -> Result<(), WasmExactSearchError> {
        if !orders.has_witness() {
            let error = invalid("wasm_observation_source_witness_language_missing");
            self.abort_with_error("registration_authority", &error);
            return Err(error);
        }
        let mut state = self.lock(control)?;
        let result = (|| {
            let SharedProducerState {
                family,
                kinds,
                kind_peak_extra,
            } = &mut *state;
            let family = family
                .as_mut()
                .ok_or_else(|| invalid("wasm_observation_shared_producer_closed"))?;
            let kinds = kinds
                .as_mut()
                .ok_or_else(|| invalid("wasm_observation_shared_producer_closed"))?;
            // The new lock-stage operation is additional actual work. The
            // existing candidate/prefix/edge/cache charges remain unchanged.
            family.charge(1, control)?;
            let mut root = |prefix: usize, remaining: u8, control: &ExecutionControl| {
                self.check(control)?;
                let root = *suffixes
                    .get(prefix)
                    .ok_or_else(|| invalid("wasm_observation_suffix_language_missing"))?;
                let rebound = importer.import_root(source, kinds, root, remaining, control);
                *kind_peak_extra = (*kind_peak_extra).max(importer.destination_peak_extra_bytes());
                self.check(control)?;
                rebound
            };
            family.register_source_qualified_with_suffixes(
                orders,
                suffixes.len(),
                &mut root,
                control,
            )?;
            self.check(control)
        })();
        if result.is_err() {
            if let Some(family) = &mut state.family {
                family.failed = true;
            }
        }
        self.observe(&state);
        if let Err(error) = &result {
            self.abort_with_error("registration", error);
        }
        result
    }

    /// Exactly once, after the native coordinator has checked every canonical
    /// branch and all worker results. No partial family can expose classes.
    pub(in crate::backend::wasm_cpu) fn finish(
        &self,
        completed_native_family: bool,
        control: &ExecutionControl,
    ) -> Result<SharedObservationOutput, WasmExactSearchError> {
        if !completed_native_family {
            let error = invalid("wasm_observation_shared_family_incomplete");
            self.abort_with_error("completion_authority", &error);
            return Err(error);
        }
        let mut state = self.lock(control)?;
        self.observe(&state);
        let result = (|| {
            let SharedProducerState { family, kinds, .. } = &mut *state;
            let family = family
                .as_mut()
                .ok_or_else(|| invalid("wasm_observation_shared_producer_closed"))?;
            let kinds = kinds
                .as_ref()
                .ok_or_else(|| invalid("wasm_observation_shared_producer_closed"))?;
            family.finish_pending_boundaries(kinds, control)?;
            self.check(control)
        })();
        if let Err(error) = result {
            if let Some(family) = &mut state.family {
                family.failed = true;
            }
            self.observe(&state);
            self.abort_with_error("finalization", &error);
            return Err(error);
        }
        self.observe(&state);
        let metrics = self.metrics();
        let family = state.family.take().unwrap();
        let kinds = state.kinds.take().unwrap();
        let physical = PhysicalActionLanguage {
            board: Some(family),
            suffix_arena: Some(Arc::clone(kinds.arena_identity())),
            ..PhysicalActionLanguage::default()
        };
        self.observe(&state);
        Ok(SharedObservationOutput {
            physical,
            kinds,
            metrics,
        })
    }
}

fn lock_poisoned() -> WasmExactSearchError {
    invalid("wasm_observation_shared_producer_lock_poisoned")
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::probability::probability_value::ProbabilityValue;
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_rules::kicks::KickTableProfileId;
    use clearra_supply::{pattern_universe::MaterializedPatternUniverse, QueueObservationPolicy};

    use crate::backend::wasm_cpu::{
        piece_order_language::PrefixSuffixCompilation,
        queue_observation_policy::QueueObservationPolicyEvaluator,
        symbolic_prefix_orders::SymbolicPrefixWork,
    };

    use super::super::tests::{fresh_bag_orders_for_tail, physical_prefixes, source_filter_plan};
    use super::*;

    fn plan() -> Arc<BoardObservationPlan> {
        let mut plan = source_filter_plan(true);
        let scope = Arc::get_mut(&mut plan).unwrap();
        scope.boundary_classes_enabled = true;
        scope.edge_closure = EdgeClosureMode::BeforeLastPhysicalLayer;
        scope.prefix_cones_enabled = true;
        scope.prefix_cones_exact = true;
        scope.prefix_cones_ancestors_only = true;
        plan
    }

    fn local(plan: &Arc<BoardObservationPlan>) -> PhysicalActionLanguage {
        let mut local = PhysicalActionLanguage::default();
        local.set_board_plan(Arc::clone(plan)).unwrap();
        local
    }

    fn produce(
        local: &mut PhysicalActionLanguage,
        kinds: &mut PieceOrderLanguageCache,
        importer: &mut PieceLanguageIncrementalImport,
        shared: &SharedObservationProducer,
        alternate: bool,
    ) -> Result<PrefixSuffixCompilation, WasmExactSearchError> {
        local.produce_shared_board_relation(
            fresh_bag_orders_for_tail(alternate),
            kinds,
            &mut SymbolicPrefixWork::default(),
            shared,
            importer,
            &ExecutionControl::default(),
            0,
        )
    }

    fn ordinary_worker(
        scope: &Arc<BoardObservationPlan>,
    ) -> (BoardActionFamily, PieceOrderLanguageCache) {
        let mut owner = local(scope);
        let mut kinds = PieceOrderLanguageCache::default();
        for alternate in [false, true] {
            assert_eq!(
                owner
                    .produce_board_relation(
                        fresh_bag_orders_for_tail(alternate),
                        &mut kinds,
                        &mut SymbolicPrefixWork::default(),
                        &ExecutionControl::default(),
                        0,
                    )
                    .unwrap(),
                PrefixSuffixCompilation::Compiled
            );
        }
        (owner.board.take().unwrap(), kinds)
    }

    #[test]
    fn shared_producer_matches_closed_shards_actions_suffixes_and_original_ids() {
        use PieceKind::{I, J, L, O, S, T, Z};
        let scope = plan();
        let shared = SharedObservationProducer::new(Arc::clone(&scope)).unwrap();
        // Two real worker-local arenas register the same candidate set in
        // opposite orders. Completed cone and kind IDs are shared only after
        // exact import; the per-worker local owner remains metadata-only.
        let source_nodes = std::thread::scope(|threads| {
            let first = threads.spawn(|| {
                let mut owner = local(&scope);
                let mut kinds = PieceOrderLanguageCache::default();
                let mut importer = PieceLanguageIncrementalImport::default();
                for alternate in [false, true] {
                    produce(&mut owner, &mut kinds, &mut importer, &shared, alternate).unwrap();
                }
                assert!(!owner.has_roots());
                assert_eq!(owner.source_filter_metrics()[0], 2);
                assert!(importer.metrics().root_visits > 0);
                kinds.node_count()
            });
            let second = threads.spawn(|| {
                let mut owner = local(&scope);
                let mut kinds = PieceOrderLanguageCache::default();
                let mut importer = PieceLanguageIncrementalImport::default();
                for alternate in [true, false] {
                    produce(&mut owner, &mut kinds, &mut importer, &shared, alternate).unwrap();
                }
                assert!(!owner.has_roots());
                kinds.node_count()
            });
            first.join().unwrap() + second.join().unwrap()
        });
        let control = ExecutionControl::default();
        let before_work;
        {
            let state = shared.state.lock().unwrap();
            let family = state.family.as_ref().unwrap();
            before_work = family.metrics.work;
            assert!(family.pending_boundaries.as_ref().unwrap().groups.len() > 1);
            assert!(family
                .nodes
                .iter()
                .all(|node| { node.physical_key().is_some() && node.remaining() > 6 }));
            assert!(family.prefix_cone_metrics()[1] > 0);
            assert!(family.prefix_cone_exact_metrics()[3] > 0);
            assert!(family
                .retained_suffix_roots(state.kinds.as_ref().unwrap(), &control)
                .is_err());
        }
        let mut output = shared.finish(true, &control).unwrap();
        assert_eq!(output.metrics[0], 4);
        assert_eq!(output.metrics[1], 5);
        assert!(output.metrics[2] > 0 && output.metrics[3] > 0);
        assert!(output.metrics[4] > 0 && output.metrics[5] > 0);
        assert!(output.metrics[7] >= output.metrics[6]);
        assert!(output.kinds.node_count() < source_nodes);
        let shared_family = output.physical.board.as_mut().unwrap();
        assert!(shared_family.pending_boundaries.is_none());
        assert!(shared_family.producer_closed);
        assert_eq!(shared_family.metrics.work, before_work + output.metrics[5]);

        let (first, first_kinds) = ordinary_worker(&scope);
        let (second, second_kinds) = ordinary_worker(&scope);
        let mut kinds = PieceOrderLanguageCache::default();
        let first_map = kinds.import_node_map(&first_kinds).unwrap();
        let second_map = kinds.import_node_map(&second_kinds).unwrap();
        let mut ordinary = BoardActionFamily::new(scope);
        ordinary
            .merge_completed_boundaries(
                vec![first, second],
                &[&first_map, &second_map],
                0,
                &kinds,
                &control,
            )
            .unwrap();
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, (1_u64 << 40) - 1)
                .unwrap();
        for family in [&mut ordinary, &mut *shared_family] {
            let mut reachability = ReachabilityWorkspace::default();
            reachability.configure_kick_profile(KickTableProfileId::Jstris180, false);
            family
                .complete_physical_edges(&catalog, &mut reachability, &control)
                .unwrap();
        }
        let first = vec![J, I, O, T, Z, S, L, Z, T, I, O];
        let second = vec![J, I, O, T, Z, S, L, Z, T, I, S];
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(998),
            PatternWeightModelId::new(999),
            vec![first.clone(), second, first],
            vec![ProbabilityValue::new(1.0 / 3.0).unwrap(); 3],
            3,
            true,
            None,
        )
        .unwrap();
        let mut expected = None;
        for union in [false, true] {
            for (family, arena) in [
                (&mut ordinary, &kinds),
                (&mut *shared_family, &output.kinds),
            ] {
                let view = family.view_with_kind_union(arena, &control, union).unwrap();
                let paths = physical_prefixes(&view, view.root(), 4);
                let coverage = QueueObservationPolicyEvaluator::for_original_universe(
                    &universe,
                    QueueObservationPolicy::VisibleSeven,
                    0,
                    None,
                    true,
                    false,
                    false,
                    None,
                )
                .unwrap()
                .evaluate(&view, &control)
                .unwrap();
                assert_eq!(coverage.covered_pattern_count, 3);
                let actual = (
                    paths,
                    coverage.covered_patterns.words().to_vec(),
                    coverage.covered_weight.to_bits(),
                    coverage.min_accepted_depth,
                    coverage.max_accepted_depth,
                );
                if let Some(expected) = &expected {
                    assert_eq!(&actual, expected);
                } else {
                    expected = Some(actual);
                }
            }
        }
        assert!(shared.finish(true, &control).is_err());
    }

    #[test]
    fn shared_producer_keeps_member_work_caps_and_rejects_unproved_pending_edges() {
        let scope = plan();
        let shared = SharedObservationProducer::new(Arc::clone(&scope)).unwrap();
        let mut owner = local(&scope);
        let mut kinds = PieceOrderLanguageCache::default();
        let mut importer = PieceLanguageIncrementalImport::default();
        produce(&mut owner, &mut kinds, &mut importer, &shared, false).unwrap();
        {
            let mut state = shared.state.lock().unwrap();
            let family = state.family.as_mut().unwrap();
            let (&parent, edges) = family
                .pending_boundaries
                .as_ref()
                .unwrap()
                .edges
                .iter()
                .next()
                .unwrap();
            let child = ProducerNode::pending(edges[0].group).unwrap();
            assert!(matches!(
                family.insert_producer_edge(
                    ProducerNode::physical(parent),
                    ProducerEdge {
                        piece: 1,
                        cells: 1,
                        child
                    },
                ),
                Err(WasmExactSearchError::InvalidProblem(
                    "wasm_observation_board_transition_invalid"
                ))
            ));
            family.metrics.suffix_roots = MAX_UNION_MEMBERS;
        }
        // A duplicate binding at capacity is a hit; a genuinely new future
        // root must fail even though its H group is still only pending.
        produce(&mut owner, &mut kinds, &mut importer, &shared, false).unwrap();
        assert!(matches!(
            produce(&mut owner, &mut kinds, &mut importer, &shared, true),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_union_limit"
            ))
        ));
        assert!(shared.finish(true, &ExecutionControl::default()).is_err());

        let limited = SharedObservationProducer::new(Arc::clone(&scope)).unwrap();
        limited
            .state
            .lock()
            .unwrap()
            .family
            .as_mut()
            .unwrap()
            .metrics
            .work = MAX_WORK - 1;
        assert!(matches!(
            produce(
                &mut local(&scope),
                &mut PieceOrderLanguageCache::default(),
                &mut PieceLanguageIncrementalImport::default(),
                &limited,
                false,
            ),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_physical_work_limit"
            ))
        ));
        assert_eq!(
            limited
                .state
                .lock()
                .unwrap()
                .family
                .as_ref()
                .unwrap()
                .metrics
                .work,
            MAX_WORK
        );
        assert!(limited.finish(true, &ExecutionControl::default()).is_err());
    }

    #[test]
    fn shared_producer_source_recycle_and_completion_authority_remain_explicit() {
        let scope = plan();
        let shared = SharedObservationProducer::new(Arc::clone(&scope)).unwrap();
        let mut owner = local(&scope);
        let mut kinds = PieceOrderLanguageCache::default();
        let mut importer = PieceLanguageIncrementalImport::default();
        produce(&mut owner, &mut kinds, &mut importer, &shared, false).unwrap();
        let inserted = importer.metrics().inserted_nodes;
        let source_nodes = importer.metrics().source_nodes;
        kinds = PieceOrderLanguageCache::default();
        importer.reset_source();
        produce(&mut owner, &mut kinds, &mut importer, &shared, false).unwrap();
        assert_eq!(importer.metrics().inserted_nodes, inserted);
        assert!(importer.metrics().source_nodes > source_nodes);
        assert_eq!(shared.metrics()[0], 2);
        assert!(matches!(
            shared.finish(false, &ExecutionControl::default()),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_shared_family_incomplete"
            ))
        ));
        assert!(shared.finish(true, &ExecutionControl::default()).is_err());

        let foreign = SharedObservationProducer::new(plan()).unwrap();
        assert!(matches!(
            produce(&mut owner, &mut kinds, &mut importer, &foreign, false),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_shared_producer_scope_invalid"
            ))
        ));
        let cancelled = SharedObservationProducer::new(plan()).unwrap();
        let control = ExecutionControl::default();
        control.cancellation.handle().cancel();
        assert!(matches!(
            cancelled.finish(true, &control),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert!(cancelled
            .finish(true, &ExecutionControl::default())
            .is_err());
    }
}
