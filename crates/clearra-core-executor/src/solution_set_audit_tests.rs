use clearra_coverage::{
    cover::minimum_cover_solver::MinimumCoverSolver,
    matrix::{coverage_matrix::CoverageMatrix, coverage_row::CoverageRow},
    pattern::pattern_bitset::PatternBitSet,
};

use super::*;

fn bits(pattern_count: usize, patterns: &[u32]) -> PatternBitSet {
    PatternBitSet::from_pattern_indices(pattern_count, patterns.to_vec())
        .expect("test pattern bitset")
}

fn dimensions(objective: &str) -> SolutionSemanticDimensions {
    SolutionSemanticDimensions::new(
        SolutionProductFamily::Pc,
        objective,
        "guideline-pc",
        "none",
        "disabled",
    )
}

fn candidate(
    key: &str,
    patterns: &[u32],
    dimensions: SolutionSemanticDimensions,
) -> SolutionAuditCandidate {
    SolutionAuditCandidate::new(key, bits(4, patterns), dimensions).expect("test candidate")
}

fn complete_input(
    normalized_keys: Vec<String>,
    candidates: Vec<SolutionAuditCandidate>,
    selection_policy: SolutionPortfolioSelectionPolicy,
) -> SolutionSetAuditInput {
    let count = normalized_keys.len();
    SolutionSetAuditInput::new(
        SolutionProductFamily::Pc,
        bits(4, &[0, 1, 2, 3]),
        selection_policy,
    )
    .with_source_checkpoints(
        SolutionAuditCheckpoint::known(count, "produced-set"),
        SolutionAuditCheckpoint::known(count, "execution-validated-set"),
        SolutionAuditCheckpoint::known(count, "spin-b2b-filtered-set"),
    )
    .with_normalized_keys(normalized_keys, true, Vec::<String>::new())
    .with_candidates(candidates)
}

fn four_class_candidates() -> Vec<SolutionAuditCandidate> {
    vec![
        candidate("solution-a", &[0, 1], dimensions("minimum-cover")),
        candidate("solution-b", &[0, 1], dimensions("minimum-cover")),
        candidate("solution-c", &[2, 3], dimensions("minimum-cover")),
        candidate("solution-d", &[0, 2], dimensions("minimum-cover")),
        candidate("solution-e", &[1, 3], dimensions("minimum-cover")),
    ]
}

#[test]
fn guarded_exact_audit_accepts_exact_observed_peak_and_rejects_peak_minus_one() {
    let input = complete_input(
        vec![
            "solution-a".to_owned(),
            "solution-b".to_owned(),
            "solution-c".to_owned(),
            "solution-d".to_owned(),
            "solution-e".to_owned(),
        ],
        four_class_candidates(),
        SolutionPortfolioSelectionPolicy::ExactMinimumCover,
    );
    let mut peak = 0_u128;
    let (expected, _) =
        SolutionSetAuditReport::analyze_with_memory_guard(input.clone(), &mut |owned_bytes| {
            peak = peak.max(owned_bytes);
            Ok::<_, ()>(())
        })
        .expect("dry guarded audit");
    assert!(peak > 0);

    let input_bytes = input
        .checked_nested_retained_bytes()
        .expect("input retained bytes");
    let exact_cap = input_bytes.checked_add(peak).expect("exact cap");
    let (actual, _) =
        SolutionSetAuditReport::analyze_with_memory_limit(input.clone(), input_bytes, exact_cap)
            .expect("exact observed cap");
    assert_eq!(actual, expected);

    assert!(matches!(
        SolutionSetAuditReport::analyze_with_memory_limit(
            input,
            input_bytes,
            exact_cap - 1,
        ),
        Err(SolutionSetAuditMemoryGuardError::MemoryCapacityExceeded {
            required_memory_bytes,
            max_memory_bytes,
        }) if required_memory_bytes > max_memory_bytes
    ));
}

#[test]
fn guarded_audit_external_addition_overflow_fails_closed() {
    let input = complete_input(
        vec!["solution-a".to_owned()],
        vec![candidate(
            "solution-a",
            &[0, 1, 2, 3],
            dimensions("coverage"),
        )],
        SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives,
    );
    assert!(matches!(
        SolutionSetAuditReport::analyze_with_memory_limit(input, u128::MAX, u128::MAX),
        Err(SolutionSetAuditMemoryGuardError::ProjectionOverflow)
    ));
}

