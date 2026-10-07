use super::*;
use crate::backend::wasm_cpu::{extended_geometry::ExtendedGeometryAdvance, piece_index};
use clearra_core_domain::{board::standard_pc_board::Board256Mask, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::{BuildProbabilityField, ProblemCompiler, SearchProblem};
use clearra_supply::queue::fixed_sequence::FixedSequence;

fn fixture(
    height: u8,
) -> (
    SearchProblem,
    ExtendedInverseCatalog,
    ExtendedGeometrySearch,
) {
    let mut holes = Board256Mask::EMPTY;
    for y in 0..4 {
        for x in 0..4 {
            holes = holes.union(Board256Mask::singleton(y * 10 + x).unwrap());
        }
    }
    let tail_start = if height == 7 { 3 } else { 4 };
    for y in tail_start..u16::from(height) {
        let column = 5 + (y - tail_start) / 4;
        holes = holes.union(Board256Mask::singleton(y * 10 + column).unwrap());
    }
    let pieces = holes.count_ones() as usize / 4;
    let initial = Board256Mask::all_cells(u16::from(height) * 10)
        .unwrap()
        .without(holes);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
        PieceWindow::new(pieces),
    )
    .with_exact_pieces(Some(pieces))
    .with_allow_hold(false)
    .with_min_remaining_queue(0)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_execution_policy(PcExecutionPolicy::default().with_workers(1));
    let problem = ProblemCompiler::compile_scenario_pc_tiling(&query).unwrap();
    let field =
        BuildProbabilityField::from_words_preserving_height(height, initial.words(), holes.words())
            .unwrap();
    let catalog = ExtendedInverseCatalog::compile_bounded(field, 100_000).unwrap();
    let universe = problem.piece_source().materialized_universe().unwrap();
    let family = universe.packing_multiset_family_for_execution(
        pieces,
        problem.initial_hold(),
        false,
        crate::backend::wasm_cpu::packing_hold_projection(&problem),
    );
    let mut geometry = ExtendedGeometrySearch::new(universe, &family, &catalog).unwrap();
    for _ in 0..100_000 {
        if !geometry.is_compiling() {
            break;
        }
        assert!(matches!(
            geometry.advance(&catalog),
            ExtendedGeometryAdvance::Pending
        ));
    }
    assert!(
        !geometry.is_compiling(),
        "the two-solution fixture is bounded, not a large empty-height run"
    );
    (problem, catalog, geometry)
}

fn rows(
    mut enumerator: ExtendedFamilyEnumerator,
    catalog: &ExtendedInverseCatalog,
) -> Vec<Vec<u32>> {
    let mut rows = Vec::new();
    while let Some(candidate) = enumerator.next_candidate(catalog).unwrap() {
        rows.push(candidate.row_ids().to_vec());
    }
    rows
}

#[test]
fn extended_parallel_real_family_preserves_disjoint_ordered_intervals_and_shared_owners() {
    for height in [7, 8, 12, 24] {
        for partitions in [1, 2, 11] {
            let (problem, catalog, mut geometry) = fixture(height);
            let enumerator = geometry.enumerator.as_ref().unwrap();
            let serial = rows(
                ExtendedFamilyEnumerator {
                    targets: Arc::clone(&enumerator.targets),
                    family: Arc::clone(&enumerator.family),
                    tasks: enumerator.tasks.clone(),
                    rows: enumerator.rows,
                    target_depth: enumerator.target_depth,
                    task_memory_limit: None,
                    refused_task_bytes: None,
                },
                &catalog,
            );
            assert_eq!(serial.len(), 2);
            let bound = ExecutionMemoryBound::unbounded_for_problem(&problem).unwrap();
            let live = geometry.retained_bytes() as u128 + catalog.retained_bytes() as u128;
            let mut plan = geometry
                .take_parallel_plan(partitions, bound, live)
                .unwrap()
                .unwrap();
            assert_eq!(plan.branches.len(), partitions.min(2));
            let shared_bytes = plan.shared_retained_bytes();
            let mut actual = Vec::new();
            let mut ordinal = 0_u128;
            for branch in &mut plan.branches {
                assert!(Arc::ptr_eq(&branch.enumerator.family, &plan.family));
                assert!(Arc::ptr_eq(&branch.enumerator.targets, &plan.targets));
                assert_eq!(branch.first_ordinal, ordinal);
                let before = actual.len();
                while let Some(candidate) = branch.next_candidate(&catalog, bound, 0).unwrap() {
                    actual.push(candidate.row_ids().to_vec());
                }
                assert_eq!((actual.len() - before) as u128, branch.candidate_count);
                ordinal += branch.candidate_count;
            }
            assert_eq!(actual, serial);
            assert_eq!(plan.shared_retained_bytes(), shared_bytes);
            geometry
                .complete_parallel_enumeration(actual.len())
                .unwrap();
            assert_eq!(geometry.candidate_count(), 2);
        }
    }
}

