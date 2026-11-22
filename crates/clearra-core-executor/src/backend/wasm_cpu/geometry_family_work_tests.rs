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
