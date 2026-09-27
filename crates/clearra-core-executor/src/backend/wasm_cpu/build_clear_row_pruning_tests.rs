// SRP rationale: regression evidence for the Build-only inverse clear-row bound.
use std::collections::HashMap;

use crate::WasmBuildProbabilityBackend;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::{
    execution_cancellation::ExecutionControl, solution::StandardBoard64ColoredTilingIdentity,
};
use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;
use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    FinessePatternKnowledge, ProblemCompiler,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::{
    catalog::GeometryCatalog, extended_board::ExtendedBoard,
    extended_inverse_catalog::ExtendedInverseCatalog,
};

fn problem(height: u16) -> clearra_problem::SearchProblem {
    ProblemCompiler::compile_scenario_pc(&PcScenarioQuery::new(
        PcScenarioBoard::standard_10(height, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::J])),
        PieceWindow::new(2),
    ))
    .unwrap()
}

#[test]
fn build_clear_row_pruning_rejects_the_fragmented_colored_image() {
    // Image 1, bottom-left origin: I at y=0,2,3,4; J at (0,1),
    // (1,1),(1,3),(1,4). Neither deleted gap can be a complete row.
    let i = 1 | (1 << 20) | (1 << 30) | (1 << 40);
    let j = (3 << 10) | (2 << 30) | (2 << 40);
    let target = i | j;
    let problem = problem(5);
    let raw = GeometryCatalog::compile_for_required_cells_on_board(&problem, 0, target).unwrap();
    let build =
        GeometryCatalog::compile_for_build_probability_on_board(&problem, 0, target).unwrap();
    for (piece, cells) in [(PieceKind::I, i), (PieceKind::J, j)] {
        assert!((0..raw.skeleton_count() as u32).any(|id| {
            let row = raw.skeleton(id);
            row.piece == piece && row.cells == cells
        }));
        assert!(!(0..build.skeleton_count() as u32).any(|id| {
            let row = build.skeleton(id);
            row.piece == piece && row.cells == cells
        }));
    }
    assert!((0..build.skeleton_count() as u32).all(|id| {
        build
            .realizations(id)
            .iter()
            .all(|realization| realization.required_deleted_rows == 0)
    }));
}

