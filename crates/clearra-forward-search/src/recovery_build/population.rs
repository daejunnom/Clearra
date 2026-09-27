//! Two separately parsed canonical supply universes. Their Cartesian product
//! is traversed lazily and never truncated, sampled, or counted twice per path.
use super::{
    RecoveryBuildError, RecoveryBuildFields, RecoveryBuildFixedQuery, RecoveryBuildFixedReport,
    RecoveryBuildStatus,
};
use crate::CrossStageEarlyLimit;
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;
use clearra_supply::{
    pattern_universe::pattern_universe_materializer::PatternUniverseMaterializer,
    queue::queue_pattern_expression::QueuePatternExpression,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryBuildQuery {
    pub fields: RecoveryBuildFields,
    pub first_supply: String,
    pub second_supply: String,
    pub early_limit: CrossStageEarlyLimit,
    pub allow_piece_exchange: bool,
    pub hold_enabled: bool,
    pub preserve_b2b: bool,
    pub initial_b2b: bool,
    pub rule_profile: RuleProfileId,
    pub spin_profile: SpinProfileId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryBuildExample {
    pub first_pattern: usize,
    pub second_pattern: usize,
    pub first_queue: Vec<PieceKind>,
    pub second_queue: Vec<PieceKind>,
    pub path: RecoveryBuildFixedReport,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryBuildPopulation {
    pub possible: u128,
    pub evaluated: u128,
    pub normal_count: u128,
    pub recovery_count: u128,
    pub no_path_count: u128,
    pub states: u128,
    pub normal_probability: f64,
    pub recovery_probability: f64,
    pub no_path_probability: f64,
    /// Representative examples only, never an exhaustive geometry/page set.
    pub normal_example: Option<RecoveryBuildExample>,
    pub recovery_example: Option<RecoveryBuildExample>,
}
#[derive(Default)]
pub(super) struct Sum {
    pub(super) value: f64,
    correction: f64,
}
impl Sum {
    pub(super) fn add(&mut self, value: f64) {
        let adjusted = value - self.correction;
        let next = self.value + adjusted;
        self.correction = (next - self.value) - adjusted;
        self.value = next;
    }
}
impl RecoveryBuildQuery {
    pub fn validate(&self) -> Result<(), RecoveryBuildError> {
        self.fields.prepare()?;
        for input in [&self.first_supply, &self.second_supply] {
            let parsed = QueuePatternExpression::parse(input, 0)
                .map_err(|_| RecoveryBuildError::InvalidSupplyPattern)?;
            if parsed.sequence_len() == 0 {
                return Err(RecoveryBuildError::EmptySupply);
            }
        }
        Ok(())
    }
    pub fn search(
        &self,
        control: &ExecutionControl,
    ) -> Result<RecoveryBuildPopulation, RecoveryBuildError> {
        self.validate()?;
        if control.is_cancelled() {
            return Err(RecoveryBuildError::Cancelled);
        }
        let first = QueuePatternExpression::parse(&self.first_supply, 0)
            .map_err(|_| RecoveryBuildError::InvalidSupplyPattern)?;
        let second = QueuePatternExpression::parse(&self.second_supply, 0)
            .map_err(|_| RecoveryBuildError::InvalidSupplyPattern)?;
        let first = PatternUniverseMaterializer::queue_pattern_expression(&first, 0)
            .map_err(|_| RecoveryBuildError::PatternDomainUnavailable)?;
        let second = PatternUniverseMaterializer::queue_pattern_expression(&second, 0)
            .map_err(|_| RecoveryBuildError::PatternDomainUnavailable)?;
        let possible = (first.pattern_count() as u128)
            .checked_mul(second.pattern_count() as u128)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        let mut report = RecoveryBuildPopulation {
            possible,
            evaluated: 0,
            normal_count: 0,
            recovery_count: 0,
            no_path_count: 0,
            states: 0,
            normal_probability: 0.0,
            recovery_probability: 0.0,
            no_path_probability: 0.0,
            normal_example: None,
            recovery_example: None,
        };
        let mut probabilities = [Sum::default(), Sum::default(), Sum::default()];
        // Public progress numbers must remain exact in JavaScript. An enormous
        // universe remains countable internally; no approximate total is sent.
        let public_total = (possible <= 9_007_199_254_740_991).then_some(possible as u64);
        control.report_progress("recovery-build", 0, public_total);
        for i in 0..first.pattern_count() {
            let a = first.sequence_at(i);
            for j in 0..second.pattern_count() {
                if control.is_cancelled() {
                    return Err(RecoveryBuildError::Cancelled);
                }
                let b = second.sequence_at(j);
                let query = RecoveryBuildFixedQuery {
                    fields: self.fields.clone(),
                    first_supply: a.to_vec(),
                    second_supply: b.to_vec(),
                    early_limit: self.early_limit,
                    allow_piece_exchange: self.allow_piece_exchange,
                    hold_enabled: self.hold_enabled,
                    preserve_b2b: self.preserve_b2b,
                    initial_b2b: self.initial_b2b,
                    rule_profile: self.rule_profile,
                    spin_profile: self.spin_profile,
                };
                let path = query.search(control)?;
                report.evaluated += 1;
                report.states = report
                    .states
                    .checked_add(path.states as u128)
                    .ok_or(RecoveryBuildError::CounterOverflow)?;
                let category = match path.status {
                    RecoveryBuildStatus::Normal => 0,
                    RecoveryBuildStatus::Recovery => 1,
                    RecoveryBuildStatus::NoPath => 2,
                };
                probabilities[category].add(first.weight_at(i).get() * second.weight_at(j).get());
                if (report.evaluated % 256 == 0 || report.evaluated == possible)
                    && report.evaluated <= 9_007_199_254_740_991
                {
                    control.report_progress(
                        "recovery-build",
                        report.evaluated as u64,
                        public_total,
                    );
                }
                match category {
                    0 => {
                        report.normal_count += 1;
                        if report.normal_example.is_none() {
                            report.normal_example = Some(RecoveryBuildExample {
                                first_pattern: i,
                                second_pattern: j,
                                first_queue: a.to_vec(),
                                second_queue: b.to_vec(),
                                path,
                            });
                        }
                    }
                    1 => {
                        report.recovery_count += 1;
                        if report.recovery_example.is_none() {
                            report.recovery_example = Some(RecoveryBuildExample {
                                first_pattern: i,
                                second_pattern: j,
                                first_queue: a.to_vec(),
                                second_queue: b.to_vec(),
                                path,
                            });
                        }
                    }
                    _ => report.no_path_count += 1,
                }
            }
        }
        control.report_progress("postprocess", 0, None);
        report.normal_probability = probabilities[0].value.clamp(0.0, 1.0);
        report.recovery_probability = probabilities[1].value.clamp(0.0, 1.0);
        report.no_path_probability = probabilities[2].value.clamp(0.0, 1.0);
        Ok(report)
    }
}
