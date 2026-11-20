#![cfg(feature = "native-c-core")]

use clearra_core_domain::{
    board::board_size::BoardSize,
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
    pruning::PruningEvidencePolicy,
    solution::{
        NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64ColoredTilingIdentity,
        StandardBoard64TilingIdentity,
    },
};
use clearra_core_executor::WasmCpuSearchBackend;
use clearra_core_ffi::{
    problem::{C_PIECE_I, C_PIECE_J, C_PIECE_L, C_PIECE_O, C_PIECE_S, C_PIECE_T, C_PIECE_Z},
    CBuildUpProblemTemplate, CNativeBuildUpEnumerationLimits, CPackingProblemBuilder, CoreCNative,
    NativeBuildUpWorkspace, NativeCandidateReducer, C_BUILDUP_STATUS_OK,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow,
};
use clearra_problem::{BuildProblemLimits, BuildQuery, BuildTemplateBridge, ProblemCompiler};
use clearra_supply::{queue::fixed_sequence::FixedSequence, PieceMultisetKey};

const LEGACY_PATTERN_ID: u32 = 0;
const PIECES: [PieceKind; 10] = [
    PieceKind::I,
    PieceKind::O,
    PieceKind::T,
    PieceKind::S,
    PieceKind::Z,
    PieceKind::J,
    PieceKind::L,
    PieceKind::I,
    PieceKind::O,
    PieceKind::T,
];
// Two independent hard-drop witnesses in original-row coordinates. In both,
// locks 6, 9 and 10 clear original rows [1], [3] and [0, 2].
const WITNESSES: [[u64; 10]; 2] = [
    [
        0x78,
        0xc0300,
        0x100e000,
        0x1803,
        0x6030000,
        0xc0100400,
        0x3008000080,
        0xf00000000,
        0xc030000000,
        0xe00004,
    ],
    [
        0x1e0,
        0xc03,
        0x4038000,
        0x600c,
        0x180c0000,
        0x300401000,
        0xc020000200,
        0x3c00000000,
        0xc0300000,
        0x3800010,
    ],
];

fn c_piece(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => C_PIECE_I,
        PieceKind::O => C_PIECE_O,
        PieceKind::T => C_PIECE_T,
        PieceKind::S => C_PIECE_S,
        PieceKind::Z => C_PIECE_Z,
        PieceKind::J => C_PIECE_J,
        PieceKind::L => C_PIECE_L,
    }
}

fn exact_identity(masks: [u64; 10]) -> StandardBoard64TilingIdentity {
    StandardBoard64TilingIdentity::from_placements(
        0,
        PIECES
            .into_iter()
            .zip(masks)
            .map(|(piece, mask)| PiecePlacementMask::new(piece, mask)),
    )
    .expect("disjoint tetromino placements in the original rows")
}

fn coarse_colored_filter(masks: [u64; 10]) -> StandardBoard64ColoredTilingIdentity {
    let mut piece_masks = [0; 7];
    for (piece, mask) in PIECES.into_iter().zip(masks) {
        let index = PieceKind::STANDARD_TETROMINOES
            .iter()
            .position(|&kind| kind == piece)
            .expect("Rust standard piece index");
        piece_masks[index] |= mask;
    }
    StandardBoard64ColoredTilingIdentity::from_piece_masks(0, piece_masks)
        .expect("valid coarse filter for the two Rust witnesses")
}

