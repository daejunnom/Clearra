#![cfg(feature = "native-c-core")]

use clearra_build_coverage::{
    coverage::{
        BuildCoverageExecution, BuildCoverageExecutionError, ConcreteAssignmentCoverageRow,
        ConcreteCoverageCompleteness, ConcreteCoverageScope,
    },
    domain::slot_domain::SlotDomain,
    query::{build_coverage_limits::BuildCoverageLimits, build_coverage_query::BuildCoverageQuery},
    template::{BuildSlot, BuildSlotId, BuildTemplate},
};
use clearra_core_domain::{
    board::{board_size::BoardSize, cell::CellCoord},
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
    pruning::PruningEvidencePolicy,
    solution::{
        NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64ColoredTilingIdentity,
        StandardBoard64TilingIdentity,
    },
};
use clearra_core_executor::{
    packing::PackingRunner, BuildUpRunner, CoreExecutor, WasmCpuSearchBackend,
};
use clearra_core_ffi::{
    problem::{C_PIECE_I, C_PIECE_J, C_PIECE_L, C_PIECE_O, C_PIECE_S, C_PIECE_T, C_PIECE_Z},
    CBuildUpProblemTemplate, CNativeBuildUpEnumerationLimits, CPackingProblemBuilder, CoreCNative,
    NativeBuildUpWorkspace, NativeCandidateReducer, PackingCandidateBatch,
    C_BUILDUP_STATUS_LOGICAL_REJECT_MAX, C_BUILDUP_STATUS_LOGICAL_REJECT_MIN, C_BUILDUP_STATUS_OK,
};
use clearra_coverage::{
    pattern::{
        pattern_bitset::PatternBitSet,
        weighted_pattern_set::{covered_weight_in_pattern_order, WeightedPatternSet},
    },
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow,
};
use clearra_problem::{
    BuildProblemLimits, BuildQuery, BuildTemplateBridge, ProblemCompiler, ScenarioQuery,
    SearchProblem, SearchProblemPreset,
};
use clearra_supply::{
    piece_source::PieceSourceKind,
    queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
    PieceMultisetKey,
};

const LEGACY_PATTERN_ID: u32 = 0;

fn exact_o_cover_query(board: BoardSize, pattern_count: usize) -> BuildCoverageQuery {
    let (slots, domains) = (0_u16..5)
        .map(|index| {
            let id = BuildSlotId::new(u32::from(index));
            let x = index * 2;
            let cells = [(x, 0), (x + 1, 0), (x, 1), (x + 1, 1)]
                .into_iter()
                .map(|(x, y)| CellCoord::new_unchecked(x, y))
                .collect();
            (
                BuildSlot::new(id, cells).with_allowed_pieces(vec![PieceKind::O]),
                SlotDomain::new(id, vec![PieceKind::O]),
            )
        })
        .unzip();
    BuildCoverageQuery::new(
        BuildTemplate::new("legacy-complete-two-line-o-cover", slots).with_board_size(board),
        domains,
        Vec::new(),
        pattern_count,
        BuildCoverageLimits::new(64, 8),
    )
}

