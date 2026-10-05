//! A one-placement smoke of the existing four-word ILC/BuildUp engine.
//! Full-height PC input/area/identity contracts are checked separately by
//! `extended_pc_execution`. This smoke is not a complete 24L PC enumeration,
//! performance gate, or qualification of every public PC reducer.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_core_executor::WasmBuildProbabilityBackend;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    WorkerPolicy,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    ExtendedPcSearchContract, ProblemCompiler,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

#[test]
fn one_placement_in_a_twenty_four_line_field_uses_all_words_in_existing_ilc() {
    let height = 24_u8;
    // One support cell below a vertical I, plus an unrelated uppermost cell.
    // Neither row is full, so initial line clear cannot remove the high words.
    // The I has an independent rotate-left-drop witness onto its support.
    let base = Board256Mask::singleton(19 * 10)
        .unwrap()
        .union(Board256Mask::singleton(23 * 10 + 9).unwrap());
    let mut target = Board256Mask::EMPTY;
    for row in 20..24 {
        target = target.union(Board256Mask::singleton(row * 10).unwrap());
    }
    assert_ne!(base.words()[3], 0);
    assert_ne!(target.words()[3], 0);
    let field =
        BuildProbabilityField::from_words_preserving_height(height, base.words(), target.words())
            .unwrap();
    assert_eq!(field.height(), 24);
    assert_eq!(field.target_piece_count(), 1);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(u16::from(height), base.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_allow_hold(false)
    .with_min_remaining_queue(0)
    .with_execution_policy(PcExecutionPolicy::default().with_worker_policy(WorkerPolicy::Fixed(1)));
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    assert_eq!(problem.initial_board().occupied_words(), base.words());
    let result = WasmBuildProbabilityBackend::execute_with_control(
        &problem,
        field,
        BuildProbabilityAggregation::Buildability,
        BuildProbabilityFinesseRequest::Off,
        &ExecutionControl::default(),
    )
    .unwrap();
    assert_eq!(
        result.bool_field("count_complete"),
        Some(true),
        "{result:?}"
    );
    assert_eq!(
        result.bool_field("probability_complete"),
        Some(true),
        "{result:?}"
    );
    assert_eq!(
        result.field("coverage_probability"),
        Some("1"),
        "{result:?}"
    );
    assert_eq!(result.usize_field("target_piece_count"), Some(1));
    assert_eq!(result.usize_field("unique_solution_count"), Some(1));
    assert_eq!(result.normalized_solution_keys().len(), 1);
    let words = base.words();
    let identity = format!(
        "ctk2|height={height}|initial={:016x}{:016x}{:016x}{:016x}|placements=",
        words[3], words[2], words[1], words[0]
    );
    assert!(result.normalized_solution_keys()[0].starts_with(&identity));
    assert_eq!(result.path_steps().len(), 1);
    assert_eq!(result.path_steps()[0].cleared_lines(), 0);
}

fn board_hex(words: [u64; 4]) -> String {
    format!(
        "{:016x}{:016x}{:016x}{:016x}",
        words[3], words[2], words[1], words[0]
    )
}

/// Different columns for the four-cell wells prevent a large partition family
/// inside one long I well. The 24L case has six forced placements, not an empty
/// 60-piece search. In 7L the two I footprints share a row but never a cell.
fn forced_i_pc(height: u8) -> (Board256Mask, Vec<[u64; 4]>) {
    let mut holes = Board256Mask::EMPTY;
    let mut placements = Vec::new();
    let starts: Vec<u16> = if height == 7 {
        vec![0, 3]
    } else {
        (0..u16::from(height)).step_by(4).collect()
    };
    for (column, start) in starts.into_iter().enumerate() {
        let mut cells = Board256Mask::EMPTY;
        for row in start..start + 4 {
            let bit = row * 10 + u16::try_from(column).unwrap();
            cells = cells.union(Board256Mask::singleton(bit).unwrap());
        }
        holes = holes.union(cells);
        placements.push(cells.words());
    }
    let full = Board256Mask::all_cells(u16::from(height) * 10).unwrap();
    (full.without(holes), placements)
}

#[test]
fn extended_pc_compiler_and_existing_ilc_complete_small_forced_pcs_in_every_profile() {
    use clearra_rules::profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x};

    for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
        for height in [7_u8, 8, 12, 24] {
            let (initial, mut placements) = forced_i_pc(height);
            let pieces = placements.len();
            assert!(pieces <= 6);
            assert!((0..u16::from(height))
                .all(|row| { !(0..10).all(|x| initial.contains_index(row * 10 + x)) }));
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10_from_words(u16::from(height), initial.words())
                    .unwrap(),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; pieces])),
                PieceWindow::new(pieces),
            )
            .with_rule(rule)
            .with_exact_pieces(Some(pieces))
            .with_allow_hold(false)
            .with_min_remaining_queue(0)
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_execution_policy(
                PcExecutionPolicy::default().with_worker_policy(WorkerPolicy::Fixed(1)),
            );
            let execution = ExtendedPcSearchContract::compile_standard_query(query, height)
                .unwrap()
                .execution_problem()
                .unwrap();
            let result = WasmBuildProbabilityBackend::execute_with_control(
                execution.problem(),
                execution.field(),
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Off,
                &ExecutionControl::default(),
            )
            .unwrap();
            assert_eq!(
                result.bool_field("count_complete"),
                Some(true),
                "{result:?}"
            );
            assert_eq!(result.bool_field("probability_complete"), Some(true));
            assert_eq!(result.field("coverage_probability"), Some("1"));
            assert_eq!(result.usize_field("workers_requested"), Some(1));
            assert_eq!(result.usize_field("workers_used"), Some(1));
            assert_eq!(result.usize_field("target_piece_count"), Some(pieces));
            assert_eq!(result.usize_field("unique_solution_count"), Some(1));
            assert_eq!(result.field("build_final_board_mask"), Some("0x0"));
            assert_eq!(
                result.field("completed_target_rows"),
                Some(format!("0x{:x}", (1_u32 << height) - 1).as_str())
            );
            assert_eq!(result.path_steps().len(), pieces);
            assert_eq!(
                result
                    .path_steps()
                    .iter()
                    .map(|step| u16::from(step.cleared_lines()))
                    .sum::<u16>(),
                u16::from(height)
            );
            // Assert the entire four-word family, not just a count/low mask.
            placements.sort_unstable();
            let placements = placements
                .into_iter()
                .map(|words| format!("I:{}", board_hex(words)))
                .collect::<Vec<_>>()
                .join(",");
            let expected = format!(
                "ctk2|height={height}|initial={}|placements={placements}",
                board_hex(initial.words())
            );
            assert_eq!(result.normalized_solution_keys(), [expected.as_str()]);
            let identity =
                clearra_core_domain::solution::ExtendedTilingSolutionKey::parse_canonical(
                    &result.normalized_solution_keys()[0],
                )
                .unwrap();
            assert_eq!(identity.height(), height);
            assert_eq!(identity.initial_board(), initial);
            assert_eq!(identity.placement_count(), pieces);
            // Existing Build family evidence is deliberately not relabelled
            // as public PC CountAll, minimum, score or distributed authority.
            assert_eq!(
                result.bool_field("build_path_multiplicity_counted"),
                Some(false)
            );
        }
    }
}
