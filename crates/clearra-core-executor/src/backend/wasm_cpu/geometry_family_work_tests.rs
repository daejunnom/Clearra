use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;

type CandidateSequence = Vec<(u32, StandardBoard64TilingIdentity, Vec<u32>)>;

fn fixture(height: u16, initial: u64, fixed_i: bool) -> (GeometryCatalog, GeometrySearch) {
    let locks = (usize::from(height) * 10 - initial.count_ones() as usize) / 4;
    let queue = if fixed_i {
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; locks]))
    } else {
        PcQueueInput::standard_7_bag()
    };
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(height, initial),
        queue,
        PieceWindow::new(locks),
    )
    .with_rule(srs_plus())
    .with_allow_hold(!fixed_i)
    .with_exact_pieces(Some(locks))
    .with_supply_window_size(SupplyWindowSize::new(if fixed_i { locks } else { 7 }))
    .with_count_policy(PcCountPolicy::CountUnique);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("family fixture");
    let catalog = GeometryCatalog::compile(&problem).expect("fixture catalog");
    let universe = problem.piece_source().materialized_universe().unwrap();
    let family = universe.packing_multiset_family_for_execution(
        locks,
        problem.initial_hold(),
        !fixed_i,
        crate::backend::wasm_cpu::packing_hold_projection(&problem),
    );
    let geometry = GeometrySearch::new(universe, &family, catalog.required_cells(), false)
        .expect("fixture geometry");
    (catalog, geometry)
}

#[derive(Debug, Eq, PartialEq)]
struct FamilySnapshot {
    nodes: Vec<super::super::geometry_family::FamilyNode>,
    root: u32,
    count: Option<u128>,
    candidates: CandidateSequence,
    ordered_digest: u64,
    unordered_digest: u64,
    metrics: [usize; 7],
    retained: usize,
    peak_retained: usize,
}

fn compile<const REDUCE_WORK: bool>(
    compiler: &mut FamilyCompiler,
    catalog: &GeometryCatalog,
) -> usize {
    let mut peak_retained = compiler.retained_bytes();
    for _ in 0..10_000_000 {
        let advance = if REDUCE_WORK {
            compiler.advance(catalog)
        } else {
            compiler.advance_with_work_reduction::<false>(catalog)
        };
        peak_retained = peak_retained.max(compiler.retained_bytes());
        assert!(compiler.stack.len() <= usize::from(compiler.target_depth) + 1);
        match advance {
            CompileAdvance::Pending => {}
            CompileAdvance::Complete => return peak_retained,
            CompileAdvance::ResourceIncomplete => panic!("small family must be complete"),
        }
    }
    panic!("small family must exhaust within finite test work");
}

fn snapshot<const REDUCE_WORK: bool>(
    mut compiler: FamilyCompiler,
    catalog: &GeometryCatalog,
) -> (FamilySnapshot, usize) {
    let peak_retained = compile::<REDUCE_WORK>(&mut compiler, catalog);
    let advance_calls = compiler.profile.advance_calls;
    let nodes = (2..compiler.family.node_count() + 2)
        .map(|reference| compiler.family.node(reference).unwrap())
        .collect();
    let root = compiler.root_family;
    let count = compiler.candidate_family_count();
    let metrics = [
        compiler.expanded_nodes,
        compiler.peak_frontier,
        compiler.domain_pruned_states,
        compiler.hall_pruned_states,
        compiler.column_pruned_states,
        compiler.component_compositions,
        compiler.tablebase_pruned_states,
    ];
    let retained = compiler.retained_bytes();
    let (mut enumerator, enumerator_count) = compiler.into_enumerator(None);
    assert_eq!(enumerator_count, count);
    let mut candidates = Vec::new();
    let mut ordered_digest = 0;
    let mut unordered_digest = 0;
    while let Some(candidate) = enumerator
        .next_candidate(catalog)
        .expect("complete traversal")
    {
        let hash = candidate.identity.bucket_hash();
        ordered_digest = mix_digest(ordered_digest, hash);
        unordered_digest =
            super::super::mix_order_independent_candidate_digest(unordered_digest, hash);
        candidates.push((
            candidate.target_index,
            candidate.identity,
            candidate.row_ids().to_vec(),
        ));
    }
    assert!(
        !candidates.is_empty(),
        "fixture has at least one exact cover"
    );
    assert_eq!(count, Some(candidates.len() as u128));
    (
        FamilySnapshot {
            nodes,
            root,
            count,
            candidates,
            ordered_digest,
            unordered_digest,
            metrics,
            retained,
            peak_retained,
        },
        advance_calls,
    )
}

