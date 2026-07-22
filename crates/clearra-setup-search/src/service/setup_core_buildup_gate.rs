use clearra_core_executor::{BuildUpRunner, PackingRunner};
use clearra_problem::ProblemCompiler;

use crate::{enumerate::BuildUpVariantProof, query::SetupSearchQuery};

use super::{
    setup_candidate_enumerator::SetupBuildCandidate,
    setup_search_service::SetupSearchExecutionError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SetupCoreBuildGate {
    packing_candidate_count: usize,
    successful_build_variant_count: usize,
    coverage_row_count: usize,
}

impl SetupCoreBuildGate {
    pub(crate) fn from_query(query: &SetupSearchQuery) -> Result<Self, SetupSearchExecutionError> {
        let problem = ProblemCompiler::compile_setup(query)
            .map_err(|_| SetupSearchExecutionError::CoreBuildUp)?;
        let packing =
            PackingRunner::run(&problem).map_err(|_| SetupSearchExecutionError::CoreBuildUp)?;
        let buildup = BuildUpRunner::run(&problem, &packing)
            .map_err(|_| SetupSearchExecutionError::CoreBuildUp)?;

        let gate = Self {
            packing_candidate_count: packing.candidate_count(),
            successful_build_variant_count: buildup.build_variant_count(),
            coverage_row_count: buildup.coverage_row_count(),
        };
        if gate.has_build_variants() {
            Ok(gate)
        } else {
            Err(SetupSearchExecutionError::CoreBuildUp)
        }
    }
}
impl SetupCoreBuildGate {
    fn proof(&self) -> BuildUpVariantProof {
        BuildUpVariantProof::new(self.successful_build_variant_count, self.coverage_row_count)
    }
}
impl SetupCoreBuildGate {
    fn has_build_variants(&self) -> bool {
        self.proof().has_build_variant()
    }
}
impl SetupCoreBuildGate {
    pub(crate) fn proof_for_candidate(
        &self,
        candidate: &SetupBuildCandidate,
    ) -> BuildUpVariantProof {
        self.proof().with_build_input(
            candidate.occupied_shape,
            candidate.final_hold,
            candidate.placed_pieces.len(),
        )
    }
}
impl SetupCoreBuildGate {
    pub(crate) fn packing_candidate_count(&self) -> usize {
        self.packing_candidate_count
    }
}
impl SetupCoreBuildGate {
    pub(crate) fn successful_build_variant_count(&self) -> usize {
        self.successful_build_variant_count
    }
}
impl SetupCoreBuildGate {
    pub(crate) fn coverage_row_count(&self) -> usize {
        self.coverage_row_count
    }
}
