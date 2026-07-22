use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_rules::profile::builtin_rules::{srs, srs_plus};

use crate::args::{ArtifactArgs, ArtifactCountMode, ArtifactRule, ArtifactScenario};

const PCO_6P_BOARD_MASK: u64 = 0x0000_00e0_f87e_3f87;
const PCO_6P_PATTERN_COUNT: usize = 840;
const TSAR_CANNON_BOARD_MASK: u64 = 0x0003_00c0_399e_3fdf;
const STANDARD_7_BAG_PATTERN_COUNT: usize = 5_040;
// Ten placements with an initially empty hold can consume eleven draws and
// leave one piece held: 7! first bags times P(7, 4) second-bag prefixes.
const EMPTY_4L_TOTAL_PATTERN_COUNT: u128 = 4_233_600;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtifactScenarioSpec {
    pub id: &'static str,
    pub initial_board_mask: u64,
    pub visible_height: u16,
    pub piece_window: usize,
    pub hold_piece: Option<PieceKind>,
    pub expected_unique_solution_count: Option<usize>,
    pub total_possible_pattern_count: u128,
    pub count_mode: ArtifactCountMode,
    pub materialization_limit: usize,
}

impl ArtifactScenarioSpec {
    pub fn from_args(args: &ArtifactArgs) -> Self {
        let mut spec = match args.scenario {
            ArtifactScenario::Pco6p => Self {
                id: args.scenario.as_str(),
                initial_board_mask: PCO_6P_BOARD_MASK,
                visible_height: 4,
                piece_window: 4,
                hold_piece: Some(PieceKind::I),
                expected_unique_solution_count: Some(63),
                total_possible_pattern_count: PCO_6P_PATTERN_COUNT as u128,
                count_mode: ArtifactCountMode::All,
                materialization_limit: PCO_6P_PATTERN_COUNT,
            },
            ArtifactScenario::TsarCannon => Self {
                id: args.scenario.as_str(),
                initial_board_mask: TSAR_CANNON_BOARD_MASK,
                visible_height: 5,
                piece_window: 6,
                hold_piece: None,
                expected_unique_solution_count: Some(42),
                total_possible_pattern_count: STANDARD_7_BAG_PATTERN_COUNT as u128,
                count_mode: ArtifactCountMode::All,
                materialization_limit: STANDARD_7_BAG_PATTERN_COUNT,
            },
            ArtifactScenario::Empty4l => Self {
                id: args.scenario.as_str(),
                initial_board_mask: 0,
                visible_height: 4,
                piece_window: 10,
                hold_piece: None,
                expected_unique_solution_count: None,
                total_possible_pattern_count: EMPTY_4L_TOTAL_PATTERN_COUNT,
                count_mode: ArtifactCountMode::Unique,
                materialization_limit: 5_040,
            },
        };
        if let Some(count_mode) = args.count_mode {
            spec.count_mode = count_mode;
        }
        if let Some(max_patterns) = args.max_patterns {
            spec.materialization_limit = max_patterns;
        }
        spec
    }

    pub fn query(self, args: &ArtifactArgs) -> PcScenarioQuery {
        let execution_policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(args.backend)
            .with_gpu_device(args.gpu_device.clone())
            .with_allow_backend_fallback(args.allow_fallback)
            .with_workers(args.workers)
            .with_use_all_logical_processors(args.use_all_logical_processors)
            .with_cpu_warmup(args.cpu_warmup)
            .with_max_patterns(self.materialization_limit)
            .with_max_candidates(args.max_candidates)
            .with_max_frontier_states(args.max_frontier_states);
        let rule = match args.rule {
            ArtifactRule::SrsPlus => srs_plus(),
            ArtifactRule::Srs => srs(),
        };
        let retained_trace_limit = match self.count_mode {
            ArtifactCountMode::Unique => 0,
            ArtifactCountMode::All => self.expected_unique_solution_count.unwrap_or(4_096),
        };
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(self.visible_height, self.initial_board_mask),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(self.piece_window),
        )
        .with_exact_pieces(Some(self.piece_window))
        .with_min_remaining_queue(0)
        .with_hold_piece(self.hold_piece)
        .with_allow_hold(true)
        .with_rule(rule)
        .with_count_policy(match self.count_mode {
            ArtifactCountMode::All => PcCountPolicy::CountAll,
            ArtifactCountMode::Unique => PcCountPolicy::CountUnique,
        })
        .with_retained_trace_limit(retained_trace_limit)
        .with_execution_policy(execution_policy)
    }
}
