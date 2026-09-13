// SRP rationale: prepare a fully declared observation/hold candidate request
// from the existing input-disclosure contract. No Range request or fallback
// is started here; the common online candidate session owns that lifecycle.
use core::{fmt, num::NonZeroUsize};

use clearra_core_domain::board::standard_pc_board::StandardPcBoard;
use clearra_pc4_tablebase::{
    prepare_pc4_observation_frontier, ConcretePathMaterializationBudgets, FixedQueueHoldState,
    FixedQueueTraversalBudgets, FixedQueueTraversalPageBudgets, LookupSessionId,
    Pc4ObservationFrontierBudgets, Pc4ObservationFrontierFamily,
    Pc4ObservationFrontierPrepareError, Pc4ObservationFrontierRequest, Pc4ObservationGraphBudgets,
    RangeAdmissionLimits, TerminalDepthContract,
};

use crate::{
    online_pc4_fixed_queue_candidate_session::{
        validate_prepared_candidate_source, AppOnlinePc4FixedQueueCandidateRequestError,
    },
    pc4_input_disclosure_policy::{Pc4PreparedOnlineInput, Pc4PreparedQueueInput},
    pc4_lookup_graph_runtime_adapter::Pc4LookupGraphCacheLimits,
    pc4_observation_candidate_adapter::Pc4ObservationCandidateBudgets,
    pc_candidate_page_boundary::{PcCandidatePageGuard, PcCandidateSourceBinding},
};

pub struct AppOnlinePc4ObservationCandidateRequest<'a> {
    pub(crate) prepared_input: &'a Pc4PreparedOnlineInput,
    pub(crate) source: &'a PcCandidateSourceBinding,
    pub(crate) first_lookup_session: LookupSessionId,
    pub(crate) start_field_id: u32,
    pub(crate) frontier: Pc4ObservationFrontierFamily,
    pub(crate) terminal_depth_contract: TerminalDepthContract,
    pub(crate) traversal_budgets: FixedQueueTraversalBudgets,
    pub(crate) traversal_page_budgets: FixedQueueTraversalPageBudgets,
    pub(crate) graph_budgets: Pc4ObservationGraphBudgets,
    pub(crate) materialization_budgets: ConcretePathMaterializationBudgets,
    pub(crate) candidate_budgets: Pc4ObservationCandidateBudgets,
    pub(crate) cache_limits: Pc4LookupGraphCacheLimits,
    pub(crate) observation_page_size: NonZeroUsize,
    pub(crate) range_limits: RangeAdmissionLimits,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppOnlinePc4ObservationCandidateRequestError {
    Source(AppOnlinePc4FixedQueueCandidateRequestError),
    InvalidPlacementArea,
    PlacementHorizonMismatch { expected: usize, declared: usize },
    HiddenBagStateMissing,
    Frontier(Pc4ObservationFrontierPrepareError),
}

impl AppOnlinePc4ObservationCandidateRequestError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Source(error) => error.reason(),
            Self::InvalidPlacementArea => "pc4_online_observation_invalid_placement_area",
            Self::PlacementHorizonMismatch { .. } => {
                "pc4_online_observation_placement_horizon_mismatch"
            }
            Self::HiddenBagStateMissing => "pc4_online_observation_hidden_bag_state_missing",
            Self::Frontier(error) => error.reason(),
        }
    }
}

impl fmt::Display for AppOnlinePc4ObservationCandidateRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}
impl std::error::Error for AppOnlinePc4ObservationCandidateRequestError {}

impl<'a> AppOnlinePc4ObservationCandidateRequest<'a> {
    /// Reuses the exact finite queue or declared hidden bag family; it does not
    /// infer an undisclosed suffix or replace hold choices with a best action.
    #[allow(clippy::too_many_arguments)]
    pub fn for_prepared_input<G: PcCandidatePageGuard>(
        source: &'a PcCandidateSourceBinding,
        prepared_input: &'a Pc4PreparedOnlineInput,
        initial_board: StandardPcBoard,
        initial_hold: FixedQueueHoldState,
        first_lookup_session: LookupSessionId,
        start_field_id: u32,
        frontier_budgets: Pc4ObservationFrontierBudgets,
        terminal_depth_contract: TerminalDepthContract,
        traversal_budgets: FixedQueueTraversalBudgets,
        traversal_page_budgets: FixedQueueTraversalPageBudgets,
        graph_budgets: Pc4ObservationGraphBudgets,
        materialization_budgets: ConcretePathMaterializationBudgets,
        candidate_budgets: Pc4ObservationCandidateBudgets,
        cache_limits: Pc4LookupGraphCacheLimits,
        observation_page_size: NonZeroUsize,
        range_limits: RangeAdmissionLimits,
        guard: &G,
    ) -> Result<Self, AppOnlinePc4ObservationCandidateRequestError> {
        validate_prepared_candidate_source(source, prepared_input, initial_board, initial_hold)
            .map_err(AppOnlinePc4ObservationCandidateRequestError::Source)?;
        let cells = u32::from(initial_board.cell_count()) - initial_board.occupied().count_ones();
        if !cells.is_multiple_of(4) {
            return Err(AppOnlinePc4ObservationCandidateRequestError::InvalidPlacementArea);
        }
        let placement_count = (cells / 4) as usize;
        let frontier_request = match prepared_input.queue() {
            Pc4PreparedQueueInput::FixedExplicit(queue) => {
                Pc4ObservationFrontierRequest::fixed_queue(
                    queue,
                    initial_hold,
                    placement_count,
                    frontier_budgets,
                )
            }
            Pc4PreparedQueueInput::PatternOrHidden {
                visible_queue,
                scope,
                bag_state,
                ..
            } => {
                if scope.placement_count() != placement_count {
                    return Err(
                        AppOnlinePc4ObservationCandidateRequestError::PlacementHorizonMismatch {
                            expected: placement_count,
                            declared: scope.placement_count(),
                        },
                    );
                }
                if scope.hidden_draws() == 0 {
                    Pc4ObservationFrontierRequest::fixed_queue(
                        visible_queue,
                        initial_hold,
                        placement_count,
                        frontier_budgets,
                    )
                } else {
                    let state = bag_state.ok_or(
                        AppOnlinePc4ObservationCandidateRequestError::HiddenBagStateMissing,
                    )?;
                    Pc4ObservationFrontierRequest::new(
                        visible_queue,
                        scope.preview_length(),
                        state,
                        scope.hidden_draws(),
                        initial_hold,
                        placement_count,
                        frontier_budgets,
                    )
                }
            }
        };
        let frontier = prepare_pc4_observation_frontier(frontier_request, &|| guard.is_cancelled())
            .map_err(AppOnlinePc4ObservationCandidateRequestError::Frontier)?;
        Ok(Self {
            prepared_input,
            source,
            first_lookup_session,
            start_field_id,
            frontier,
            terminal_depth_contract,
            traversal_budgets,
            traversal_page_budgets,
            graph_budgets,
            materialization_budgets,
            candidate_budgets,
            cache_limits,
            observation_page_size,
            range_limits,
        })
    }
}
