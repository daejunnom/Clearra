use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
use clearra_supply::queue::observed_queue::ObservedQueue;

use crate::{
    assemble::{
        execution_policy_assembler::{assemble_policy, ExecutionPolicyInput},
        parse_hex_mask,
        pc_scenario_policy_assembler::count_policy,
        piece_sequence_assembler::PieceSequenceAssembler,
        rule_profile_assembler::RuleProfileAssembler,
    },
    fixture::PcScenarioFixture,
};

pub(super) fn query_from_fixture(fixture: &PcScenarioFixture) -> Result<PcScenarioQuery, String> {
    let scenario = fixture.scenario();
    if scenario.goal() != "clear-to-empty" {
        return Err(format!("unsupported scenario goal '{}'", scenario.goal()));
    }
    let mut query = PcScenarioQuery::new(
        PcScenarioBoard::new(
            scenario.board_width(),
            scenario.visible_height(),
            parse_hex_mask(scenario.initial_board_mask())?,
        ),
        queue_input(scenario)?,
        PieceWindow::new(scenario.max_pieces()),
    )
    .with_hold_piece(match scenario.hold() {
        Some(piece) => {
            Some(PieceSequenceAssembler::parse_piece(piece).map_err(|error| error.message())?)
        }
        None => None,
    })
    .with_rule(RuleProfileAssembler::parse_rule(scenario.rule()).map_err(|error| error.message())?)
    .with_requires_180(scenario.requires_180())
    .with_exact_pieces(scenario.exact_pieces())
    .with_min_remaining_queue(scenario.min_remaining_queue())
    .with_allow_hold(scenario.allow_hold())
    .with_count_policy(count_policy(scenario.count_policy())?);
    query = query.with_execution_policy(
        assemble_policy(ExecutionPolicyInput {
            backend: scenario.backend(),
            workers: scenario.workers(),
            use_all_logical_processors: None,
            cpu_warmup: None,
            gpu_warmup: None,
            deterministic: scenario.deterministic(),
            max_frontier_states: scenario.max_frontier_states(),
            max_candidates: scenario.max_candidates(),
            max_patterns: scenario.max_patterns(),
            max_memory_mib: scenario.max_memory_mib(),
            gpu_device: scenario.gpu_device(),
            allow_backend_fallback: scenario.allow_backend_fallback(),
        })
        .map_err(|error| error.message())?,
    );
    if let Some(retained_trace_limit) = scenario.retained_trace_limit() {
        query = query.with_retained_trace_limit(retained_trace_limit);
    }
    let kick_profile_json = scenario.kick_profile_json_string();
    if let Some(profile) =
        RuleProfileAssembler::parse_verified_kick_profile(kick_profile_json.as_deref())
            .map_err(|error| error.message())?
    {
        query = query.with_verified_kick_table_profile(profile);
    }
    Ok(query)
}

fn queue_input(scenario: &crate::fixture::ScenarioFixtureInput) -> Result<PcQueueInput, String> {
    let sequence = PieceSequenceAssembler::parse_fixed_sequence(scenario.remaining_queue())
        .map_err(|error| error.message())?;
    match scenario.queue_mode() {
        "fixed" => Ok(PcQueueInput::fixed_sequence(sequence)),
        "observed" => Ok(PcQueueInput::observed(ObservedQueue::new(
            sequence.pieces().to_vec(),
        ))),
        "standard-7-bag" => {
            if !sequence.is_empty() {
                return Err(
                    "scenario queue_mode 'standard-7-bag' requires an empty realized queue"
                        .to_owned(),
                );
            }
            Ok(PcQueueInput::standard_7_bag())
        }
        mode => Err(format!("unsupported scenario queue_mode '{mode}'")),
    }
}