#[test]
fn native_and_rust_agree_on_complete_bounded_legacy_fixed_and_finite_sources() {
    let control = ExecutionControl::default();
    let board = BoardSize::new(10, 2).expect("two-line standard field");
    let all_o = [PieceKind::O; 5];
    let mut last_t = all_o;
    last_t[4] = PieceKind::T;
    // Five O pieces force one exact tiling. One T and four O cannot tile the
    // two rows: the only odd-color-imbalance piece is T. A clear before T
    // cannot help, because T would have only one original row left.
    let expected_assignment = StandardBoard64TilingIdentity::from_placements(
        0,
        (0..5).map(|index| PiecePlacementMask::new(PieceKind::O, 0xc03_u64 << (2 * index))),
    )
    .expect("unique exact O placement family");
    let cases = [
        (
            "fixed positive",
            PcQueueInput::fixed_sequence(FixedSequence::new(all_o.to_vec())),
            1,
            1,
        ),
        (
            "fixed negative",
            PcQueueInput::fixed_sequence(FixedSequence::new(last_t.to_vec())),
            1,
            0,
        ),
        (
            "finite positive and negative ordinals",
            PcQueueInput::pattern_expression(
                QueuePatternExpression::parse("OOOO[OT]", 8).expect("two explicit queues"),
            ),
            2,
            1,
        ),
    ];
    for (case, queue, pattern_count, expected_hits) in cases {
        let query = exact_o_cover_query(board, pattern_count);
        let build = BuildQuery::coverage_bridge(
            BuildTemplateBridge::new(query.template().id(), board, 5),
            pattern_count,
            BuildProblemLimits::new(
                query.limits().max_assignments(),
                query.limits().max_patterns(),
            ),
        );
        // Exercise the public typed Core Build entry with its real complete
        // fixed/finite queue. The CLI/App Legacy default still supplies an
        // Observed queue; this is not a claim about that default route.
        let source_query =
            PcScenarioQuery::new(PcScenarioBoard::standard_10(2, 0), queue, PieceWindow::new(5))
                .with_allow_hold(false)
                .with_exact_pieces(Some(5))
                .with_count_policy(PcCountPolicy::CountAll)
                .with_retained_trace_limit(1)
                .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1));
        let legacy = SearchProblem::new(
            SearchProblemPreset::Build,
            ScenarioQuery::build_preset(source_query, build),
        )
        .expect("typed Legacy Core request");
        let source = legacy.piece_source();
        let universe = source
            .materialized_universe()
            .expect("original ordinal authority");
        assert_eq!(
            source.kind(),
            if pattern_count == 1 {
                PieceSourceKind::FixedQueue
            } else {
                PieceSourceKind::MaterializedPatternUniverse
            },
            "{case}",
        );
        assert!(source.complete() && universe.complete(), "{case}");
        assert_eq!(universe.truncation_reason(), None, "{case}");
        assert_eq!(universe.pattern_count(), pattern_count, "{case}");
        assert_eq!(
            universe.total_possible_pattern_count(),
            pattern_count as u128,
            "{case}"
        );

        // Keep only the scalar witness. In particular, PackingRunResult's
        // admission and retained graph must be gone before either backend
        // opens its next request under the same process authority.
        let native_word = {
            let packing = PackingRunner::run_with_control(&legacy, &control)
                .expect("existing native Legacy packing must admit the whole complete source");
            assert!(packing.count_complete(), "{case}");
            assert_eq!(packing.candidate_count(), expected_hits, "{case}");
            let build = BuildUpRunner::run_with_control(&legacy, &packing, &control)
                .expect("existing native Legacy BuildUp must finish");
            assert!(build.count_complete(), "{case}");
            assert!(build.materialized_coverage_complete(), "{case}");
            assert_eq!(build.coverage_rows().len(), expected_hits, "{case}");
            if expected_hits == 0 {
                0
            } else {
                let row = &build.coverage_rows()[0];
                let candidate = packing.candidate_view_at(0).expect("the one physical candidate");
                assert_eq!(
                    candidate.standard_board64_tiling_identity(0).unwrap(),
                    expected_assignment
                );
                assert_eq!(row.candidate_id(), candidate.candidate_id());
                assert_eq!(row.piece_source_id(), source.id().get());
                assert_eq!(row.pattern_count(), pattern_count);
                row.coverage_bits().word_at(0)
            }
        };

        let mut oracle_word = 0_u64;
        let mut rust_word = 0_u64;
        for ordinal in 0..pattern_count {
            let actual_source = universe.sequence_at(ordinal);
            let expected_hit = actual_source.as_ref() == &all_o;
            assert!(
                expected_hit || actual_source.as_ref() == &last_t,
                "{case}: foreign ordinal",
            );
            if expected_hit {
                oracle_word |= 1 << ordinal;
            }
            let rust = ProblemCompiler::compile_scenario_pc(
                &PcScenarioQuery::new(
                    PcScenarioBoard::standard_10(2, 0),
                    PcQueueInput::fixed_sequence(FixedSequence::new(actual_source.into_owned())),
                    PieceWindow::new(5),
                )
                .with_rule(legacy.rule_profile_value())
                .with_allow_hold(false)
                .with_exact_pieces(Some(5))
                .with_count_policy(PcCountPolicy::CountAll)
                .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
                .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1)),
            )
            .expect("one original ordinal, independently executed by Rust");
            let result = WasmCpuSearchBackend::execute_with_control(&rust, &control)
                .expect("Rust physical execution must finish");
            for field in [
                "count_complete",
                "build_variant_count_exact",
                "probability_complete",
                "solution_probability_complete",
            ] {
                assert_eq!(
                    result.bool_field(field), Some(true),
                    "{case}: {ordinal}, {field}",
                );
            }
            assert_eq!(result.bool_field("resource_truncated"), Some(false));
            assert_eq!(result.usize_field("coverage_pattern_count"), Some(1));
            assert_eq!(
                result.solution_coverages().len(),
                usize::from(expected_hit),
                "{case}: ordinal {ordinal}",
            );
            if let Some(row) = result.solution_coverages().first() {
                assert_eq!(row.identity(), expected_assignment);
                assert_eq!(row.covered_patterns().pattern_count(), 1);
                assert_eq!(row.covered_patterns().word_at(0), 1);
                rust_word |= 1 << ordinal;
            }
        }
        assert_eq!(oracle_word.count_ones() as usize, expected_hits, "{case}");
        assert_eq!(native_word, oracle_word, "{case}: original Native ordinals");
        assert_eq!(rust_word, oracle_word, "{case}: original Rust ordinals");

        let scope = ConcreteCoverageScope::new_uniform(
            board,
            source.id().get(),
            universe.pattern_universe_id(),
            universe.pattern_weight_model_id(),
            0x4c45_4741_4359_0002,
        );
        let bits = PatternBitSet::from_words(pattern_count, vec![rust_word]).unwrap();
        let covered = BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope,
            vec![ConcreteAssignmentCoverageRow::new(
                expected_assignment,
                scope,
                bits.clone(),
                ConcreteCoverageCompleteness::Complete,
            )],
        )
        .expect("exact concrete Rust row, including an explicit zero");
        assert!(covered.exact_cover_complete(), "{case}");
        assert_eq!(covered.union().covered_patterns(), &bits, "{case}");
        let product = CoreExecutor::execute_build_coverage_with_control(&legacy, &query, &control)
            .expect("real Legacy Core projection on the admitted request");
        assert_eq!(product.bool_field("count_complete"), Some(true), "{case}");
        assert_eq!(
            product.bool_field("probability_complete"), Some(true),
            "{case}"
        );
        assert_eq!(
            product.bool_field("resource_truncated"), Some(false),
            "{case}"
        );
        assert_eq!(
            product.usize_field("pattern_count"), Some(pattern_count),
            "{case}"
        );
        assert_eq!(
            product.usize_field("union_covered_patterns"), Some(expected_hits),
            "{case}"
        );
        assert_eq!(
            product
                .field("build_coverage_probability")
                .expect("Legacy union probability")
                .parse::<f64>()
                .expect("numeric probability"),
            covered.result().probability().get(),
            "{case}: complete uniform requested denominator",
        );
    }
}

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
    let mut native_batches = Vec::with_capacity(WITNESSES.len());
    for (index, witness) in WITNESSES.into_iter().enumerate() {
        // The native stream counts accepted rows from zero on each call.
        // Use a task-local reducer so its accepted count has the same scope.
        let mut sink = NativeCandidateReducer::new(&compact).expect("task-local candidate sink");
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
        assert_eq!(
            outcome.report.buildup_status, C_BUILDUP_STATUS_OK,
            "native BuildUp for witness {index}: {:?}",
            outcome.report
        );
        assert_eq!(
            outcome.status, 0,
            "native packing for witness {index}: {:?}",
            outcome.report
        );
        assert_ne!(outcome.report.complete, 0);
        assert_eq!(outcome.report.generated_count, 1);
        assert_eq!(outcome.report.buildable_count, 1);
        assert_eq!(sink.accepted_candidate_count(), 1);
        native_batches.push(sink.into_uncanonicalized_candidates());
    }
    let mut native_candidates =
        PackingCandidateBatch::merge_batches(native_batches, board.width(), board.height())
            .expect("merge independent native witness batches");
    native_candidates.canonicalize_identities();
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

