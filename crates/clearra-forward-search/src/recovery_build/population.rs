//! Two separately parsed canonical supply universes. Stage coverage languages
//! count their product without traversing all pairs or counting paths twice.
use super::{RecoveryBuildError, RecoveryBuildFields, RecoveryBuildFixedReport};
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
    /// Complete common-frame target/supply chain. Empty preserves the paired API.
    pub stages: Vec<super::RecoveryBuildStage>,
    pub all_solutions: bool,
    pub minimum_solutions: bool,
    pub required_solution_keys: Vec<String>,
    pub minimum_source_identity: Option<String>,
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
pub struct RecoveryBuildSolution {
    /// Exact logical placement identity, not a representative queue identity.
    pub key: String,
    pub covered_count: u128,
    pub probability: f64,
    pub example: RecoveryBuildExample,
}
#[derive(Clone, Debug, PartialEq)]
pub struct RecoveryBuildPopulation {
    pub solutions: Vec<RecoveryBuildSolution>,
    pub solutions_complete: bool,
    /// Each class is a distinct nonempty set of solutions accepting an input.
    /// It is exact for minimum cover, not a probability weight or sampled queue.
    pub coverage_classes: Option<Vec<Vec<usize>>>,
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
        if self.stages.is_empty() {
            self.fields.prepare()?;
        } else {
            super::chain::validate(self)?;
        }
        if (self.minimum_solutions && !self.all_solutions)
            || (!self.required_solution_keys.is_empty() && !self.minimum_solutions)
            || self.required_solution_keys.iter().any(|k| k.is_empty())
            || self
                .required_solution_keys
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.required_solution_keys.len()
        {
            return Err(RecoveryBuildError::InvalidSupplyPattern);
        }

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
        super::parallel::search_serial(self.clone(), control).map_err(|error| match error {
            super::RecoveryBuildParallelError::Search(error) => error,
            _ => RecoveryBuildError::PatternDomainUnavailable,
        })
    }
}

/// Two independent universes are retained; the Cartesian product is an index,
/// never an allocated list of pairs. Serial and parallel use this same kernel.
pub(super) struct PreparedPopulation {
    pub query: RecoveryBuildQuery,
    pub first: clearra_supply::pattern_universe::MaterializedPatternUniverse,
    pub second: clearra_supply::pattern_universe::MaterializedPatternUniverse,
    pub possible: u128,
}
impl PreparedPopulation {
    pub fn new(query: RecoveryBuildQuery) -> Result<Self, RecoveryBuildError> {
        query.validate()?;
        if !query.stages.is_empty() {
            return Err(RecoveryBuildError::InvalidSupplyPattern);
        }
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
        Ok(Self {
            query,
            first,
            second,
            possible,
        })
    }
    pub fn progress(&self, completed: u128, control: &ExecutionControl) {
        const MAX_EXACT: u128 = 9_007_199_254_740_991;
        if completed <= MAX_EXACT {
            control.report_progress(
                "recovery-build",
                completed as u64,
                (self.possible <= MAX_EXACT).then_some(self.possible as u64),
            );
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
                solutions: Vec::new(),
                solutions_complete: false,
                coverage_classes: None,
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
            },
            probabilities: [Sum::default(), Sum::default(), Sum::default()],
        }
    }
    /// Merge certified, disjoint first-source ranges in rank order. Alternative
    /// paths and mirrored targets have already been unioned inside each range.
    pub fn record_block(
        &mut self,
        source: &PreparedPopulation,
        start: usize,
        count: usize,
        block: super::staged::BlockResult,
    ) -> Result<(), RecoveryBuildError> {
        let width = source.second.pattern_count() as u128;
        if start as u128 * width != self.report.evaluated
            || block
                .counts
                .iter()
                .try_fold(0_u128, |sum, n| sum.checked_add(*n))
                != Some(count as u128 * width)
        {
            return Err(RecoveryBuildError::PatternDomainUnavailable);
        }
        self.report.evaluated += count as u128 * width;
        self.report.states = self
            .report
            .states
            .checked_add(block.states)
            .ok_or(RecoveryBuildError::CounterOverflow)?;
        self.report.normal_count += block.counts[0];
        self.report.recovery_count += block.counts[1];
        self.report.no_path_count += block.counts[2];
        for (sum, value) in self.probabilities.iter_mut().zip(block.probabilities) {
            sum.add(value);
        }
        if self.report.normal_example.is_none() {
            self.report.normal_example = block.normal;
        }
        if self.report.recovery_example.is_none() {
            self.report.recovery_example = block.recovery;
        }
        Ok(())
    }
    pub fn finish(mut self) -> RecoveryBuildPopulation {
        self.report.normal_probability = self.probabilities[0].value.clamp(0.0, 1.0);
        self.report.recovery_probability = self.probabilities[1].value.clamp(0.0, 1.0);
        self.report.no_path_probability = self.probabilities[2].value.clamp(0.0, 1.0);
        self.report
    }
}
