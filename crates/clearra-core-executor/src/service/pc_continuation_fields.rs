use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::{
    dag::ContinuationHint,
    request::{OpeningPcSearchQuery, PcContinuationTokenCodec, PcCountPolicy, PcHoldPolicy},
};
use clearra_problem::{PcQuery, SearchProblem, SearchProblemPreset};
use clearra_rules::profile::rule_profile::RuleProfile;
use clearra_supply::{
    execution_automaton::SupplyProvenanceId,
    hold::hold_policy::HoldPolicy,
    piece_source::{PieceSourceId, PieceSourceKind},
    QueueObservationPolicy, SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState,
    SupplyObservationIdentity,
};

use crate::{service::field, CoreExecutionResult, CorePathStep};

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
    continuation_fields(remaining, pc_query.hold_policy(), next_candidate, token)
}

fn continuation_fields(
    remaining: &[PieceKind],
    hold: PcHoldPolicy,
    next_candidate: Option<&'static str>,
    token: Option<String>,
) -> Vec<(String, String)> {
    let token_available = token.is_some();
    let token_value = token.unwrap_or_else(|| "none".to_owned());
    vec![
        field(
            "continuation_enough_queue_for_next_pc",
            next_candidate.is_some(),
        ),
        field("remaining_queue_len", remaining.len()),
        field("remaining_queue_preview", pieces_preview(remaining)),
        field("remaining_hold", hold_policy_name(hold)),
        field("next_pc_available", next_candidate.is_some()),
        field("next_pc_candidate", next_candidate.unwrap_or("none")),
        field("continuation_token_available", token_available),
        field(
            "continuation_token_unavailable_reason",
            if token_available {
                "none"
            } else {
                "insufficient_remaining_queue"
            },
        ),
        field("continue_available", token_available),
        field("continuation_available", token_available),
        field(
            "continuation_token_version",
            if token_available { "pc2" } else { "none" },
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

// The compact compatibility path supports a 64-entry fixed queue. Do not
// keep an unbounded suffix or the whole SearchProblem through postprocess.
const MAX_FIXED_OPENING_CONTINUATION_QUEUE: usize = 64;

/// Bounded input for an admitted standard compact PC opening. Preparing it
/// does not prove that the accepted result reaches the suffix.
pub struct PreparedFixedOpeningContinuation {
    fixed: [PieceKind; MAX_FIXED_OPENING_CONTINUATION_QUEUE],
    fixed_len: usize,
    placements: usize,
    target: PcTarget,
    rule: RuleProfile,
    objective: ObjectivePolicy,
    count_policy: PcCountPolicy,
    initial_hold: PcHoldPolicy,
    source_id: PieceSourceId,
}

impl PreparedFixedOpeningContinuation {
    pub fn prepare(problem: &SearchProblem) -> Option<Self> {
        let pc_query = problem.scenario().pc_query()?;
        if problem.preset() != SearchProblemPreset::OpeningPc
            || problem.piece_source().kind() != PieceSourceKind::FixedQueue
        {
            return None;
        }
        let fixed = problem
            .core_query()
            .remaining_queue()
            .as_fixed_sequence()?
            .pieces();
        let placements = pc_query.exact_piece_count();
        let initial_hold = pc_query.hold_policy();
        // The compatibility token retains the source target. A larger
        // residual horizon is not proof of a different target query. An
        // initially empty hold can draw one extra verified queue entry.
        let remaining = fixed.len().checked_sub(placements)?;
        let target = pc_query.target();
        let fits_without_hold_draw = remaining_fits_target(remaining, target);
        let fits_with_hold_draw = initial_hold == PcHoldPolicy::EnabledEmpty
            && remaining
                .checked_sub(1)
                .is_some_and(|remaining| remaining_fits_target(remaining, target));
        if !fits_without_hold_draw && !fits_with_hold_draw {
            return None;
        }
        let objective = pc_query.objective();
        let count_policy = pc_query.count_policy();
        if (objective != ObjectivePolicy::all() && objective != ObjectivePolicy::unique())
            || !matches!(
                count_policy,
                PcCountPolicy::CountAll | PcCountPolicy::CountUnique
            )
            || pc_query.verified_kick_profile().is_some()
            || pc_query.supply_window_size().is_some()
            || problem.solution_probability_policy().requested()
            || problem
                .queue_observation_policy()
                .requires_observation_policy()
        {
            return None;
        }
        let mut pieces = [PieceKind::I; MAX_FIXED_OPENING_CONTINUATION_QUEUE];
        pieces.get_mut(..fixed.len())?.copy_from_slice(fixed);
        Some(Self {
            fixed: pieces,
            fixed_len: fixed.len(),
            placements,
            target,
            rule: pc_query.rule(),
            objective,
            count_policy,
            initial_hold,
            source_id: problem.piece_source().id(),
        })
    }

    /// Called only on the accepted outcome of the prepared request. The
    /// concrete lock witness, not the existence of a suffix, binds the
    /// reached queue cursor and hold state.
    pub fn attach(&self, result: CoreExecutionResult) -> CoreExecutionResult {
        if result.field("continuation_token").is_some()
            || result.bool_field("solution_found") != Some(true)
            || result.bool_field("buildability_verified") != Some(true)
            || result.bool_field("count_complete") != Some(true)
            || result.bool_field("objective_search_complete") != Some(true)
            || result.bool_field("resource_truncated") != Some(false)
        {
            return result;
        }
        let steps = result.path_steps();
        if steps.len() != self.placements
            || steps
                .iter()
                .map(|step| u16::from(step.cleared_lines()))
                .sum::<u16>()
                != u16::from(self.target.lines())
        {
            return result;
        }
        let Some((remaining, hold)) = self.remaining_supply(steps) else {
            return result;
        };
        let hint = ContinuationHint::for_remaining_queue(remaining.len());
        if hint.next_target() != Some(self.target) {
            return result;
        }
        let query = OpeningPcSearchQuery::new(self.target)
            .with_rule(self.rule)
            .with_hold_policy(hold)
            .with_objective(self.objective)
            .with_count_policy(self.count_policy)
            .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle);
        let token = PcContinuationTokenCodec::encode_opening_continuation(
            &query,
            hold.initial_piece(),
            remaining,
        );
        result.with_additional_fields(continuation_fields(
            remaining,
            hold,
            Some(hint.next_label()),
            Some(token),
        ))
    }

    fn remaining_supply(&self, steps: &[CorePathStep]) -> Option<(&[PieceKind], PcHoldPolicy)> {
        let fixed = &self.fixed[..self.fixed_len];
        if !self.initial_hold.is_enabled() {
            let (prefix, remaining) = fixed.split_at(self.placements);
            return steps
                .iter()
                .map(CorePathStep::piece)
                .eq(prefix.iter().copied())
                .then_some((remaining, PcHoldPolicy::Disabled));
        }

        let automaton = SupplyExecutionAutomaton::sequence();
        let mut state = SupplyExecutionState::with_contract(
            self.source_id,
            PieceSourceKind::FixedQueue,
            0,
            self.initial_hold.initial_piece(),
            HoldPolicy::Allowed,
            0,
            0,
            SupplyObservationIdentity::full_queue_oracle(),
            SupplyProvenanceId(self.source_id.get()),
        );
        for step in steps {
            let branch = match step.hold() {
                "use-current" => SupplyBranchKind::Current,
                "swap-held" => SupplyBranchKind::SwapHeld,
                "store-current-use-next" => SupplyBranchKind::StoreCurrent,
                // A projected/unplaced piece or terminal release does not
                // establish the state of this concrete fixed suffix.
                _ => return None,
            };
            let cursor = usize::from(state.cursor());
            let next = automaton
                .transition(
                    state,
                    branch,
                    *fixed.get(cursor)?,
                    fixed.get(cursor + 1).copied(),
                )
                .ok()?;
            if next.used_piece != step.piece() {
                return None;
            }
            state = next.next_state;
        }
        let hold = state
            .hold_piece()
            .map_or(PcHoldPolicy::EnabledEmpty, PcHoldPolicy::EnabledWithPiece);
        Some((fixed.get(usize::from(state.cursor())..)?, hold))
    }
}

fn remaining_fits_target(remaining: usize, target: PcTarget) -> bool {
    ContinuationHint::for_remaining_queue(remaining).next_target() == Some(target)
}

/// Attach the existing pc2 continuation contract to an accepted compact Rust
/// Opening result using the already retained path, not a second search.
pub fn attach_exact_fixed_opening_continuation(
    problem: &SearchProblem,
    result: CoreExecutionResult,
) -> CoreExecutionResult {
    match PreparedFixedOpeningContinuation::prepare(problem) {
        Some(prepared) => prepared.attach(result),
        None => result,
    }
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