const PREFIX_COUNT: usize = 31;
const PREFIX_ASSIGNMENT_COUNT: usize = 4;
const MAX_PHYSICAL_PREFIX_ORDINALS: usize = 3;

fn prefix_assignment_pieces() -> [[PieceKind; 10]; PREFIX_ASSIGNMENT_COUNT] {
    [
        PIECES,
        std::array::from_fn(|i| if i == 0 { PieceKind::O } else { PIECES[i] }),
        std::array::from_fn(|i| if i == 1 { PieceKind::I } else { PIECES[i] }),
        std::array::from_fn(|i| match i {
            0 => PieceKind::O,
            1 => PieceKind::I,
            _ => PIECES[i],
        }),
    ]
}

fn prefix_assignment_identity(pieces: [PieceKind; 10]) -> StandardBoard64TilingIdentity {
    StandardBoard64TilingIdentity::from_placements(
        0,
        pieces
            .into_iter()
            .zip(WITNESSES[0])
            .map(|(piece, mask)| PiecePlacementMask::new(piece, mask)),
    )
    .expect("exact slot ownership in original rows")
}

fn prefix_matrix_query(board: BoardSize) -> BuildCoverageQuery {
    let (slots, domains) = PIECES
        .into_iter()
        .zip(WITNESSES[0])
        .enumerate()
        .map(|(index, (piece, mask))| {
            let id = BuildSlotId::new(u32::try_from(index).expect("small slot ordinal"));
            let cells = (0..40_u16)
                .filter(|&cell| mask & (1_u64 << cell) != 0)
                .map(|cell| CellCoord::new_unchecked(cell % 10, cell / 10))
                .collect();
            let domain = if index < 2 {
                vec![PieceKind::I, PieceKind::O]
            } else {
                vec![piece]
            };
            (
                BuildSlot::new(id, cells).with_allowed_pieces(domain.clone()),
                SlotDomain::new(id, domain),
            )
        })
        .unzip();
    BuildCoverageQuery::new(
        BuildTemplate::new("legacy-bounded-prefix-matrix", slots).with_board_size(board),
        domains,
        Vec::new(),
        PREFIX_COUNT,
        BuildCoverageLimits::new(1024, 4096),
    )
}

