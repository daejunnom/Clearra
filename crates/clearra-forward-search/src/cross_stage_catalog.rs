//! Existential join over two declared exact-lock-time candidate catalogs.
//!
//! Catalog generation (and conversion from static tilings through line clears)
//! belongs to the producer. This layer never treats a partial catalog as an
//! exhaustive Build universe. It probes every candidate pair for a normal
//! witness BEFORE classifying any interleaved witness as additional recovery.

use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;

use crate::cross_stage_recovery::{
    CrossStageEarlyLimit, CrossStagePairReport, CrossStagePairStatus, CrossStageRecoveryQuery,
    CrossStageRole, CrossStageSearchError,
};
use crate::reachability::ReachabilityWorkspace;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CrossStageCatalogCompletion {
    /// Some candidates or lock-time realizations may still be missing.
    #[default]
    Partial,
    /// Exhausted only in the producer's declared scope, not arbitrary geometry.
    Exhausted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossStageCatalog {
    candidates: Vec<Vec<CrossStageRole>>,
    completion: CrossStageCatalogCompletion,
}

impl CrossStageCatalog {
    pub fn new(
        candidates: Vec<Vec<CrossStageRole>>,
        completion: CrossStageCatalogCompletion,
    ) -> Self {
        Self {
            candidates,
            completion,
        }
    }
    pub fn candidates(&self) -> &[Vec<CrossStageRole>] {
        &self.candidates
    }
    pub const fn completion(&self) -> CrossStageCatalogCompletion {
        self.completion
    }
}

/// The common executed problem. Source-stage sizes and placement-stage sizes
/// are deliberately independent; a source token may fill the other stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossStageExecution {
    pub initial_board: Board256Mask,
    pub stage_one_target: Board256Mask,
    pub final_board: Board256Mask,
    pub height: u8,
    pub stage_one_supply: Vec<PieceKind>,
    pub stage_two_supply: Vec<PieceKind>,
    pub early_limit: CrossStageEarlyLimit,
    pub hold_enabled: bool,
    pub rule_profile: RuleProfileId,
    pub spin_profile: SpinProfileId,
    pub initial_b2b: bool,
    pub preserve_b2b: bool,
}

impl CrossStageExecution {
    fn pair(&self, first: &[CrossStageRole], second: &[CrossStageRole]) -> CrossStageRecoveryQuery {
        CrossStageRecoveryQuery {
            initial_board: self.initial_board,
            stage_one_target: self.stage_one_target,
            final_board: self.final_board,
            height: self.height,
            stage_one_supply: self.stage_one_supply.clone(),
            stage_two_supply: self.stage_two_supply.clone(),
            stage_one_roles: first.to_vec(),
            stage_two_roles: second.to_vec(),
            early_limit: self.early_limit,
            hold_enabled: self.hold_enabled,
            rule_profile: self.rule_profile,
            spin_profile: self.spin_profile,
            initial_b2b: self.initial_b2b,
            preserve_b2b: self.preserve_b2b,
        }
    }