#[test]
fn family_work_reduction_preserves_ordered_mixed_ilc_families() {
    let mut reference_calls = 0;
    let mut reduced_calls = 0;
    for initial in [0x3c0f_03c0f, 0x90_747d_83e5] {
        let (catalog, mut search) = fixture(4, initial, false);
        let compiler = search.compiler.take().unwrap();
        let reference =
            FamilyCompiler::new(catalog.required_cells(), Arc::clone(&compiler.targets));
        let (old, old_calls) = snapshot::<false>(reference, &catalog);
        let (new, new_calls) = snapshot::<true>(compiler, &catalog);
        assert_eq!(new, old, "initial board {initial:#x}");
        assert!(new_calls < old_calls, "batching must remove empty advances");
        reference_calls += old_calls;
        reduced_calls += new_calls;
    }
    eprintln!("family work mixed ILC: advance_calls {reference_calls} -> {reduced_calls}");
}

#[test]
fn family_work_reduction_preserves_fixed_and_bounded_families() {
    for (height, initial) in [(4, 0), (4, 0xc030_0c03)] {
        let (catalog, search) = fixture(height, initial, true);
        let targets = Arc::clone(&search.compiler.as_ref().unwrap().targets);
        for bounded in [false, true] {
            let fresh = || {
                if bounded {
                    FamilyCompiler::try_new(catalog.required_cells(), Arc::clone(&targets)).unwrap()
                } else {
                    FamilyCompiler::new(catalog.required_cells(), Arc::clone(&targets))
                }
            };
            let (old, old_calls) = snapshot::<false>(fresh(), &catalog);
            let (new, new_calls) = snapshot::<true>(fresh(), &catalog);
            assert_eq!(new, old, "initial={initial:#x}, bounded={bounded}");
            assert!(new_calls < old_calls);
        }
    }
}

#[test]
fn construction_index_release_handoff_preserves_order_and_mutable_owner_boundaries() {
    fn collect(enumerator: &mut FamilyEnumerator, catalog: &GeometryCatalog) -> CandidateSequence {
        let mut candidates = Vec::new();
        while let Some(candidate) = enumerator.next_candidate(catalog).unwrap() {
            candidates.push((
                candidate.target_index,
                candidate.identity,
                candidate.row_ids().to_vec(),
            ));
        }
        candidates
    }

    fn digests(candidates: &CandidateSequence) -> (u64, u64) {
        candidates
            .iter()
            .fold((0, 0), |(ordered, unordered), (_, identity, _)| {
                let hash = identity.bucket_hash();
                (
                    mix_digest(ordered, hash),
                    super::super::mix_order_independent_candidate_digest(unordered, hash),
                )
            })
    }

    for (initial, fixed_i) in [(0x3c0f_03c0f, false), (0xc030_0c03, true)] {
        let (catalog, mut search) = fixture(4, initial, fixed_i);
        let mut optimized = search.compiler.take().unwrap();
        let mut reference =
            FamilyCompiler::new(catalog.required_cells(), Arc::clone(&optimized.targets));
        compile::<true>(&mut optimized, &catalog);
        compile::<true>(&mut reference, &catalog);
        let root = reference.root_family;
        let depth = reference.target_depth;
        let targets = Arc::clone(&reference.targets);
        let original_retained = reference.family.retained_bytes();
        let expected_counts = reference.family.path_count_table().unwrap();
        let expected_nodes = (2..reference.family.node_count() + 2)
            .map(|reference_id| reference.family.node(reference_id).unwrap())
            .collect::<Vec<_>>();

        // Completion state remains mutable and keeps exact interning reuse.
        let mut state = reference.into_completion_state();
        assert!(state.family.unbounded_interning_active());
        assert_eq!(state.family.retained_bytes(), original_retained);
        let (append_id, append) = (2..state.family.node_count() + 2)
            .map(|reference_id| (reference_id, state.family.node(reference_id).unwrap()))
            .find(|(_, node)| node.kind == FamilyNodeKind::Append)
            .unwrap();
        assert_eq!(
            state.family.append(append.row_id, append.left),
            Some(append_id)
        );

        // The bounded constructor and temporary snapshot-style view keep
        // that construction index intact rather than taking the terminal path.
        let mut reference =
            FamilyEnumerator::try_new_bounded(targets, state.family, root, depth, u128::MAX)
                .unwrap();
        assert!(reference.family.unbounded_interning_active());
        assert_eq!(reference.family.retained_bytes(), original_retained);

        let mut path_counts = None;
        let (mut optimized, count) = optimized.into_enumerator(Some(&mut path_counts));
        assert!(!optimized.family.unbounded_interning_active());
        assert_eq!(optimized.tasks[0].family, root);
        assert_eq!(count, Some(expected_counts[root as usize]));
        assert_eq!(path_counts, Some(expected_counts));
        assert_eq!(
            (2..optimized.family.node_count() + 2)
                .map(|reference_id| optimized.family.node(reference_id).unwrap())
                .collect::<Vec<_>>(),
            expected_nodes,
        );
        let expected = collect(&mut reference, &catalog);
        let actual = collect(&mut optimized, &catalog);
        assert!(!actual.is_empty());
        assert_eq!(actual, expected);
        assert_eq!(digests(&actual), digests(&expected));

        let mut reference_family = Arc::try_unwrap(reference.family).unwrap();
        assert!(reference_family.unbounded_interning_active());
        let released = reference_family.release_construction_index();
        assert!(released > 0);
        assert_eq!(
            optimized.family.retained_bytes() + released,
            original_retained
        );
        assert_eq!(
            optimized.family.retained_bytes(),
            reference_family.retained_bytes()
        );
    }
}