#[test]
fn native_and_rust_join_two_legacy_four_line_candidates_on_original_ordinal() {
    let cancellation = ExecutionCancellationToken::new();
    let board = BoardSize::new(10, 4).expect("standard 10×4 target");
    let bridge = BuildTemplateBridge::new("legacy-two-candidate-probe", board, PIECES.len());
    let legacy = ProblemCompiler::compile_build(&BuildQuery::coverage_bridge(
        bridge,
        1,
        BuildProblemLimits::new(1024, 4096),
    ))
    .expect("canonical legacy cover request");
    let universe = legacy
        .piece_source()
        .materialized_universe()
        .expect("canonical observed supply");
    assert_eq!(
        universe.sequence_at(LEGACY_PATTERN_ID as usize).as_ref(),
        &PIECES
    );
    assert_eq!(universe.pattern_count(), 4096);
    assert!(!legacy.piece_source().complete());
    assert!(!legacy.supply().hold_enabled());

    // Limit the native producer to two known skeleton lists. Do not enumerate
    // all of the legacy Geometry family or rewrite the legacy observed source
    // as a fixed C queue: those are different scopes and can admit differently.
    let compact = CPackingProblemBuilder::from_search_problem_with_piece_multiset(
        &legacy,
        PieceMultisetKey::from_pieces(PIECES),
    )
    .expect("native ordinal-zero Geometry multiset");
    let catalog = CoreCNative::compile_geometry_catalog_with_cancellation(&compact, &cancellation)
        .expect("native ILC catalog for the unmodified legacy request");
    let view = catalog.view();
    let template = CBuildUpProblemTemplate::compile(&legacy).expect("native legacy scratch");
    let mut scratch = template.new_scratch();
    template
        .configure_piece_source_pattern(&mut scratch, LEGACY_PATTERN_ID)
        .expect("bind original observed ordinal directly in native scratch");
    template.attach_geometry_catalog(&mut scratch, &catalog);
    let mut workspace = NativeBuildUpWorkspace::new();
    let mut sink = NativeCandidateReducer::new(&compact).expect("canonical candidate sink");
    for witness in WITNESSES {
        let skeleton_rows = PIECES
            .into_iter()
            .zip(witness)
            .map(|(piece, mask)| {
                let row = view
                    .skeleton_cell_masks()
                    .iter()
                    .zip(view.skeleton_piece_kinds())
                    .position(|(&cells, &kind)| cells == mask && kind == u32::from(c_piece(piece)))
                    .expect("native catalog must carry each witnessed concrete placement");
                u32::try_from(row).expect("native catalog row fits u32")
            })
            .collect::<Vec<_>>();
        let outcome = catalog
            .stream_buildable_rows(
                &skeleton_rows,
                &compact,
                &mut scratch,
                &mut workspace,
                &cancellation,
                PruningEvidencePolicy::BestEffort,
                &mut sink,
            )
            .expect("native candidate admission for the requested ordinal");
        assert_eq!(outcome.report.buildup_status, C_BUILDUP_STATUS_OK);
        assert_eq!(outcome.status, 0, "native packing result");
        assert_ne!(outcome.report.complete, 0);
        assert_eq!(outcome.report.generated_count, 1);
        assert_eq!(outcome.report.buildable_count, 1);
    }
    let native_candidates = sink.into_candidates();
    assert_eq!(native_candidates.len(), 2);

    // Rust is deliberately bounded to this one ordinal and the two coarse
    // colored regions. Full placement identity remains the equality authority.
    let rust_problem = ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(PIECES.to_vec())),
            PieceWindow::new(PIECES.len()),
        )
        .with_rule(legacy.rule_profile_value())
        .with_allow_hold(false)
        .with_exact_pieces(Some(PIECES.len()))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_allowed_colored_solution_identities(WITNESSES.map(coarse_colored_filter))
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1)),
    )
    .expect("independent fixed-ordinal Rust reference");
    let rust = WasmCpuSearchBackend::execute_with_control(
        &rust_problem,
        &ExecutionControl::new(cancellation.clone()),
    )
    .expect("compact Rust provider on the exact ordinal");
    assert_eq!(rust.bool_field("count_complete"), Some(true));
    assert_eq!(rust.bool_field("solution_probability_complete"), Some(true));
    assert_eq!(rust.usize_field("coverage_pattern_count"), Some(1));

    let expected = WITNESSES.map(exact_identity);
    assert_ne!(expected[0], expected[1]);
    let mut confirmed = [false; 2];
    for candidate in native_candidates.iter() {
        let identity = candidate
            .standard_board64_tiling_identity(0)
            .expect("native candidate has a concrete original-row identity");
        let index = expected
            .iter()
            .position(|&wanted| wanted == identity)
            .expect("native row matches one exact independent witness");
        assert!(!confirmed[index], "duplicate native witness identity");

        let mut scratch = template.new_scratch();
        template.attach_geometry_catalog(&mut scratch, &catalog);
        template
            .configure_packing_candidate(
                &mut scratch,
                &candidate,
                LEGACY_PATTERN_ID,
                LEGACY_PATTERN_ID,
            )
            .expect("bind exact native candidate ID to the same original queue ordinal");
        let native = workspace
            .enumerate_buildup_variants_with_cancellation(
                &scratch,
                &CNativeBuildUpEnumerationLimits {
                    max_variants: 1,
                    ..Default::default()
                },
                &cancellation,
            )
            .expect("native exact BuildUp, not the legacy assignment join");
        assert_eq!(native.status, C_BUILDUP_STATUS_OK);
        assert_ne!(native.buffer.count_complete, 0);
        assert!(native.buffer.total_variant_count > 0);
        assert_eq!(native.buffer.count, 1);
        let witness = &native.buffer.variants[0];
        assert_eq!(witness.candidate_id, candidate.candidate_id);
        assert_eq!(witness.coverage_pattern_id, LEGACY_PATTERN_ID);
        assert_eq!(witness.placed_count, 10);
        assert_eq!(witness.cleared_lines, 4);
        assert_eq!(witness.final_board, 0);

        let normalized = NormalizedTilingSolutionKey::from_standard_board64_identity(identity);
        let coverage = rust
            .normalized_solution_coverages()
            .iter()
            .find(|row| row.solution_key() == normalized.as_str())
            .expect("join Rust coverage by exact identity, not candidate position");
        assert_eq!(coverage.covered_patterns().pattern_count(), 1);
        assert_eq!(coverage.covered_patterns().word_at(0), 1);
        confirmed[index] = true;
    }
    assert_eq!(confirmed, [true, true]);
}
