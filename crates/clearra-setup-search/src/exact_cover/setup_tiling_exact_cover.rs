use clearra_exact_cover::{
    bridge::SetupTilingBridge,
    solver::{DlxSearchLimits, DlxTruncatedReason},
};

pub struct SetupTilingExactCover;

impl SetupTilingExactCover {
    pub fn enumerate_shape_tilings(
        shape_mask: u64,
        candidate_masks: Vec<u64>,
        limits: DlxSearchLimits,
    ) -> Result<SetupTilingExactCoverReport, clearra_exact_cover::solver::DlxSolverError> {
        let report = SetupTilingBridge::enumerate(shape_mask, candidate_masks, limits)?;
        Ok(SetupTilingExactCoverReport {
            solution_candidate_ids: report
                .solutions()
                .iter()
                .map(|solution| solution.candidate_ids().to_vec())
                .collect(),
            complete: report.complete(),
            searched_nodes: report.searched_nodes(),
            truncation_reason: report.truncated_reason(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupTilingExactCoverReport {
    solution_candidate_ids: Vec<Vec<usize>>,
    complete: bool,
    searched_nodes: usize,
    truncation_reason: Option<DlxTruncatedReason>,
}

impl SetupTilingExactCoverReport {
    pub fn solution_candidate_ids(&self) -> &[Vec<usize>] {
        &self.solution_candidate_ids
    }
}
impl SetupTilingExactCoverReport {
    pub fn complete(&self) -> bool {
        self.complete
    }
}
impl SetupTilingExactCoverReport {
    pub fn searched_nodes(&self) -> usize {
        self.searched_nodes
    }
}
impl SetupTilingExactCoverReport {
    pub fn truncation_reason(&self) -> Option<DlxTruncatedReason> {
        self.truncation_reason
    }
}

#[cfg(test)]
mod tests {
    use clearra_exact_cover::solver::DlxSearchLimits;

    use super::*;

    #[test]
    fn standard_setup_tiling_still_works() {
        let report = SetupTilingExactCover::enumerate_shape_tilings(
            0b1111,
            vec![0b0011, 0b1100, 0b0101, 0b1010],
            DlxSearchLimits::new(8, 128),
        )
        .expect("tiling report");

        assert!(report.complete());
        assert_eq!(report.solution_candidate_ids().len(), 2);
        assert_eq!(report.truncation_reason(), None);
    }
}
