use clearra_core_executor::CoreExecutionResult;
use clearra_problem::SetupSearchQuery;

use crate::{
    setup_ranked_family_result::{SetupRankedFamilyResult, SetupRankedFamilyResultError},
    setup_ranking_contract::SetupRankingContract,
};

/// The single promotion boundary from an actual Setup executor result into a
/// query-bound ranked-family result.
pub struct SetupRankingFacade;

impl SetupRankingFacade {
    pub fn promote(
        contract: SetupRankingContract,
        query: &SetupSearchQuery,
        core_result: CoreExecutionResult,
    ) -> Result<SetupRankedFamilyResult, SetupRankedFamilyResultError> {
        SetupRankedFamilyResult::from_core_result(contract, query, core_result)
    }
}