#[test]
fn extended_parallel_product_splits_preserve_sixty_row_prefix_and_continuations() {
    let (problem, catalog, mut geometry) = fixture(24);
    let mut target = geometry.enumerator.as_ref().unwrap().targets[0].clone();
    target.counts = [0; 7];
    target.counts[piece_index(PieceKind::I)] = 60;
    let ids = catalog
        .skeletons()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.piece == PieceKind::I)
        .take(2)
        .map(|(id, _)| id as u32)
        .collect::<Vec<_>>();
    // This is a traversal-capacity oracle, not a claim that repeated catalog
    // rows constitute a legal PC. Physical exact-cover parity is tested above.
    let mut family = GeometrySolutionFamily::new();
    let a = family.append(ids[0], FAMILY_EMPTY).unwrap();
    let a = family.append(ids[0], a).unwrap();
    let b = family.append(ids[1], FAMILY_EMPTY).unwrap();
    let b = family.append(ids[1], b).unwrap();
    let union = family.union(a, b).unwrap();
    let pair = family.product(union, union).unwrap();
    let mut root = family.product(pair, pair).unwrap();
    for _ in 0..52 {
        root = family.append(ids[0], root).unwrap();
    }
    geometry.enumerator = Some(ExtendedFamilyEnumerator::new(
        vec![target],
        family,
        root,
        60,
    ));
    geometry.candidate_family_count = Some(16);
    let enumerator = geometry.enumerator.as_ref().unwrap();
    let serial = rows(
        ExtendedFamilyEnumerator {
            targets: Arc::clone(&enumerator.targets),
            family: Arc::clone(&enumerator.family),
            tasks: enumerator.tasks.clone(),
            rows: enumerator.rows,
            target_depth: 60,
            task_memory_limit: None,
            refused_task_bytes: None,
        },
        &catalog,
    );
    assert_eq!(serial.len(), 16);
    let bound = ExecutionMemoryBound::unbounded_for_problem(&problem).unwrap();
    let mut plan = geometry.take_parallel_plan(11, bound, 0).unwrap().unwrap();
    assert_eq!(plan.branches.len(), 11);
    let mut actual = Vec::new();
    let mut ordinal = 0_u128;
    for branch in &mut plan.branches {
        assert_eq!(branch.first_ordinal, ordinal);
        let before = actual.len();
        while let Some(candidate) = branch.next_candidate(&catalog, bound, 0).unwrap() {
            assert_eq!(candidate.row_ids().len(), 60);
            assert_eq!(&candidate.row_ids()[..52], vec![ids[0]; 52].as_slice());
            actual.push(candidate.row_ids().to_vec());
        }
        assert_eq!((actual.len() - before) as u128, branch.candidate_count);
        ordinal += branch.candidate_count;
    }
    assert_eq!(ordinal, 16);
    assert_eq!(actual, serial);
}

#[test]
fn extended_parallel_plan_refuses_memory_before_transfer_and_preserves_serial_state() {
    let (problem, catalog, mut geometry) = fixture(8);
    let bound = ExecutionMemoryBound::unbounded_for_problem(&problem)
        .unwrap()
        .with_cap(1)
        .unwrap();
    assert!(matches!(
        geometry.take_parallel_plan(11, bound, 0),
        Err(WasmExactSearchError::ResourceAdmission(_))
    ));
    assert!(geometry.enumerator.is_some());
    assert_eq!(geometry.candidate_count(), 0);
    let mut count = 0;
    loop {
        match geometry.advance(&catalog) {
            ExtendedGeometryAdvance::Candidate(_) => count += 1,
            ExtendedGeometryAdvance::Complete => break,
            _ => panic!("the completed exact family remains serially consumable after refusal"),
        }
    }
    assert_eq!(count, 2);
}

#[test]
fn extended_parallel_plan_refuses_a_consumed_root_and_private_traversal_overcommit() {
    let (problem, catalog, mut geometry) = fixture(8);
    assert!(matches!(
        geometry.advance(&catalog),
        ExtendedGeometryAdvance::Candidate(_)
    ));
    let bound = ExecutionMemoryBound::unbounded_for_problem(&problem).unwrap();
    assert!(geometry.take_parallel_plan(2, bound, 0).is_err());
    let (_, _, mut unconsumed) = fixture(8);
    let mut plan = unconsumed.take_parallel_plan(1, bound, 0).unwrap().unwrap();
    let tiny = bound.with_cap(1).unwrap();
    assert!(matches!(
        plan.branches[0].next_candidate(&catalog, tiny, 0),
        Err(WasmExactSearchError::ResourceAdmission(_))
    ));
}