#[test]
#[ignore = "measure this bounded native/Rust prefix matrix separately before routine qualification"]
fn native_and_rust_agree_on_complete_bounded_legacy_prefix_assignment_matrix() {
    let cancellation = ExecutionCancellationToken::new();
    let control = ExecutionControl::new(cancellation.clone());
    let board = BoardSize::new(10, 4).expect("standard four-line board");
    let query = prefix_matrix_query(board);
    let legacy = ProblemCompiler::compile_build(&BuildQuery::coverage_bridge(
        BuildTemplateBridge::new(query.template().id(), board, PIECES.len()),
        query.pattern_count(),
        BuildProblemLimits::new(
            query.limits().max_assignments(),
            query.limits().max_patterns(),
        ),
    ))
    .expect("actual Legacy Build bridge and observed ordinal source");
    let source = legacy.piece_source();
    let universe = source
        .materialized_universe()
        .expect("materialized Legacy source");
    assert!(
        !source.complete(),
        "this probe never qualifies the full source"
    );
    assert!(!legacy.supply().hold_enabled());
    assert!(universe.pattern_count() >= PREFIX_COUNT);
    let queues = (0..PREFIX_COUNT)
        .map(|ordinal| universe.sequence_at(ordinal))
        .collect::<Vec<_>>();
    let first = queues
        .iter()
        .position(|queue| queue.as_ref() == &PIECES)
        .expect("the original native witness queue is in scope");
    // Leave the first seven fixed. In the second bag O and I may be placed
    // in either order, with the same original-row masks, before T locks last.
    let mut swapped_tail = PIECES;
    swapped_tail.swap(7, 8);
    let second = queues
        .iter()
        .position(|queue| queue.as_ref() == &swapped_tail)
        .expect("find the independently replayed order in the real requested prefix");
    assert_ne!(first, second);

    let assignment_pieces = prefix_assignment_pieces();
    let identities = assignment_pieces.map(prefix_assignment_identity);
    let multisets = assignment_pieces.map(PieceMultisetKey::from_pieces);
    assert_eq!(
        multisets[0], multisets[3],
        "the swapped assignment cannot be ruled out by supply",
    );
    let compatible = queues
        .iter()
        .filter(|queue| multisets.contains(&PieceMultisetKey::from_pieces(queue.iter().copied())))
        .count();
    assert!(
        (2..=MAX_PHYSICAL_PREFIX_ORDINALS).contains(&compatible),
        "unexpected compatible fixture count: {compatible}"
    );
    let compact =
        CPackingProblemBuilder::from_search_problem_with_piece_multiset(&legacy, multisets[0])
            .expect("compact Geometry descriptor for the source multiset");
    let catalog = CoreCNative::compile_geometry_catalog_with_cancellation(&compact, &cancellation)
        .expect("Native exact ILC catalog");
    let view = catalog.view();
    let skeleton_row = |piece, mask| {
        view.skeleton_cell_masks()
            .iter()
            .zip(view.skeleton_piece_kinds())
            .position(|(&cells, &kind)| cells == mask && kind == u32::from(c_piece(piece)))
    };
    let skeleton_rows = PIECES
        .into_iter()
        .zip(WITNESSES[0])
        .map(|(piece, mask)| {
            u32::try_from(skeleton_row(piece, mask).expect("native exact positive placement"))
                .expect("catalog row fits u32")
        })
        .collect::<Vec<_>>();
    for wrong_pieces in assignment_pieces.iter().skip(1) {
        // I/O cannot represent one another's four-cell slot. An absent
        // skeleton row is a geometry proof; never reinterpret a failed or
        // resource-limited BuildUp call as a coverage zero.
        assert!(wrong_pieces
            .iter()
            .copied()
            .zip(WITNESSES[0])
            .any(|(piece, mask)| skeleton_row(piece, mask).is_none()));
    }

    let template =
        CBuildUpProblemTemplate::compile(&legacy).expect("Native observed source scratch");
    let mut scratch = template.new_scratch();
    template.attach_geometry_catalog(&mut scratch, &catalog);
    template
        .configure_piece_source_pattern(&mut scratch, u32::try_from(first).unwrap())
        .expect("admit the known candidate using its actual original ordinal");
    let mut workspace = NativeBuildUpWorkspace::new();
    let mut sink = NativeCandidateReducer::new(&compact).expect("one-candidate native reducer");
    let admitted = catalog
        .stream_buildable_rows(
            &skeleton_rows,
            &compact,
            &mut scratch,
            &mut workspace,
            &cancellation,
            PruningEvidencePolicy::BestEffort,
            &mut sink,
        )
        .expect("one concrete native candidate");
    assert_eq!(admitted.status, 0, "{admitted:?}");
    assert_ne!(admitted.report.complete, 0);
    assert_eq!(admitted.report.buildable_count, 1);
    let candidates = sink.into_candidates();
    assert_eq!(candidates.len(), 1);
    let candidate = candidates.iter().next().expect("verified native candidate");
    assert_eq!(
        candidate.standard_board64_tiling_identity(0).unwrap(),
        identities[0],
    );

    let mut native_bits = [0_u64; PREFIX_ASSIGNMENT_COUNT];
    let mut rust_bits = [0_u64; PREFIX_ASSIGNMENT_COUNT];
    for (ordinal, queue) in queues.into_iter().enumerate() {
        let multiset = PieceMultisetKey::from_pieces(queue.iter().copied());
        if !multisets.contains(&multiset) {
            // No hold, exactly ten locks: all four cells are exact
            // negative for this ordinal by piece conservation.
            continue;
        }
        let index = u32::try_from(ordinal).expect("small original ordinal");
        if multiset == multisets[0] {
            let mut scratch = template.new_scratch();
            template.attach_geometry_catalog(&mut scratch, &catalog);
            template
                .configure_packing_candidate(&mut scratch, &candidate, index, index)
                .expect("bind Native concrete candidate and original ordinal");
            let native = workspace
                .enumerate_buildup_variants_with_cancellation(
                    &scratch,
                    &CNativeBuildUpEnumerationLimits {
                        max_variants: 1,
                        ..Default::default()
                    },
                    &cancellation,
                )
                .expect("Native exact fixed-ordinal result");
            assert_ne!(
                native.buffer.count_complete, 0,
                "ordinal {ordinal}: {}",
                native.status,
            );
            if native.status == C_BUILDUP_STATUS_OK {
                assert_eq!(native.buffer.count, 1);
                assert!(native.buffer.total_variant_count > 0);
                let witness = &native.buffer.variants[0];
                assert_eq!(witness.candidate_id, candidate.candidate_id);
                assert_eq!(witness.coverage_pattern_id, index);
                assert_eq!(witness.placed_count, 10);
                assert_eq!(witness.cleared_lines, 4);
                assert_eq!(witness.final_board, 0);
                native_bits[0] |= 1_u64 << ordinal;
            } else {
                assert!(
                    (C_BUILDUP_STATUS_LOGICAL_REJECT_MIN..=C_BUILDUP_STATUS_LOGICAL_REJECT_MAX)
                        .contains(&native.status),
                    "non-logical Native failure at ordinal {ordinal}: {}",
                    native.status,
                );
                assert_eq!(native.buffer.total_variant_count, 0);
                assert_eq!(native.buffer.count, 0);
            }
        }

        let filters = identities
            .into_iter()
            .zip(multisets)
            .filter(|(_, needed)| *needed == multiset)
            .map(|(identity, _)| {
                StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(identity)
            });
        let rust_problem = ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0),
                PcQueueInput::fixed_sequence(FixedSequence::new(queue.into_owned())),
                PieceWindow::new(PIECES.len()),
            )
            .with_rule(legacy.rule_profile_value())
            .with_allow_hold(false)
            .with_exact_pieces(Some(PIECES.len()))
            .with_count_policy(PcCountPolicy::CountAll)
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
            .with_allowed_colored_solution_identities(filters)
            .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1)),
        )
        .expect("independent Rust physical execution for the same original ordinal");
        let rust = WasmCpuSearchBackend::execute_with_control(&rust_problem, &control)
            .expect("the fixed-ordinal Rust producer must finish");
        assert_eq!(rust.usize_field("coverage_pattern_count"), Some(1));
        for field in [
            "count_complete",
            "probability_complete",
            "solution_probability_complete",
        ] {
            assert_eq!(
                rust.bool_field(field),
                Some(true),
                "ordinal {ordinal}: {field}"
            );
        }
        assert_eq!(rust.bool_field("resource_truncated"), Some(false));
        for (assignment, identity) in identities.iter().enumerate() {
            let mut matching = rust
                .solution_coverages()
                .iter()
                .filter(|row| row.identity() == *identity);
            let row = matching.next();
            assert!(
                matching.next().is_none(),
                "duplicate Rust evidence for one exact identity",
            );
            assert_eq!(
                row.is_some(),
                rust.normalized_solution_identities().contains(identity),
            );
            if let Some(row) = row {
                assert_eq!(
                    multisets[assignment], multiset,
                    "Rust cannot add foreign supply"
                );
                assert_eq!(row.covered_patterns().pattern_count(), 1);
                assert_eq!(row.covered_patterns().word_at(0), 1);
                rust_bits[assignment] |= 1_u64 << ordinal;
            }
        }
    }
    assert_eq!(
        native_bits, rust_bits,
        "compare every assignment×original-ordinal cell",
    );
    assert_ne!(native_bits[0] & (1_u64 << first), 0);
    assert_ne!(native_bits[0] & (1_u64 << second), 0);
    assert_eq!(&native_bits[1..], &[0; PREFIX_ASSIGNMENT_COUNT - 1]);
    assert!(native_bits[0].count_ones() < PREFIX_COUNT as u32);

    // This artificially names only the fixture's original prefix. Its
    // denominator must not become the globally truncated Cover source.
    let scope = ConcreteCoverageScope::new_uniform(
        board,
        source.id().get(),
        PatternUniverseId::new(0x4c45_4741_4359_001f),
        PatternWeightModelId::new(0x554e_4946_4f52_001f),
        0x4e41_5449_5645_001f,
    );
    let evidence = identities
        .into_iter()
        .zip(rust_bits)
        .rev()
        .map(|(identity, word)| {
            ConcreteAssignmentCoverageRow::new(
                identity,
                scope,
                PatternBitSet::from_words(PREFIX_COUNT, vec![word]).unwrap(),
                ConcreteCoverageCompleteness::Complete,
            )
        })
        .collect::<Vec<_>>();
    assert!(matches!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope,
            evidence[1..].to_vec(),
        ),
        Err(BuildCoverageExecutionError::MissingConcreteCoverageRow { .. })
    ));
    let reduction =
        BuildCoverageExecution::from_verified_concrete_assignment_rows(&query, scope, evidence)
            .expect("complete shuffled identity-keyed rows, including explicit zeros");
    assert!(reduction.exact_cover_complete());
    assert_eq!(reduction.assignments().len(), PREFIX_ASSIGNMENT_COUNT);
    assert_eq!(reduction.matrix().matrix().pattern_count(), PREFIX_COUNT);
    for (assignment, matrix_row) in reduction
        .assignments()
        .iter()
        .zip(reduction.matrix().matrix().rows())
    {
        let exact = StandardBoard64TilingIdentity::from_placements(
            0,
            assignment.assigned_slots().iter().map(|slot| {
                PiecePlacementMask::new(slot.piece(), WITNESSES[0][slot.slot_id().get() as usize])
            }),
        )
        .expect("reducer identity still uses each concrete placement");
        let index = identities
            .iter()
            .position(|identity| *identity == exact)
            .unwrap();
        assert_eq!(matrix_row.coverage_bits().word_at(0), native_bits[index]);
    }
    let expected_union = PatternBitSet::from_words(PREFIX_COUNT, vec![native_bits[0]]).unwrap();
    let prefix_weights = WeightedPatternSet::uniform(PREFIX_COUNT).expect("uniform Legacy prefix");
    assert_eq!(reduction.union().covered_patterns(), &expected_union);
    assert_eq!(
        reduction.result().probability(),
        covered_weight_in_pattern_order(PREFIX_COUNT, &expected_union, |pattern| {
            prefix_weights.weight(pattern)
        })
        .expect("every covered ordinal has a prefix weight"),
        "exact uniform requested-prefix weights in Legacy ordinal order",
    );
    eprintln!(
        "legacy cover bounded prefix: assignments={PREFIX_ASSIGNMENT_COUNT}, original_ordinals={PREFIX_COUNT}, compatible_ordinals={compatible}, checked_positive_ordinals={first},{second}, covered_ordinals={}, prefix_probability={}",
        expected_union.count_ones(),
        reduction.result().probability().get(),
    );
}