#[cfg(feature = "parallel")]
fn collect_prepared(
    prepared: ParallelGeometryPreparation,
    catalog: &GeometryCatalog,
) -> Vec<(Option<u128>, CandidateSequence)> {
    let searches = match prepared {
        ParallelGeometryPreparation::Ready { plan, .. } => plan.searches,
        ParallelGeometryPreparation::Serial { geometry, .. } => vec![geometry],
    };
    searches
        .into_iter()
        .map(|mut search| {
            let count = search.candidate_family_count();
            let mut candidates = Vec::new();
            loop {
                match search.advance(catalog) {
                    GeometryAdvance::Pending => {}
                    GeometryAdvance::Candidate(candidate) => candidates.push((
                        candidate.target_index,
                        candidate.identity,
                        candidate.row_ids().to_vec(),
                    )),
                    GeometryAdvance::Complete => return (count, candidates),
                    GeometryAdvance::ResourceIncomplete(reason) => panic!("{reason}"),
                }
            }
        })
        .collect()
}

#[cfg(feature = "parallel")]
#[test]
fn family_work_reduction_parallel_preparation_preserves_canonical_shards() {
    let (catalog, fresh) = fixture(4, 0x3c0f_03c0f, false);
    let (_, mut reference) = fixture(4, 0x3c0f_03c0f, false);
    let compiler = reference.compiler.as_mut().unwrap();
    compile::<false>(compiler, &catalog);
    reference.observe_compiler_metrics();
    let compiler = reference.compiler.take().unwrap();
    let (enumerator, count) = compiler.into_enumerator(None);
    reference.enumerator = Some(enumerator);
    reference.candidate_family_count = count;
    let control = ExecutionControl::default();
    let old = reference
        .prepare_parallel(&catalog, &control, 9, |_, requested| requested)
        .expect("reference split");
    let new = fresh
        .prepare_parallel(&catalog, &control, 9, |_, requested| requested)
        .expect("fresh compile and split");
    let old = collect_prepared(old, &catalog);
    let new = collect_prepared(new, &catalog);
    assert!(new.len() > 1, "exercise canonical task partitioning");
    assert_eq!(new, old);
}

#[cfg(feature = "parallel")]
#[test]
fn family_work_reduction_parallel_compile_progress_observes_cancellation() {
    use clearra_core_domain::execution_cancellation::{
        ExecutionCancellationHandle, ExecutionCancellationToken, ExecutionProgress, ProgressSink,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CancelOnProgress {
        handle: ExecutionCancellationHandle,
        reports: AtomicUsize,
    }
    impl ProgressSink for CancelOnProgress {
        fn report(&self, progress: ExecutionProgress) {
            assert_eq!(progress.stage, "geometry");
            assert!(progress.completed > 0);
            assert_eq!(progress.total, None);
            self.reports.fetch_add(1, Ordering::Relaxed);
            self.handle.cancel();
        }
    }

    let (catalog, search) = fixture(4, 0x3c0f_03c0f, false);
    let token = ExecutionCancellationToken::new();
    let sink = Arc::new(CancelOnProgress {
        handle: token.handle(),
        reports: AtomicUsize::new(0),
    });
    let control = ExecutionControl::new(token).with_progress_sink(sink.clone());
    assert!(matches!(
        search.prepare_parallel(&catalog, &control, 9, |_, requested| requested),
        Err(WasmExactSearchError::Cancelled)
    ));
    assert_eq!(sink.reports.load(Ordering::Relaxed), 1);
}