#[test]
fn build_clear_row_pruning_impossible_color_target_completes_in_every_aggregation() {
    let i = 1 | (1 << 20) | (1 << 30) | (1 << 40);
    let j = (3 << 10) | (2 << 30) | (2 << 40);
    let identity =
        StandardBoard64ColoredTilingIdentity::from_piece_masks(0, [i, 0, 0, 0, 0, j, 0]).unwrap();
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(5, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::J])),
        PieceWindow::new(2),
    )
    .with_exact_pieces(Some(2))
    .with_allowed_colored_solution_identities([identity]);
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let field = BuildProbabilityField::from_words(5, [0; 4], [i | j, 0, 0, 0]).unwrap();
    for (aggregation, finesse) in [
        (
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Off,
        ),
        (
            BuildProbabilityAggregation::TilingOnly,
            BuildProbabilityFinesseRequest::Off,
        ),
        (
            BuildProbabilityAggregation::spin_search(SpinProfileSelection::AllSpinPlus),
            BuildProbabilityFinesseRequest::Off,
        ),
        (
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityFinesseRequest::Search {
                pattern_knowledge: FinessePatternKnowledge::Both,
            },
        ),
    ] {
        let result = WasmBuildProbabilityBackend::execute_with_control(
            &problem,
            field,
            aggregation,
            finesse,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(result.bool_field("objective_search_complete"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        assert_eq!(result.field("packing_candidate_count"), Some("0"));
        if !aggregation.is_tiling_only() {
            assert_eq!(result.field("coverage_probability"), Some("0"));
            assert_eq!(result.field("coverage_pattern_count"), Some("1"));
        }
    }
}

#[test]
fn build_clear_row_pruning_extended_impossible_target_completes_with_zero_probability() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(8, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_allow_hold(false);
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let field = BuildProbabilityField::from_words(
        8,
        [0; 4],
        [1 | (1 << 20) | (1 << 40) | (1 << 60), 0, 0, 0],
    )
    .unwrap();
    for aggregation in [
        BuildProbabilityAggregation::Buildability,
        BuildProbabilityAggregation::TilingOnly,
    ] {
        let result = WasmBuildProbabilityBackend::execute_with_control(
            &problem,
            field,
            aggregation,
            BuildProbabilityFinesseRequest::Off,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(result.bool_field("objective_search_complete"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        assert_eq!(result.field("packing_candidate_count"), Some("0"));
        assert_eq!(result.field("unique_solution_count"), Some("0"));
        if !aggregation.is_tiling_only() {
            assert_eq!(result.field("coverage_probability"), Some("0"));
        }
    }
}

fn count_tilings(catalog: &ExtendedInverseCatalog) -> u64 {
    fn visit(
        catalog: &ExtendedInverseCatalog,
        remaining: ExtendedBoard,
        memo: &mut HashMap<ExtendedBoard, u64>,
    ) -> u64 {
        if remaining.is_empty() {
            return 1;
        }
        if let Some(count) = memo.get(&remaining) {
            return *count;
        }
        let options = remaining
            .cells()
            .map(|cell| {
                catalog
                    .support(cell)
                    .iter()
                    .copied()
                    .filter(|id| catalog.skeleton(*id).cells.is_subset_of(remaining))
                    .collect::<Vec<_>>()
            })
            .min_by_key(Vec::len)
            .unwrap();
        let count = options
            .into_iter()
            .map(|id| visit(catalog, remaining.without(catalog.skeleton(id).cells), memo))
            .sum();
        memo.insert(remaining, count);
        count
    }
    visit(catalog, catalog.required_cells(), &mut HashMap::new())
}

#[test]
fn build_clear_row_pruning_image_two_reduces_139876_tilings_to_one() {
    // Eight screenshot rows, bottom to top, x=0..9.
    let rows = [0x201_u16, 0x303, 0x303, 0x201, 0x303, 0x201, 0x201, 0x303];
    let mut target = ExtendedBoard::EMPTY;
    for (y, row) in rows.into_iter().enumerate() {
        for x in 0..10 {
            if row & (1 << x) != 0 {
                target.insert((y * 10 + x) as u16);
            }
        }
    }
    let field = BuildProbabilityField::from_words(8, [0; 4], target.words()).unwrap();
    let raw = ExtendedInverseCatalog::compile_unpruned(field).unwrap();
    let build = ExtendedInverseCatalog::compile(field).unwrap();
    assert_eq!(raw.skeletons().len(), 390);
    assert_eq!(build.skeletons().len(), 34);
    assert_eq!(count_tilings(&raw), 139_876);
    assert_eq!(count_tilings(&build), 1);
}

#[test]
fn build_clear_row_pruning_preserves_full_pc_catalog_and_digest() {
    let problem = problem(4);
    let full = (1_u64 << 40) - 1;
    let raw = GeometryCatalog::compile_for_required_cells_on_board(&problem, 0, full).unwrap();
    let build = GeometryCatalog::compile_for_build_probability_on_board(&problem, 0, full).unwrap();
    assert_eq!(raw.identity_digest(), build.identity_digest());
    assert_eq!(raw.skeleton_count(), build.skeleton_count());
    for id in 0..raw.skeleton_count() as u32 {
        assert_eq!(raw.skeleton(id), build.skeleton(id));
        assert_eq!(raw.realizations(id), build.realizations(id));
    }
}

#[test]
fn build_clear_row_pruning_retains_only_realizations_with_completable_gaps() {
    let problem = problem(6);
    // Logical rows 1 and 3 can clear; row 2 cannot. Piece cells may still
    // be fragmented across either full row, independent of color adjacency.
    let full = 1023_u64;
    let target = 1 | (1 << 20) | (1 << 40) | (1 << 50);
    let base = (full << 10) | (full << 30);
    let raw = GeometryCatalog::compile_for_required_cells_on_board(&problem, base, target).unwrap();
    let build =
        GeometryCatalog::compile_for_build_probability_on_board(&problem, base, target).unwrap();
    let allowed = (1 << 1) | (1 << 3);
    let expected: Vec<_> = (0..raw.skeleton_count() as u32)
        .filter_map(|id| {
            let row = raw.skeleton(id);
            let realizations: Vec<_> = raw
                .realizations(id)
                .iter()
                .copied()
                .filter(|r| r.required_deleted_rows & !allowed == 0)
                .collect();
            (!realizations.is_empty()).then_some((row.piece, row.cells, realizations))
        })
        .collect();
    let actual: Vec<_> = (0..build.skeleton_count() as u32)
        .map(|id| {
            let row = build.skeleton(id);
            (row.piece, row.cells, build.realizations(id).to_vec())
        })
        .collect();
    assert_eq!(actual, expected);
    assert!(
        !actual.is_empty(),
        "a valid fragmented I spans two completable rows"
    );
}
