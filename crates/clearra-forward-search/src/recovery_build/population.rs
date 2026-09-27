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
struct Sum {
    value: f64,
    correction: f64,
}
impl Sum {
    fn add(&mut self, value: f64) {
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
        if control.is_cancelled() {
            return Err(RecoveryBuildError::Cancelled);
        }
        let prepared = PreparedPopulation::new(self.clone())?;
        let mut accumulator = PopulationAccumulator::new(prepared.possible);
        prepared.progress(0, control);
        for index in 0..prepared.possible {
            let path = prepared.evaluate(index, control)?;
            accumulator.record(&prepared, index, path.status, path.states, Some(path))?;
            if accumulator.report.evaluated % 256 == 0 {
                prepared.progress(accumulator.report.evaluated, control);
            }
        }
        prepared.progress(prepared.possible, control);
        control.report_progress("postprocess", 0, None);
        Ok(accumulator.finish())
    }
}

/// Two independent universes are retained; the Cartesian product is an index,
/// never an allocated list of pairs. Serial and parallel use this same kernel.
pub(super) struct PreparedPopulation {
    pub query: RecoveryBuildQuery,
    first: clearra_supply::pattern_universe::MaterializedPatternUniverse,
    second: clearra_supply::pattern_universe::MaterializedPatternUniverse,
    pub possible: u128,
}
impl PreparedPopulation {
    pub fn new(query: RecoveryBuildQuery) -> Result<Self, RecoveryBuildError> {
        query.validate()?;
        let parse = |source: &str| {
            let expression = QueuePatternExpression::parse(source, 0)
                .map_err(|_| RecoveryBuildError::InvalidSupplyPattern)?;
            PatternUniverseMaterializer::queue_pattern_expression(&expression, 0)
                .map_err(|_| RecoveryBuildError::PatternDomainUnavailable)
        };
        let first = parse(&query.first_supply)?;
        let second = parse(&query.second_supply)?;
        let possible = (first.pattern_count() as u128)
            .checked_mul(second.pattern_count() as u128)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        Ok(Self { query, first, second, possible })
    }
    pub fn indices(&self, index: u128) -> Result<(usize, usize), RecoveryBuildError> {
        if index >= self.possible { return Err(RecoveryBuildError::PatternDomainUnavailable); }
        let width = self.second.pattern_count() as u128;
        Ok(((index / width) as usize, (index % width) as usize))
    }
    pub fn evaluate(&self, index: u128, control: &ExecutionControl)
        -> Result<RecoveryBuildFixedReport, RecoveryBuildError> {
        if control.is_cancelled() { return Err(RecoveryBuildError::Cancelled); }
        let (i, j) = self.indices(index)?;
        RecoveryBuildFixedQuery {
            fields: self.query.fields.clone(),
            first_supply: self.first.sequence_at(i).to_vec(),
            second_supply: self.second.sequence_at(j).to_vec(),
            early_limit: self.query.early_limit,
            allow_piece_exchange: self.query.allow_piece_exchange,
            hold_enabled: self.query.hold_enabled,
            preserve_b2b: self.query.preserve_b2b,
            initial_b2b: self.query.initial_b2b,
            rule_profile: self.query.rule_profile,
            spin_profile: self.query.spin_profile,
        }.search(control)
    }
    pub fn progress(&self, completed: u128, control: &ExecutionControl) {
        const MAX_EXACT: u128 = 9_007_199_254_740_991;
        if completed <= MAX_EXACT {
            control.report_progress("recovery-build", completed as u64,
                (self.possible <= MAX_EXACT).then_some(self.possible as u64));
        }
    }
}

pub(super) struct PopulationAccumulator {
    pub report: RecoveryBuildPopulation,
    probabilities: [Sum; 3],
}
impl PopulationAccumulator {
    pub fn new(possible: u128) -> Self {
        Self {
            report: RecoveryBuildPopulation {
                possible, evaluated: 0, normal_count: 0, recovery_count: 0,
                no_path_count: 0, states: 0, normal_probability: 0.0,
                recovery_probability: 0.0, no_path_probability: 0.0,
                normal_example: None, recovery_example: None,
            },
            probabilities: [Sum::default(), Sum::default(), Sum::default()],
        }
    }
    /// Must be called in global row-major pair order, regardless of completion
    /// order. This preserves both compensated sums and canonical examples.
    pub fn record(&mut self, source: &PreparedPopulation, index: u128,
        status: RecoveryBuildStatus, states: usize, path: Option<RecoveryBuildFixedReport>)
        -> Result<(), RecoveryBuildError> {
        if index != self.report.evaluated {
            return Err(RecoveryBuildError::PatternDomainUnavailable);
        }
        let (i,j) = source.indices(index)?;
        let category = match status {
            RecoveryBuildStatus::Normal => 0,
            RecoveryBuildStatus::Recovery => 1,
            RecoveryBuildStatus::NoPath => 2,
        };
        self.probabilities[category].add(source.first.weight_at(i).get() * source.second.weight_at(j).get());
        self.report.states = self.report.states.checked_add(states as u128)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        self.report.evaluated += 1;
        let example = match status {
            RecoveryBuildStatus::Normal => {
                self.report.normal_count += 1;
                Some(&mut self.report.normal_example)
            },
            RecoveryBuildStatus::Recovery => {
                self.report.recovery_count += 1;
                Some(&mut self.report.recovery_example)
            },
            RecoveryBuildStatus::NoPath => { self.report.no_path_count += 1; None },
        };
        if let Some(slot) = example {
            if slot.is_none() {
                let path = path.ok_or(RecoveryBuildError::PatternDomainUnavailable)?;
                if path.status != status || path.states != states {
                    return Err(RecoveryBuildError::PatternDomainUnavailable);
                }
                *slot = Some(RecoveryBuildExample {
                    first_pattern: i, second_pattern: j,
                    first_queue: source.first.sequence_at(i).to_vec(),
                    second_queue: source.second.sequence_at(j).to_vec(), path,
                });
            }
        }
        Ok(())
    }
    pub fn finish(mut self) -> RecoveryBuildPopulation {
        self.report.normal_probability = self.probabilities[0].value.clamp(0.0,1.0);
        self.report.recovery_probability = self.probabilities[1].value.clamp(0.0,1.0);
        self.report.no_path_probability = self.probabilities[2].value.clamp(0.0,1.0);
        self.report
    }
}