#[test]
fn exact_coverage_and_semantic_dimensions_define_equivalence() {
    let report = SolutionSetAuditReport::analyze(complete_input(
        vec![
            "solution-c".to_owned(),
            "solution-a".to_owned(),
            "solution-b".to_owned(),
        ],
        vec![
            candidate("solution-a", &[0, 1, 2, 3], dimensions("coverage")),
            candidate("solution-b", &[0, 1, 2, 3], dimensions("coverage")),
            candidate("solution-c", &[0, 1, 2, 3], dimensions("minimum-cover")),
        ],
        SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives,
    ))
    .expect("audit report");

    assert_eq!(report.coverage_classes().len(), 2);
    assert_eq!(report.portfolio_families().len(), 2);
    let collapsed = report
        .coverage_classes()
        .iter()
        .find(|class| class.member_keys().len() == 2)
        .expect("equivalent class");
    assert_eq!(collapsed.member_keys(), &["solution-a", "solution-b"]);
    assert_eq!(collapsed.representative_key(), "solution-a");
    assert!(report.complete());
}

#[test]
fn class_family_representative_and_snapshot_order_is_input_order_independent() {
    let candidates = four_class_candidates();
    let keys = candidates
        .iter()
        .map(|candidate| candidate.canonical_key().to_owned())
        .collect::<Vec<_>>();
    let expected = SolutionSetAuditReport::analyze(complete_input(
        keys.clone(),
        candidates.clone(),
        SolutionPortfolioSelectionPolicy::ExactMinimumCover,
    ))
    .expect("canonical audit");

    let actual = SolutionSetAuditReport::analyze(complete_input(
        keys.into_iter().rev().collect(),
        candidates.into_iter().rev().collect(),
        SolutionPortfolioSelectionPolicy::ExactMinimumCover,
    ))
    .expect("shuffled audit");

    assert_eq!(actual, expected);
    assert_eq!(
        actual.coverage_classes()[0].representative_key(),
        "solution-a"
    );
}

#[test]
fn unknown_empty_evidence_never_becomes_empty_complete() {
    let report = SolutionSetAuditReport::unavailable(
        SolutionProductFamily::Pc,
        "normalized-solution-authority-unavailable",
    );

    assert!(!report.complete());
    assert_eq!(report.stages().len(), SolutionSetAuditStageKind::ALL.len());
    assert!(report.stages().iter().all(|stage| !stage.complete()));
    assert!(report.coverage_classes().is_empty());
    assert!(report.portfolio_families().is_empty());
    assert!(!report.portfolio_snapshot().complete());
    let page = report
        .portfolio_snapshot()
        .page(None, 10)
        .expect("empty unavailable page");
    assert!(page.entries().is_empty());
    assert!(!page.complete());
}

#[test]
fn proven_empty_evidence_is_complete_only_when_every_source_identity_is_known() {
    let report = SolutionSetAuditReport::analyze(
        SolutionSetAuditInput::new(
            SolutionProductFamily::Pc,
            PatternBitSet::new(0),
            SolutionPortfolioSelectionPolicy::ExactMinimumCover,
        )
        .with_source_checkpoints(
            SolutionAuditCheckpoint::known(0, "produced-empty"),
            SolutionAuditCheckpoint::known(0, "execution-empty"),
            SolutionAuditCheckpoint::known(0, "filtered-empty"),
        )
        .with_normalized_keys(Vec::new(), true, Vec::<String>::new()),
    )
    .expect("proven empty report");

    assert!(report.complete());
    assert!(report.exact_minimum_proven());
    assert!(report.portfolio_families().is_empty());
    assert!(report
        .portfolio_snapshot()
        .page(None, 1)
        .expect("proven empty page")
        .complete());

    let invalid_identity = SolutionAuditCheckpoint::known(0, "not-calculated");
    assert!(!invalid_identity.complete());
    assert!(invalid_identity.identity_hash().is_none());
}

#[test]
fn product_deferred_exact_audit_never_claims_portfolio_completion_or_runs_the_solver() {
    let candidates = four_class_candidates();
    let keys = candidates
        .iter()
        .map(|candidate| candidate.canonical_key().to_owned())
        .collect::<Vec<_>>();
    let mut guard_calls = Vec::new();
    let (report, _) = SolutionSetAuditReport::analyze_with_memory_guard(
        complete_input(
            keys,
            candidates,
            SolutionPortfolioSelectionPolicy::ProductDeferredExactMinimumCover,
        ),
        &mut |owned_bytes| {
            guard_calls.push(owned_bytes);
            Ok::<_, ()>(())
        },
    )
    .expect("deferred source audit");

    // One call reserves the generic audit projection and one validates the
    // retained report. An exact solver would make additional guarded calls.
    assert_eq!(guard_calls.len(), 2);
    assert!(!report.complete());
    assert!(!report.exact_minimum_proven());
    assert!(!report.portfolio_snapshot().complete());
    assert_eq!(report.coverage_classes().len(), 4);
    assert_eq!(report.portfolio_snapshot().len(), 4);
    assert!(report
        .stage(SolutionSetAuditStageKind::PortfolioSelected)
        .rejection_reasons()
        .iter()
        .any(|reason| {
            reason == "exact-minimum-cover-selection-deferred-to-product-coordinator"
        }));
}

