use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::{
    dag::ContinuationHint,
    request::{PcContinuationTokenCodec, PcCountPolicy, PcHoldPolicy},
};
use clearra_problem::{PcQuery, SearchProblem, SearchProblemPreset};

use crate::{service::field, CoreExecutionResult};

pub(crate) fn opening_continuation_fields(
    pc_query: &PcQuery,
    fixed_pieces: Option<&[PieceKind]>,
    consumed: usize,
) -> Vec<(String, String)> {
    let remaining = fixed_pieces
        .map(|pieces| pieces.get(consumed..).unwrap_or(&[]))
        .unwrap_or(&[]);
    let hint = ContinuationHint::for_remaining_queue(remaining.len());
    let next_candidate = hint.is_available().then_some(hint.next_label());
    let token = if next_candidate.is_some() && !remaining.is_empty() {
        let query = pc_query.to_opening_query();
        Some(PcContinuationTokenCodec::encode_opening_continuation(
            &query,
            pc_query.hold_policy().initial_piece(),
            remaining,
        ))
    } else {
        None
    };
    let token_value = token.clone().unwrap_or_else(|| "none".to_owned());
    vec![
        field(
            "continuation_enough_queue_for_next_pc",
            next_candidate.is_some(),
        ),
        field("remaining_queue_len", remaining.len()),
        field("remaining_queue_preview", pieces_preview(remaining)),
        field("remaining_hold", hold_policy_name(pc_query.hold_policy())),
        field("next_pc_available", next_candidate.is_some()),
        field("next_pc_candidate", next_candidate.unwrap_or("none")),
        field("continuation_token_available", token.is_some()),
        field(
            "continuation_token_unavailable_reason",
            if token.is_some() {
                "none"
            } else {
                "insufficient_remaining_queue"
            },
        ),
        field("continue_available", token.is_some()),
        field("continuation_available", token.is_some()),
        field(
            "continuation_token_version",
            if token.is_some() { "pc2" } else { "none" },
        ),
        field("continuation_token", token_value.clone()),
        field(
            "continue_hint",
            if token_value == "none" {
                "none".to_owned()
            } else {
                format!("clearra continue {token_value}")
            },
        ),
    ]
}

/// Attach the existing pc2 continuation contract to an accepted compact Rust
/// Opening result. For a fixed, no-hold 2L search the five accepted locks
/// consume the first five queue entries exactly. We use the path already
/// retained by the solver; this adapter does not request a replay family.
pub fn attach_exact_fixed_no_hold_2l_opening_continuation(
    problem: &SearchProblem,
    result: CoreExecutionResult,
) -> CoreExecutionResult {
    let Some(pc_query) = problem.scenario().pc_query() else {
        return result;
    };
    if problem.preset() != SearchProblemPreset::OpeningPc
        || pc_query.target() != PcTarget::two_lines()
        || pc_query.hold_policy() != PcHoldPolicy::Disabled
        || result.field("continuation_token").is_some()
    {
        return result;
    }
    let Some(fixed) = problem.core_query().remaining_queue().as_fixed_sequence() else {
        return result;
    };
    let fixed = fixed.pieces();
    let consumed = pc_query.exact_piece_count();
    // pc2 retains the source target. Do not publish a 4L/6L next-puzzle
    // label with a 2L token before those distinct contracts are qualified.
    let hint = ContinuationHint::for_remaining_queue(fixed.len().saturating_sub(consumed));
    if hint.next_target() != Some(pc_query.target()) {
        return result;
    }
    let objective = pc_query.objective();
    if (objective != ObjectivePolicy::all() && objective != ObjectivePolicy::unique())
        || !matches!(
            pc_query.count_policy(),
            PcCountPolicy::CountAll | PcCountPolicy::CountUnique
        )
        || pc_query.verified_kick_profile().is_some()
        || pc_query.supply_window_size().is_some()
        || problem.solution_probability_policy().requested()
        || problem
            .queue_observation_policy()
            .requires_observation_policy()
        || result.bool_field("solution_found") != Some(true)
        || result.bool_field("buildability_verified") != Some(true)
        || result.bool_field("count_complete") != Some(true)
        || result.bool_field("objective_search_complete") != Some(true)
        || result.bool_field("resource_truncated") != Some(false)
    {
        return result;
    }
    let steps = result.path_steps();
    let prefix = &fixed[..consumed]; // The same-target hint proved a nonempty suffix.
    if steps.len() != consumed
        || !steps
            .iter()
            .map(|step| step.piece())
            .eq(prefix.iter().copied())
        || steps
            .iter()
            .map(|step| u16::from(step.cleared_lines()))
            .sum::<u16>()
            != u16::from(pc_query.target().lines())
    {
        return result;
    }
    let fields = opening_continuation_fields(pc_query, Some(fixed), consumed);
    result.with_additional_fields(fields)
}

pub(crate) fn remaining_preview(pieces: Option<&[PieceKind]>, consumed: usize) -> String {
    pieces
        .map(|pieces| pieces_preview(pieces.get(consumed..).unwrap_or(&[])))
        .unwrap_or_else(|| "none".to_owned())
}

fn hold_policy_name(policy: PcHoldPolicy) -> String {
    match policy {
        PcHoldPolicy::Disabled | PcHoldPolicy::EnabledEmpty => "none".to_owned(),
        PcHoldPolicy::EnabledWithPiece(piece) => piece.as_ascii().to_string(),
    }
}

fn pieces_preview(pieces: &[PieceKind]) -> String {
    if pieces.is_empty() {
        return "none".to_owned();
    }
    pieces.iter().map(|piece| piece.as_ascii()).collect()
}