    fn validate(&self) -> Result<(), CrossStageSearchError> {
        if self.height == 0 || self.height > 25 {
            return Err(CrossStageSearchError::InvalidHeight);
        }
        if self.stage_one_supply.is_empty() || self.stage_two_supply.is_empty() {
            return Err(CrossStageSearchError::EmptyStage);
        }
        self.stage_one_supply
            .len()
            .checked_add(self.stage_two_supply.len())
            .ok_or(CrossStageSearchError::SizeOverflow)?;
        let cells = u16::from(self.height) * 10;
        if [self.initial_board, self.stage_one_target, self.final_board]
            .iter()
            .any(|mask| mask.fits_cell_count(cells) != Ok(true))
        {
            return Err(CrossStageSearchError::BoardOutsideField);
        }
        ReachabilityWorkspace::new(self.height, self.rule_profile)
            .map_err(|_| CrossStageSearchError::UnsupportedRuleProfile)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CrossStageCatalogStatus {
    Normal,
    AdditionalRecovery,
    /// A real recovery witness, but omitted candidates may admit a normal path.
    RecoveryWithoutNormalExclusion,
    NoPathWithinCatalogs,
    IncompleteCatalogs,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossStageCatalogWitness {
    pub stage_one_candidate: usize,
    pub stage_two_candidate: usize,
    pub path: CrossStagePairReport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossStageCatalogReport {
    pub status: CrossStageCatalogStatus,
    pub witness: Option<CrossStageCatalogWitness>,
    pub normal_pairs_checked: usize,
    pub recovery_pairs_checked: usize,
    pub normal_states: usize,
    pub recovery_states: usize,
    pub catalogs_exhausted: bool,
    pub normal_exclusion_proven: bool,
    /// False for witness returns. This API proves existence, not all-path listing.
    pub declared_scope_exhausted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CrossStageCatalogError {
    Execution(CrossStageSearchError),
    InvalidCandidate { stage: u8, index: usize },
    CounterOverflow,
}

impl From<CrossStageSearchError> for CrossStageCatalogError {
    fn from(error: CrossStageSearchError) -> Self {
        Self::Execution(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossStageCatalogQuery {
    pub execution: CrossStageExecution,
    pub stage_one: CrossStageCatalog,
    pub stage_two: CrossStageCatalog,
}

impl CrossStageCatalogQuery {
    pub fn search(
        &self,
        control: &ExecutionControl,
    ) -> Result<CrossStageCatalogReport, CrossStageCatalogError> {
        self.execution.validate()?;
        for (stage, catalog) in [(1, &self.stage_one), (2, &self.stage_two)] {
            for (index, candidate) in catalog.candidates.iter().enumerate() {
                cancelled(control)?;
                if candidate.is_empty()
                    || candidate.iter().any(|role| {
                        role.lock_mask
                            .fits_cell_count(u16::from(self.execution.height) * 10)
                            != Ok(true)
                            || role
                                .lock_mask
                                .words()
                                .iter()
                                .map(|word| word.count_ones())
                                .sum::<u32>()
                                != 4
                    })
                {
                    return Err(CrossStageCatalogError::InvalidCandidate { stage, index });
                }
            }
        }
        cancelled(control)?;
        let exhausted = self.stage_one.completion == CrossStageCatalogCompletion::Exhausted
            && self.stage_two.completion == CrossStageCatalogCompletion::Exhausted;
        let mut report = CrossStageCatalogReport {
            status: CrossStageCatalogStatus::IncompleteCatalogs,
            witness: None,
            normal_pairs_checked: 0,
            recovery_pairs_checked: 0,
            normal_states: 0,
            recovery_states: 0,
            catalogs_exhausted: exhausted,
            normal_exclusion_proven: false,
            declared_scope_exhausted: false,
        };
        // Never materialize the quadratic product or join coverage by multiplying
        // per-stage percentages. Every combination is replayed on the SAME board.
        for recovery in [false, true] {
            for (first_index, first) in self.stage_one.candidates.iter().enumerate() {
                for (second_index, second) in self.stage_two.candidates.iter().enumerate() {
                    cancelled(control)?;
                    let query = self.execution.pair(first, second);
                    query.validate()?;
                    let effective = query
                        .early_limit
                        .effective_max(query.stage_one_supply.len(), second.len());
                    if recovery && effective == 0 {
                        continue;
                    }
                    let (found, states) =
                        query.run_pass(control, if recovery { effective } else { 0 })?;
                    let (state_total, pair_total) = if recovery {
                        (
                            &mut report.recovery_states,
                            &mut report.recovery_pairs_checked,
                        )
                    } else {
                        (&mut report.normal_states, &mut report.normal_pairs_checked)
                    };
                    *state_total = state_total
                        .checked_add(states)
                        .ok_or(CrossStageCatalogError::CounterOverflow)?;
                    *pair_total = pair_total
                        .checked_add(1)
                        .ok_or(CrossStageCatalogError::CounterOverflow)?;
                    if let Some((steps, actual)) = found {
                        report.status = if !recovery {
                            CrossStageCatalogStatus::Normal
                        } else if report.normal_exclusion_proven {
                            CrossStageCatalogStatus::AdditionalRecovery
                        } else {
                            CrossStageCatalogStatus::RecoveryWithoutNormalExclusion
                        };
                        report.witness = Some(CrossStageCatalogWitness {
                            stage_one_candidate: first_index,
                            stage_two_candidate: second_index,
                            path: query.report(
                                if recovery {
                                    CrossStagePairStatus::Recovery
                                } else {
                                    CrossStagePairStatus::Normal
                                },
                                effective,
                                actual,
                                if recovery { 0 } else { states },
                                if recovery { states } else { 0 },
                                steps,
                            ),
                        });
                        return Ok(report);
                    }
                }
            }
            if !recovery {
                report.normal_exclusion_proven = exhausted;
            }
        }
        report.declared_scope_exhausted = exhausted;
        report.status = if exhausted {
            CrossStageCatalogStatus::NoPathWithinCatalogs
        } else {
            CrossStageCatalogStatus::IncompleteCatalogs
        };
        Ok(report)
    }
}

fn cancelled(control: &ExecutionControl) -> Result<(), CrossStageCatalogError> {
    if control.is_cancelled() {
        Err(CrossStageSearchError::Cancelled.into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "cross_stage_catalog_tests.rs"]
mod tests;
