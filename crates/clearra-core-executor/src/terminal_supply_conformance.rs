use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        StandardBoard64TilingIdentity,
    },
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_supply::queue::{
    fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
};

pub(crate) const TERMINAL_SUPPLY_P0_INITIAL_MASK: u64 = 0x1c0701c07;
pub(crate) const TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT: usize = 18;
pub(crate) const TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH: &str = "cts1:8a7fc484d9b49994";
const TERMINAL_SUPPLY_P0_PACKED_PIECE_CODES: u64 = 0x1ac688;

// This exact identity authority was machine-derived once from the native P0
// enumeration (not copied from the issue document), then frozen as data-only
// evidence. Future compiler, geometry, WASM, and C paths must reproduce it
// rather than defining the expected set together.
const TERMINAL_SUPPLY_P0_EXPECTED_PLACEMENT_MASKS: [[u64; 7]; 18] = [
    [
        120,
        25_190_400,
        67_338_240,
        786_816,
        206_561_083_392,
        60_163_096_576,
        825_170_592_256,
    ],
    [
        122_880,
        806_092_800,
        131_520,
        17_230_200_864,
        103_280_541_696,
        962_072_674_816,
        8_598_323_224,
    ],
    [
        122_880,
        25_794_969_600,
        131_520,
        269_222_400,
        103_280_541_696,
        962_609_545_216,
        33_554_488,
    ],
    [
        983_040,
        805_307_136,
        32_880,
        50_356_224,
        103_280_541_696,
        25_778_192_392,
        962_072_674_432,
    ],
    [
        125_829_120,
        24_600,
        229_440,
        786_816,
        206_561_083_392,
        60_129_542_176,
        825_170_592_256,
    ],
    [
        125_829_120,
        24_600,
        229_440,
        274_878_693_888,
        206_158_430_592,
        60_129_542_176,
        550_695_337_984,
    ],
    [
        125_829_120,
        24_600,
        263_040,
        196_704,
        206_561_083_392,
        60_129_574_912,
        825_171_116_032,
    ],
    [
        125_829_120,
        24_600,
        939_786_240,
        196_704,
        206_158_430_592,
        60_129_574_912,
        824_634_245_632,
    ],
    [
        1_006_632_960,
        787_200,
        16_440,
        103_129_546_752,
        98_496,
        25_778_200_576,
        962_072_805_376,
    ],
    [
        1_006_632_960,
        787_200,
        58_736_640,
        98_352,
        103_079_215_296,
        25_769_811_976,
        962_072_805_376,
    ],
    [
        8_598_331_400,
        49_200,
        234_881_088,
        269_222_400,
        196_992,
        962_609_545_216,
        120_275_861_504,
    ],
    [
        8_598_331_400,
        806_092_800,
        131_520,
        17_230_200_864,
        103_280_541_696,
        962_072_674_816,
        114_704,
    ],
    [
        8_598_331_400,
        824_633_721_600,
        65_760,
        100_712_448,
        206_561_083_392,
        51_556_384_784,
        537_788_416,
    ],
    [
        515_396_075_520,
        787_200,
        57_360,
        67_305_600,
        50_331_744,
        25_778_192_392,
        550_695_337_984,
    ],
    [
        550_293_209_600,
        24_600,
        117_440_544,
        134_611_200,
        98_496,
        481_304_772_608,
        60_137_930_752,
    ],
    [
        550_293_209_600,
        403_046_400,
        65_760,
        8_615_100_432,
        51_640_270_848,
        481_036_337_408,
        57_352,
    ],
    [
        550_293_209_600,
        412_316_860_800,
        32_880,
        50_356_224,
        103_280_541_696,
        25_778_192_392,
        268_894_208,
    ],
    [
        1_030_792_151_040,
        24_600,
        67_338_240,
        786_816,
        50_331_744,
        939_524_608,
        60_137_930_752,
    ],
];

pub(crate) fn terminal_supply_p0_expected_identities() -> Vec<StandardBoard64TilingIdentity> {
    TERMINAL_SUPPLY_P0_EXPECTED_PLACEMENT_MASKS
        .iter()
        .map(|placement_masks| {
            StandardBoard64TilingIdentity::from_compact_parts(
                TERMINAL_SUPPLY_P0_INITIAL_MASK,
                TERMINAL_SUPPLY_P0_PACKED_PIECE_CODES,
                placement_masks,
            )
            .expect("frozen terminal-supply P0 identity")
        })
        .collect()
}

pub(crate) fn terminal_supply_p0_fixed_problem() -> SearchProblem {
    terminal_supply_p0_problem(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
        PieceKind::S,
        PieceKind::T,
        PieceKind::O,
        PieceKind::I,
        PieceKind::L,
        PieceKind::J,
        PieceKind::Z,
    ])))
}

pub(crate) fn terminal_supply_p0_generic_problem() -> SearchProblem {
    terminal_supply_p0_problem(PcQueueInput::pattern_expression(
        QueuePatternExpression::parse("STOILJZ", 1).expect("terminal-supply P0 literal pattern"),
    ))
}

fn terminal_supply_p0_problem(queue: PcQueueInput) -> SearchProblem {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, TERMINAL_SUPPLY_P0_INITIAL_MASK),
        queue,
        PieceWindow::new(7),
    )
    .with_allow_hold(true)
    .with_exact_pieces(Some(7))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_workers(1)
            .with_worker_hardware_limit(1),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("terminal-supply P0 problem");
    assert!(problem.supply().hold_enabled());
    assert!(problem.supply().projects_unplaced_lookahead());
    assert!(!problem.supply().projects_standard_bag_lookahead());
    problem
}

#[test]
fn terminal_supply_p0_explicit_identity_authority_is_sorted_unique_and_hash_stable() {
    let identities = terminal_supply_p0_expected_identities();
    assert_eq!(identities.len(), TERMINAL_SUPPLY_P0_EXPECTED_UNIQUE_COUNT);
    assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(&identities)
            .as_str(),
        TERMINAL_SUPPLY_P0_EXPECTED_NORMALIZED_SET_HASH
    );
}