#[test]
fn product_deferred_exact_audit_is_fail_closed_for_empty_or_uncovered_sources() {
    let empty = SolutionSetAuditReport::analyze(
        SolutionSetAuditInput::new(
            SolutionProductFamily::Pc,
            PatternBitSet::new(0),
            SolutionPortfolioSelectionPolicy::ProductDeferredExactMinimumCover,
        )
        .with_source_checkpoints(
            SolutionAuditCheckpoint::known(0, "produced-empty"),
            SolutionAuditCheckpoint::known(0, "execution-empty"),
            SolutionAuditCheckpoint::known(0, "filtered-empty"),
        )
        .with_normalized_keys(Vec::new(), true, Vec::<String>::new()),
    )
    .expect("deferred empty source audit");
    assert!(!empty.complete());
    assert!(!empty.portfolio_snapshot().complete());
    assert!(empty
        .stage(SolutionSetAuditStageKind::PortfolioSelected)
        .rejection_reasons()
        .iter()
        .any(|reason| {
            reason == "exact-minimum-cover-selection-deferred-to-product-coordinator"
        }));

    let uncovered = SolutionSetAuditReport::analyze(complete_input(
        vec!["solution-a".to_owned()],
        vec![candidate("solution-a", &[0], dimensions("minimum-cover"))],
        SolutionPortfolioSelectionPolicy::ProductDeferredExactMinimumCover,
    ))
    .expect("deferred uncovered source audit");
    assert!(!uncovered.complete());
    let reasons = uncovered.portfolio_families()[0].incomplete_reasons();
    assert!(reasons
        .iter()
        .any(|reason| reason == "required-pattern-cover-incomplete"));
    assert!(reasons.iter().any(|reason| {
        reason == "exact-minimum-cover-selection-deferred-to-product-coordinator"
    }));
}

#[test]
fn lazy_cursor_is_stable_for_equal_snapshots_and_fails_on_snapshot_drift() {
    let candidates = four_class_candidates();
    let keys = candidates
        .iter()
        .map(|candidate| candidate.canonical_key().to_owned())
        .collect::<Vec<_>>();
    let first = SolutionSetAuditReport::analyze(complete_input(
        keys.clone(),
        candidates.clone(),
        SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives,
    ))
    .expect("first audit");
    let reordered = SolutionSetAuditReport::analyze(complete_input(
        keys.into_iter().rev().collect(),
        candidates.into_iter().rev().collect(),
        SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives,
    ))
    .expect("reordered audit");
    let page = first
        .portfolio_snapshot()
        .page(None, 1)
        .expect("first page");
    let cursor = page.next_cursor().expect("next cursor");

    assert_eq!(
        reordered
            .portfolio_snapshot()
            .page(Some(cursor), 1)
            .expect("same snapshot accepts cursor")
            .offset(),
        1
    );

    let mut drifted_candidates = four_class_candidates();
    drifted_candidates.push(candidate(
        "solution-z",
        &[0, 1],
        dimensions("minimum-cover"),
    ));
    let drifted = SolutionSetAuditReport::analyze(complete_input(
        drifted_candidates
            .iter()
            .map(|candidate| candidate.canonical_key().to_owned())
            .collect(),
        drifted_candidates,
        SolutionPortfolioSelectionPolicy::EquivalentCoverageRepresentatives,
    ))
    .expect("different audit");
    assert!(matches!(
        drifted.portfolio_snapshot().page(Some(cursor), 1),
        Err(SolutionPortfolioPageError::SnapshotDrift { .. })
    ));
}

