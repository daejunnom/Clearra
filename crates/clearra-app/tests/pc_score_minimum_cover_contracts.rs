use std::sync::Arc;

use clearra_core_domain::solution::normalized_tiling_solution::{
    NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
};

pub use clearra_app::{CoveragePortfolioAlternativeSet, PortfolioAlternativeSetIdentity};

pub const PC_SCORE_MAX_PATTERNS: usize = 1_066_867_200;
pub const PC_SCORE_CANONICAL_SELECTION: &str = "smallest-canonical-candidate-id";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcScoreIngressOrigin {
    CanonicalPcScore,
    CompatibilityScore,
}

impl PcScoreIngressOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalPcScore => "canonical-pc-score",
            Self::CompatibilityScore => "compatibility-score",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcScoreProblemPreset {
    OpeningPc,
    ScenarioPc,
}

impl PcScoreProblemPreset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OpeningPc => "opening-pc",
            Self::ScenarioPc => "scenario-pc",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcScoreQuerySnapshot {
    marker: Arc<str>,
}

impl PcScoreQuerySnapshot {
    fn fixture(marker: &str) -> Self {
        Self {
            marker: Arc::from(marker),
        }
    }

    pub(crate) fn checked_pointee_retained_bytes(&self) -> Option<u128> {
        Some(self.marker.len() as u128)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoreProfileSelection {
    Guideline,
}

impl ScoreProfileSelection {
    pub const fn as_str(self) -> &'static str {
        "guideline"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinProfileSelection {
    Guideline,
}

impl SpinProfileSelection {
    pub const fn as_str(self) -> &'static str {
        "guideline"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcScoreCompletenessEvidence {
    source_universe_complete: bool,
    execution_source_complete: bool,
    objective_complete: bool,
    count_complete: bool,
    probability_complete: bool,
    resource_probability_complete: bool,
    matrix_complete: bool,
    summary_complete: bool,
}

impl PcScoreCompletenessEvidence {
    fn complete_fixture() -> Self {
        Self {
            source_universe_complete: true,
            execution_source_complete: true,
            objective_complete: true,
            count_complete: true,
            probability_complete: true,
            resource_probability_complete: true,
            matrix_complete: true,
            summary_complete: true,
        }
    }

    pub const fn source_universe_complete(self) -> bool {
        self.source_universe_complete
    }

    pub const fn execution_source_complete(self) -> bool {
        self.execution_source_complete
    }

    pub const fn objective_complete(self) -> bool {
        self.objective_complete
    }

    pub const fn count_complete(self) -> bool {
        self.count_complete
    }

    pub const fn probability_complete(self) -> bool {
        self.probability_complete
    }

    pub const fn resource_probability_complete(self) -> bool {
        self.resource_probability_complete
    }

    pub const fn matrix_complete(self) -> bool {
        self.matrix_complete
    }

    pub const fn summary_complete(self) -> bool {
        self.summary_complete
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcScorePatternWinnerV1 {
    pattern_id: usize,
    candidate_id: u64,
    solution_identity: StandardBoard64TilingIdentity,
    score: u64,
    informational_attack: u32,
}

impl PcScorePatternWinnerV1 {
    fn fixture(
        pattern_id: usize,
        candidate_id: u64,
        score: u64,
        informational_attack: u32,
    ) -> Self {
        Self {
            pattern_id,
            candidate_id,
            solution_identity: identity(candidate_id),
            score,
            informational_attack,
        }
    }

    pub const fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub const fn solution_identity(&self) -> StandardBoard64TilingIdentity {
        self.solution_identity
    }

    pub const fn score(&self) -> u64 {
        self.score
    }

    pub const fn informational_attack(&self) -> u32 {
        self.informational_attack
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcScoreSummaryV2Result {
    contract_id: &'static str,
    origin: PcScoreIngressOrigin,
    query: PcScoreQuerySnapshot,
    problem_preset: PcScoreProblemPreset,
    problem_id: Arc<str>,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    score_profile_selection: ScoreProfileSelection,
    spin_profile_selection: SpinProfileSelection,
    initial_b2b: u32,
    score_profile_id: Arc<str>,
    materialized_pattern_count: usize,
    total_pattern_count: u128,
    matrix_cell_count: usize,
    all_universe_patterns_covered: bool,
    pattern_optimal_count: usize,
    failed_pc_pattern_count: usize,
    best_score: Option<u64>,
    pattern_winners: Arc<Vec<PcScorePatternWinnerV1>>,
    completeness: PcScoreCompletenessEvidence,
}

pub mod pc_score_summary_result {
    use clearra_core_executor::CoreExecutionResult;

    use super::PcScoreSummaryV2Result;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub(crate) struct ValidatedPcScoreExecutionEvidence {
        report: PcScoreSummaryV2Result,
    }

    impl ValidatedPcScoreExecutionEvidence {
        pub(crate) fn report(&self) -> &PcScoreSummaryV2Result {
            &self.report
        }

        pub(crate) fn matches_core_result(&self, _result: &CoreExecutionResult) -> bool {
            true
        }
    }
}

impl PcScoreSummaryV2Result {
    pub const fn contract_id(&self) -> &'static str {
        self.contract_id
    }

    pub const fn origin(&self) -> PcScoreIngressOrigin {
        self.origin
    }

    pub const fn query(&self) -> &PcScoreQuerySnapshot {
        &self.query
    }

    pub const fn problem_preset(&self) -> PcScoreProblemPreset {
        self.problem_preset
    }

    pub fn problem_id(&self) -> &str {
        self.problem_id.as_ref()
    }

    pub const fn piece_source_id(&self) -> u64 {
        self.piece_source_id
    }

    pub const fn pattern_universe_id(&self) -> u64 {
        self.pattern_universe_id
    }

    pub const fn pattern_weight_model_id(&self) -> u64 {
        self.pattern_weight_model_id
    }

    pub const fn score_profile_selection(&self) -> ScoreProfileSelection {
        self.score_profile_selection
    }

    pub const fn spin_profile_selection(&self) -> SpinProfileSelection {
        self.spin_profile_selection
    }

    pub const fn initial_b2b(&self) -> u32 {
        self.initial_b2b
    }

    pub fn score_profile_id(&self) -> &str {
        self.score_profile_id.as_ref()
    }

    pub const fn materialized_pattern_count(&self) -> usize {
        self.materialized_pattern_count
    }

    pub const fn total_pattern_count(&self) -> u128 {
        self.total_pattern_count
    }

    pub const fn matrix_cell_count(&self) -> usize {
        self.matrix_cell_count
    }

    pub const fn all_universe_patterns_covered(&self) -> bool {
        self.all_universe_patterns_covered
    }

    pub const fn pattern_optimal_count(&self) -> usize {
        self.pattern_optimal_count
    }

    pub const fn failed_pc_pattern_count(&self) -> usize {
        self.failed_pc_pattern_count
    }

    pub const fn best_score(&self) -> Option<u64> {
        self.best_score
    }

    pub fn pattern_winners(&self) -> &[PcScorePatternWinnerV1] {
        self.pattern_winners.as_slice()
    }

    pub fn pattern_winner_count(&self) -> usize {
        self.pattern_winners.len()
    }

    pub const fn completeness(&self) -> PcScoreCompletenessEvidence {
        self.completeness
    }
}

pub mod pc_score_postprocess {
    use std::sync::Arc;

    use super::PcScorePatternWinnerV1;

    #[derive(Clone, Debug)]
    pub(crate) struct PcScoreDerivation {
        execution_source_complete: bool,
        pattern_winners: Arc<Vec<PcScorePatternWinnerV1>>,
    }

    impl PcScoreDerivation {
        pub(crate) fn fixture(
            execution_source_complete: bool,
            pattern_winners: Arc<Vec<PcScorePatternWinnerV1>>,
        ) -> Self {
            Self {
                execution_source_complete,
                pattern_winners,
            }
        }

        pub(crate) const fn execution_source_complete(&self) -> bool {
            self.execution_source_complete
        }

        pub(crate) fn pattern_winners(&self) -> &[PcScorePatternWinnerV1] {
            self.pattern_winners.as_slice()
        }

        pub(crate) fn pattern_winner_owner(&self) -> &Arc<Vec<PcScorePatternWinnerV1>> {
            &self.pattern_winners
        }
    }
}

#[path = "../src/pc_score_minimum_cover_result.rs"]
mod pc_score_minimum_cover_result;

use clearra_app::PortfolioEnumerationStop;
use pc_score_minimum_cover_result::{
    validate_pc_score_portfolio_v2_result, PcScorePortfolioValidationError,
    PC_SCORE_PORTFOLIO_RESULT_CONTRACT,
};
use pc_score_postprocess::PcScoreDerivation;

fn identity(candidate_id: u64) -> StandardBoard64TilingIdentity {
    StandardBoard64TilingIdentity::from_compact_parts(candidate_id, 0, &[])
        .expect("fixture identity")
}

fn two_by_two_winners(attacks: [u32; 4]) -> Vec<PcScorePatternWinnerV1> {
    vec![
        PcScorePatternWinnerV1::fixture(0, 1, 100, attacks[0]),
        PcScorePatternWinnerV1::fixture(0, 2, 100, attacks[1]),
        PcScorePatternWinnerV1::fixture(1, 3, 200, attacks[2]),
        PcScorePatternWinnerV1::fixture(1, 4, 200, attacks[3]),
    ]
}

fn authority_fixture(
    summary_winners: Vec<PcScorePatternWinnerV1>,
    derivation_winners: Vec<PcScorePatternWinnerV1>,
    pattern_count: usize,
) -> (PcScoreSummaryV2Result, PcScoreDerivation) {
    let best_score = summary_winners
        .iter()
        .map(PcScorePatternWinnerV1::score)
        .max();
    let summary_winners = Arc::new(summary_winners);
    let summary = PcScoreSummaryV2Result {
        contract_id: "pc-score-summary.v2",
        origin: PcScoreIngressOrigin::CanonicalPcScore,
        query: PcScoreQuerySnapshot::fixture("query-a"),
        problem_preset: PcScoreProblemPreset::OpeningPc,
        problem_id: Arc::from("problem-a"),
        piece_source_id: 11,
        pattern_universe_id: 22,
        pattern_weight_model_id: 33,
        score_profile_selection: ScoreProfileSelection::Guideline,
        spin_profile_selection: SpinProfileSelection::Guideline,
        initial_b2b: 0,
        score_profile_id: Arc::from("guideline-v1"),
        materialized_pattern_count: pattern_count,
        total_pattern_count: pattern_count as u128,
        matrix_cell_count: summary_winners.len(),
        all_universe_patterns_covered: true,
        pattern_optimal_count: pattern_count,
        failed_pc_pattern_count: 0,
        best_score,
        pattern_winners: summary_winners,
        completeness: PcScoreCompletenessEvidence::complete_fixture(),
    };
    let derivation = PcScoreDerivation::fixture(true, Arc::new(derivation_winners));
    (summary, derivation)
}

#[test]
fn b_option_keeps_every_score_tie_and_enumerates_every_minimum_cover() {
    let winners = two_by_two_winners([900, 1, 700, 3]);
    let mut reordered = winners.clone();
    reordered.reverse();
    let (summary, derivation) = authority_fixture(winners, reordered, 2);

    let result =
        validate_pc_score_portfolio_v2_result(&summary, &derivation).expect("typed portfolio");
    assert_eq!(result.contract_id(), PC_SCORE_PORTFOLIO_RESULT_CONTRACT);
    assert!(result.completeness().complete());
    assert_eq!(result.pattern_best_scores(), &[100, 200]);
    assert_eq!(
        result
            .eligible_candidates()
            .iter()
            .map(|candidate| candidate.score_candidate_id())
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    assert_eq!(result.selected_score_candidate_ids(), &[1, 3]);
    assert_eq!(result.canonical_score_candidate_id(), 1);
    assert_eq!(
        result.canonical_solution_key().to_string(),
        NormalizedTilingSolutionKey::from_standard_board64_identity(identity(1)).to_string()
    );
    assert_eq!(
        result
            .portfolio_alternatives()
            .canonical_page()
            .portfolio()
            .candidate_ids(),
        &[1, 3]
    );
    assert_eq!(result.portfolio_alternatives().optimal_cardinality(), 2);

    let mut portfolios = vec![vec![1, 3]];
    let mut store = result
        .portfolio_alternatives()
        .open_store()
        .expect("lazy exact store");
    loop {
        let advance = store
            .next_page(u64::MAX, &mut || false)
            .expect("next exact alternative");
        if let Some(page) = advance.page() {
            portfolios.push(page.portfolio().candidate_ids().to_vec());
        }
        if advance.stop() == PortfolioEnumerationStop::Sealed
            || advance.checkpoint().enumeration_complete()
        {
            break;
        }
    }
    assert_eq!(portfolios, [vec![1, 3], vec![1, 4], vec![2, 3], vec![2, 4]]);
}

#[test]
fn attacks_and_input_row_order_do_not_change_eligibility_order_or_identity() {
    let left_winners = two_by_two_winners([0, u32::MAX, 8, 99]);
    let right_winners = two_by_two_winners([999, 1, u32::MAX, 0]);
    let mut left_derivation = left_winners.clone();
    left_derivation.rotate_left(1);
    let mut right_derivation = right_winners.clone();
    right_derivation.reverse();
    let (left_summary, left_derivation) = authority_fixture(left_winners, left_derivation, 2);
    let (right_summary, right_derivation) = authority_fixture(right_winners, right_derivation, 2);

    let left = validate_pc_score_portfolio_v2_result(&left_summary, &left_derivation)
        .expect("left portfolio");
    let right = validate_pc_score_portfolio_v2_result(&right_summary, &right_derivation)
        .expect("right portfolio");

    assert_ne!(
        left.pattern_winners()[0].informational_attack(),
        right.pattern_winners()[0].informational_attack()
    );
    assert_eq!(
        left.eligible_candidate_map_sha256(),
        right.eligible_candidate_map_sha256()
    );
    assert_eq!(
        left.score_eligibility_sha256(),
        right.score_eligibility_sha256()
    );
    assert_eq!(
        left.portfolio_alternatives().set_identity_sha256(),
        right.portfolio_alternatives().set_identity_sha256()
    );
    assert_eq!(
        left.selected_score_candidate_ids(),
        right.selected_score_candidate_ids()
    );
}

#[test]
fn set_identity_binds_query_source_profile_universe_rows_and_product_build() {
    let winners = two_by_two_winners([4, 3, 2, 1]);
    let (summary, derivation) = authority_fixture(winners.clone(), winners, 2);
    let baseline =
        validate_pc_score_portfolio_v2_result(&summary, &derivation).expect("baseline identity");
    let identity = baseline.portfolio_alternatives().identity();
    let product_build = clearra_host_contract::ProductBuildIdentity::current();
    assert!(identity
        .query_identity()
        .starts_with("pc-score-minimals-query.v2"));
    assert!(identity.source_identity().contains("pc-score-source.v2"));
    assert!(identity.profile_identity().contains("pc-score-profile.v2"));
    assert!(identity
        .universe_identity()
        .contains(baseline.score_eligibility_sha256()));
    for field in [
        product_build.engine_build_id(),
        product_build.source_commit(),
        product_build.contract_schema_version(),
        product_build.supply_semantics_id(),
        product_build.artifact_schema_version(),
    ] {
        assert!(identity.build_identity().contains(field));
    }

    let mut changed_query = summary.clone();
    changed_query.problem_id = Arc::from("problem-b");
    let changed_query = validate_pc_score_portfolio_v2_result(&changed_query, &derivation)
        .expect("changed query identity");
    assert_ne!(
        baseline.portfolio_alternatives().set_identity_sha256(),
        changed_query.portfolio_alternatives().set_identity_sha256()
    );

    let mut changed_profile = summary.clone();
    changed_profile.score_profile_id = Arc::from("guideline-v2");
    let changed_profile = validate_pc_score_portfolio_v2_result(&changed_profile, &derivation)
        .expect("changed profile identity");
    assert_ne!(
        baseline.portfolio_alternatives().set_identity_sha256(),
        changed_profile
            .portfolio_alternatives()
            .set_identity_sha256()
    );

    let mut changed_universe = summary;
    changed_universe.pattern_universe_id += 1;
    let changed_universe = validate_pc_score_portfolio_v2_result(&changed_universe, &derivation)
        .expect("changed universe identity");
    assert_ne!(
        baseline.portfolio_alternatives().set_identity_sha256(),
        changed_universe
            .portfolio_alternatives()
            .set_identity_sha256()
    );
}

#[test]
fn score_not_attack_controls_eligibility() {
    let tied = two_by_two_winners([0, u32::MAX, 0, u32::MAX]);
    let reduced = vec![tied[0], tied[2], tied[3]];
    let (tied_summary, tied_derivation) = authority_fixture(tied.clone(), tied, 2);
    let (reduced_summary, reduced_derivation) = authority_fixture(reduced.clone(), reduced, 2);

    let tied = validate_pc_score_portfolio_v2_result(&tied_summary, &tied_derivation)
        .expect("tied score family");
    let reduced = validate_pc_score_portfolio_v2_result(&reduced_summary, &reduced_derivation)
        .expect("lower score candidate omitted from winner family");

    assert_eq!(tied.eligible_candidates().len(), 4);
    assert_eq!(reduced.eligible_candidates().len(), 3);
    assert_ne!(
        tied.score_eligibility_sha256(),
        reduced.score_eligibility_sha256()
    );
    assert_ne!(
        tied.portfolio_alternatives().set_identity_sha256(),
        reduced.portfolio_alternatives().set_identity_sha256()
    );
}

#[test]
fn unequal_scores_inside_one_claimed_winner_family_fail_closed() {
    let mut winners = two_by_two_winners([1, 2, 3, 4]);
    winners[1].score = 99;
    let (summary, derivation) = authority_fixture(winners.clone(), winners, 2);

    assert_eq!(
        validate_pc_score_portfolio_v2_result(&summary, &derivation).unwrap_err(),
        PcScorePortfolioValidationError::WinnerFamilyInvalid
    );
}

#[test]
fn incomplete_weight_replay_and_coverage_authorities_fail_closed() {
    let winners = two_by_two_winners([1, 2, 3, 4]);

    let (mut weight_summary, weight_derivation) =
        authority_fixture(winners.clone(), winners.clone(), 2);
    weight_summary.completeness.probability_complete = false;
    assert_eq!(
        validate_pc_score_portfolio_v2_result(&weight_summary, &weight_derivation).unwrap_err(),
        PcScorePortfolioValidationError::WeightModelIncomplete
    );

    let (replay_summary, _) = authority_fixture(winners.clone(), winners.clone(), 2);
    let replay_derivation = PcScoreDerivation::fixture(false, Arc::new(winners.clone()));
    assert_eq!(
        validate_pc_score_portfolio_v2_result(&replay_summary, &replay_derivation).unwrap_err(),
        PcScorePortfolioValidationError::LegalReplayIncomplete
    );

    let (mut coverage_summary, coverage_derivation) =
        authority_fixture(winners.clone(), winners, 2);
    coverage_summary.all_universe_patterns_covered = false;
    coverage_summary.pattern_optimal_count = 1;
    coverage_summary.failed_pc_pattern_count = 1;
    assert_eq!(
        validate_pc_score_portfolio_v2_result(&coverage_summary, &coverage_derivation).unwrap_err(),
        PcScorePortfolioValidationError::CoverageIncomplete
    );
}

#[test]
fn summary_and_derivation_are_compared_without_attack_but_with_score_and_identity() {
    let summary_winners = two_by_two_winners([1, 2, 3, 4]);
    let mut score_mismatch = summary_winners.clone();
    score_mismatch[0].score += 1;
    let (summary, derivation) = authority_fixture(summary_winners.clone(), score_mismatch, 2);
    assert_eq!(
        validate_pc_score_portfolio_v2_result(&summary, &derivation).unwrap_err(),
        PcScorePortfolioValidationError::WinnerEvidenceMismatch
    );

    let mut identity_mismatch = summary_winners.clone();
    identity_mismatch[0].solution_identity = identity(99);
    let (summary, derivation) = authority_fixture(summary_winners, identity_mismatch, 2);
    assert_eq!(
        validate_pc_score_portfolio_v2_result(&summary, &derivation).unwrap_err(),
        PcScorePortfolioValidationError::WinnerEvidenceMismatch
    );
}

#[test]
fn construction_stays_within_the_default_two_mib_stack_contract() {
    const PATTERN_COUNT: usize = 32;
    let handle = std::thread::Builder::new()
        .name("pc-score-minimals-2mib".to_owned())
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let winners = (0..PATTERN_COUNT)
                .map(|pattern_id| {
                    PcScorePatternWinnerV1::fixture(
                        pattern_id,
                        pattern_id as u64 + 1,
                        pattern_id as u64 + 100,
                        u32::MAX - pattern_id as u32,
                    )
                })
                .collect::<Vec<_>>();
            let (summary, derivation) = authority_fixture(winners.clone(), winners, PATTERN_COUNT);
            let result = validate_pc_score_portfolio_v2_result(&summary, &derivation)
                .expect("2 MiB construction");
            assert_eq!(result.selected_score_candidate_ids().len(), PATTERN_COUNT);
            assert_eq!(
                result.portfolio_alternatives().optimal_cardinality(),
                PATTERN_COUNT
            );
        })
        .expect("spawn 2 MiB thread");
    handle.join().expect("2 MiB thread did not overflow");
}
