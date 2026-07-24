use clearra_pc_graph::request::{
    PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow,
};

use crate::{
    args::pc_scenario_args::PcScenarioArgs,
    assemble::{
        execution_policy_assembler::ExecutionPolicyAssembler, parse_hex_mask,
        pc_scenario_policy_assembler::count_policy,
        piece_sequence_assembler::PieceSequenceAssembler,
        rule_profile_assembler::RuleProfileAssembler,
    },
};

pub(super) fn inline_query(args: &PcScenarioArgs) -> Result<PcScenarioQuery, String> {
    let field = args
        .field()
        .ok_or_else(|| "inline pc-scenario requires --field 0x...".to_owned())?;
    let queue = args
        .queue()
        .ok_or_else(|| "inline pc-scenario requires --queue <pieces>".to_owned())?;
    let sequence =
        PieceSequenceAssembler::parse_fixed_sequence(queue).map_err(|error| error.message())?;
    if sequence.is_empty() {
        return Err("inline pc-scenario queue must not be empty".to_owned());
    }
    let max_pieces = args.max_pieces().unwrap_or(sequence.len());
    let mut query = PcScenarioQuery::new(
        PcScenarioBoard::new(
            args.board_width().unwrap_or(10),
            args.visible_height().unwrap_or(2),
            parse_hex_mask(field)?,
        ),
        PcQueueInput::fixed_sequence(sequence),
        PieceWindow::new(max_pieces),
    )
    .with_hold_piece(match args.hold() {
        Some(piece) => {
            Some(PieceSequenceAssembler::parse_piece(piece).map_err(|error| error.message())?)
        }
        None => None,
    })
    .with_rule(
        RuleProfileAssembler::parse_rule(args.rule().unwrap_or("srs-plus"))
            .map_err(|error| error.message())?,
    )
    .with_requires_180(args.requires_180())
    .with_exact_pieces(args.exact_pieces())
    .with_min_remaining_queue(args.min_remaining_queue().unwrap_or(0))
    .with_allow_hold(args.allow_hold().unwrap_or(true))
    .with_count_policy(count_policy(args.count_policy())?);
    query = query.with_execution_policy(
        ExecutionPolicyAssembler::from_pc_scenario_args(args).map_err(|error| error.message())?,
    );
    if let Some(retained_trace_limit) = args.retained_trace_limit() {
        query = query.with_retained_trace_limit(retained_trace_limit);
    }
    if let Some(profile) =
        RuleProfileAssembler::parse_verified_kick_profile(args.kick_profile_json())
            .map_err(|error| error.message())?
    {
        query = query.with_verified_kick_table_profile(profile);
    }
    if args.solution_probabilities() {
        query = query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
    }
    Ok(query)
}