#[test]
fn two_pass_exact_selection_matches_exact_cardinality_and_retains_required_patterns() {
    let candidates = four_class_candidates();
    let keys = candidates
        .iter()
        .map(|candidate| candidate.canonical_key().to_owned())
        .collect::<Vec<_>>();
    let required = bits(4, &[0, 1, 2, 3]);
    let matrix = CoverageMatrix::from_rows(
        4,
        candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| CoverageRow::new(index, candidate.coverage().clone()))
            .collect(),
    )
    .expect("candidate matrix");
    let direct = MinimumCoverSolver::solve_exact(&matrix, &required).expect("direct exact cover");

    let report = SolutionSetAuditReport::analyze(complete_input(
        keys,
        candidates,
        SolutionPortfolioSelectionPolicy::ExactMinimumCover,
    ))
    .expect("two-pass report");
    let family = &report.portfolio_families()[0];

    assert!(direct.is_complete());
    assert!(direct.is_proven_minimum());
    assert_eq!(
        family.representative_keys().len(),
        direct.row_indices().len()
    );
    assert_eq!(family.covered_patterns(), direct.covered_patterns());
    assert!(family
        .covered_patterns()
        .is_superset(&required)
        .expect("matching pattern universe"));
    assert!(family.complete());
    assert!(family.exact_minimum_proven());
}

#[test]
fn two_pass_exact_selection_exposes_original_row_lex_first_identity() {
    let candidates = vec![
        candidate("solution-a", &[1, 2, 3], dimensions("minimum-cover")),
        candidate("solution-b", &[0], dimensions("minimum-cover")),
        candidate("solution-c", &[0, 1], dimensions("minimum-cover")),
    ];
    let keys = candidates
        .iter()
        .map(|candidate| candidate.canonical_key().to_owned())
        .collect::<Vec<_>>();

    let report = SolutionSetAuditReport::analyze(complete_input(
        keys,
        candidates,
        SolutionPortfolioSelectionPolicy::ExactMinimumCover,
    ))
    .expect("two-pass report with a properly dominated original row");
    let family = &report.portfolio_families()[0];

    // Coverage classes are ordered by their bit words before the exact
    // portfolio is selected: b={0}, c={0,1}, a={1,2,3}. Both [b,a] and [c,a]
    // are minimum covers. The exact proof may discard b as dominated, but the
    // public representative identity must be the original-row lex-first
    // portfolio [b,a].
    assert_eq!(family.representative_keys(), ["solution-b", "solution-a"]);
    assert!(family.complete());
    assert!(family.exact_minimum_proven());
}

#[test]
fn canonical_exact_selection_preserves_incomplete_partial_coverage_evidence() {
    let candidates = vec![
        candidate("solution-a", &[0], dimensions("minimum-cover")),
        candidate("solution-b", &[1], dimensions("minimum-cover")),
    ];
    let keys = candidates
        .iter()
        .map(|candidate| candidate.canonical_key().to_owned())
        .collect::<Vec<_>>();

    let report = SolutionSetAuditReport::analyze(complete_input(
        keys,
        candidates,
        SolutionPortfolioSelectionPolicy::ExactMinimumCover,
    ))
    .expect("an incomplete family remains a typed partial audit result");
    let family = &report.portfolio_families()[0];

    assert_eq!(family.representative_keys(), ["solution-a", "solution-b"]);
    assert_eq!(family.covered_patterns(), &bits(4, &[0, 1]));
    assert!(!family.complete());
    assert!(!family.exact_minimum_proven());
    assert_eq!(
        family.incomplete_reasons(),
        ["required-pattern-cover-incomplete"]
    );
}

#[test]
fn complete_audit_conserves_counts_and_records_identity_at_every_stage() {
    let candidates = four_class_candidates();
    let keys = candidates
        .iter()
        .map(|candidate| candidate.canonical_key().to_owned())
        .collect::<Vec<_>>();
    let report = SolutionSetAuditReport::analyze(complete_input(
        keys,
        candidates,
        SolutionPortfolioSelectionPolicy::ExactMinimumCover,
    ))
    .expect("complete audit");

    assert!(report.complete());
    for stage in report.stages() {
        assert!(stage.complete(), "{}", stage.kind().as_str());
        let input = stage.input_count().expect("complete input count");
        let output = stage.output_count().expect("complete output count");
        let rejected = stage.rejection_count().expect("complete rejection count");
        assert_eq!(input, output + rejected, "{}", stage.kind().as_str());
        if rejected != 0 {
            assert!(
                !stage.rejection_reasons().is_empty(),
                "{}",
                stage.kind().as_str()
            );
        }
        assert!(stage.input_identity_hash().is_some());
        assert!(stage.output_identity_hash().is_some());
    }
    assert_eq!(
        report
            .stage(SolutionSetAuditStageKind::CoverageClassed)
            .rejection_count(),
        Some(1)
    );
}
