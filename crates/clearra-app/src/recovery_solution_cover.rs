//! Recovery's adapter to the existing exact minimum/pinned portfolio engine.
//! The input is a COMPLETE solution coverage catalog, never representative
//! replay samples. Render pagination and GIF/PNG choices are not solver inputs.
use clearra_coverage::{
    cover::pinned_minimum_cover::{PinnedMinimumCoverError, PinnedMinimumCoverInput},
    pattern::pattern_bitset::PatternBitSet,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct RecoverySolutionCoverageRow {
    pub solution_id: String,
    pub covered_pairs: PatternBitSet,
}
#[derive(Clone, Debug)]
pub struct RecoverySolutionCoverageCatalog {
    pub input_identity: [u8; 32],
    pub universe_identity: [u8; 32],
    pub weight_model_identity: [u8; 32],
    pub pattern_count: usize,
    pub expected_solution_count: usize,
    pub enumeration_complete: bool,
    pub coverage_complete: bool,
    pub rows: Vec<RecoverySolutionCoverageRow>,
}
#[derive(Debug)]
pub enum RecoveryMinimumError {
    Cancelled,
    IncompleteCatalog,
    StaleInput,
    InvalidSolutionIdentity,
    PatternCountMismatch,
    UnknownPinnedSolution(String),
    DuplicatePinnedSolution(String),
    AllocationFailed,
    Exact(PinnedMinimumCoverError),
}

/// Keeps exact-search rows in stable original-solution order. Private pin bits
/// belong solely to the existing minimum engine, not to probability outputs.
pub struct RecoveryMinimumPreparation {
    pub solution_ids: Vec<String>,
    pub input: PinnedMinimumCoverInput,
}
impl RecoverySolutionCoverageCatalog {
    pub fn prepare_minimum(
        &self,
        expected_input: [u8; 32],
        pinned: &[String],
    ) -> Result<RecoveryMinimumPreparation, RecoveryMinimumError> {
        if self.input_identity != expected_input {
            return Err(RecoveryMinimumError::StaleInput);
        }
        if !self.enumeration_complete
            || !self.coverage_complete
            || self.expected_solution_count != self.rows.len()
        {
            return Err(RecoveryMinimumError::IncompleteCatalog);
        }
        let mut order = BTreeMap::new();
        for row in &self.rows {
            if row.solution_id.is_empty()
                || order
                    .insert(row.solution_id.as_str(), &row.covered_pairs)
                    .is_some()
            {
                return Err(RecoveryMinimumError::InvalidSolutionIdentity);
            }
            if row.covered_pairs.pattern_count() != self.pattern_count {
                return Err(RecoveryMinimumError::PatternCountMismatch);
            }
        }
        let mut ids = Vec::new();
        let mut rows = Vec::new();
        ids.try_reserve_exact(order.len())
            .map_err(|_| RecoveryMinimumError::AllocationFailed)?;
        rows.try_reserve_exact(order.len())
            .map_err(|_| RecoveryMinimumError::AllocationFailed)?;
        let mut required_words = vec![0_u64; self.pattern_count.div_ceil(64)];
        for (id, row) in order {
            for (union, word) in required_words.iter_mut().zip(row.to_owned_words()) {
                *union |= word;
            }
            ids.push(id.to_owned());
            rows.push(row.clone());
        }
        let mut pins = Vec::new();
        pins.try_reserve_exact(pinned.len())
            .map_err(|_| RecoveryMinimumError::AllocationFailed)?;
        for id in pinned {
            let index = ids
                .binary_search(id)
                .map_err(|_| RecoveryMinimumError::UnknownPinnedSolution(id.clone()))?;
            if pins.contains(&index) {
                return Err(RecoveryMinimumError::DuplicatePinnedSolution(id.clone()));
            }
            pins.push(index);
        }
        let required = PatternBitSet::from_words(self.pattern_count, required_words)
            .map_err(|_| RecoveryMinimumError::PatternCountMismatch)?;
        let input = PinnedMinimumCoverInput::new(required, rows, pins)
            .map_err(RecoveryMinimumError::Exact)?;
        Ok(RecoveryMinimumPreparation {
            solution_ids: ids,
            input,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_coverage::pattern::pattern_id::PatternId;
    fn catalog(masks: &[u8]) -> RecoverySolutionCoverageCatalog {
        RecoverySolutionCoverageCatalog {
            input_identity: [1; 32],
            universe_identity: [2; 32],
            weight_model_identity: [3; 32],
            pattern_count: 4,
            expected_solution_count: masks.len(),
            enumeration_complete: true,
            coverage_complete: true,
            rows: masks
                .iter()
                .enumerate()
                .map(|(id, &mask)| RecoverySolutionCoverageRow {
                    solution_id: format!("solution-{id}"),
                    covered_pairs: PatternBitSet::from_patterns(
                        4,
                        (0..4)
                            .filter(|bit| mask & (1 << bit) != 0)
                            .map(PatternId::new),
                    )
                    .unwrap(),
                })
                .collect(),
        }
    }
    #[test]
    fn recovery_minimum_and_pins_match_independent_exhaustive_subsets() {
        for seed in 0..32_u32 {
            let masks = (0..5)
                .map(|i| ((seed.wrapping_mul(13) + i * 7) % 16) as u8)
                .collect::<Vec<_>>();
            let catalog = catalog(&masks);
            for pins in [0_u32, 1, 3, 17] {
                let pinned = (0..5)
                    .filter(|i| pins & (1 << i) != 0)
                    .map(|i| format!("solution-{i}"))
                    .collect::<Vec<_>>();
                let prepared = catalog.prepare_minimum([1; 32], &pinned).unwrap();
                let actual = prepared.input.canonical_portfolio().unwrap();
                let required = masks.iter().fold(0, |a, b| a | b);
                let expected = (0..32_u32)
                    .filter(|set| set & pins == pins)
                    .filter(|set| {
                        (0..5)
                            .filter(|i| set & (1 << i) != 0)
                            .fold(0, |a, i| a | masks[i])
                            == required
                    })
                    .map(|set| set.count_ones() as usize)
                    .min()
                    .unwrap();
                assert_eq!(actual.all_row_indices().len(), expected);
                assert!(actual
                    .pinned_row_indices()
                    .iter()
                    .all(|&i| actual.all_row_indices().contains(&i)));
            }
        }
    }
    #[test]
    fn recovery_minimum_does_not_optimize_a_representative_page() {
        let mut c = catalog(&[15, 1, 2, 0]);
        let result = c
            .prepare_minimum([1; 32], &["solution-3".into()])
            .unwrap()
            .input
            .canonical_portfolio()
            .unwrap();
        assert_eq!(result.all_row_indices(), [0, 3]); // Even a redundant pin remains.
        assert!(matches!(
            c.prepare_minimum([9; 32], &[]),
            Err(RecoveryMinimumError::StaleInput)
        ));
        c.enumeration_complete = false;
        assert!(matches!(
            c.prepare_minimum([1; 32], &[]),
            Err(RecoveryMinimumError::IncompleteCatalog)
        ));
        c.enumeration_complete = true;
        c.expected_solution_count = 1000;
        assert!(matches!(
            c.prepare_minimum([1; 32], &[]),
            Err(RecoveryMinimumError::IncompleteCatalog)
        ));
    }
}

/// Compile the exact support quotient to the existing original-row minimum
/// authority. A class represents every input with that same supporter set;
/// weights stay in the probability layer and are never replaced by class size.
pub(crate) fn select_recovery_minimum(
    query: &clearra_forward_search::RecoveryBuildQuery,
    report: &clearra_forward_search::RecoveryBuildPopulation,
    identity: [u8; 32],
    control: &clearra_core_domain::execution_cancellation::ExecutionControl,
) -> Result<Vec<String>, RecoveryMinimumError> {
    if !report.solutions_complete || report.evaluated != report.possible {
        return Err(RecoveryMinimumError::IncompleteCatalog);
    }
    let classes = report
        .coverage_classes
        .as_ref()
        .ok_or(RecoveryMinimumError::IncompleteCatalog)?;
    let ids = report
        .solutions
        .iter()
        .map(|s| s.key.clone())
        .collect::<Vec<_>>();
    select_support_minimum(
        &ids,
        classes,
        &query.required_solution_keys,
        identity,
        control,
    )
}

/// Uses the same exact minimum authority on a complete multi-stage catalog.
/// The expected query binds every target, source, rule and boundary constraint.
pub fn select_recovery_chain_minimum(
    query: &clearra_forward_search::RecoveryChainQuery,
    report: &clearra_forward_search::RecoveryChainCatalog,
    pinned: &[String],
    control: &clearra_core_domain::execution_cancellation::ExecutionControl,
) -> Result<Vec<String>, RecoveryMinimumError> {
    use sha2::{Digest, Sha256};
    if &report.input != query {
        return Err(RecoveryMinimumError::StaleInput);
    }
    if !report.complete {
        return Err(RecoveryMinimumError::IncompleteCatalog);
    }
    let identity = Sha256::digest(format!("recovery-chain.v1:{query:?}").as_bytes()).into();
    let ids = report
        .solutions
        .iter()
        .map(|s| s.key.clone())
        .collect::<Vec<_>>();
    select_support_minimum(&ids, &report.coverage_classes, pinned, identity, control)
}

fn select_support_minimum(
    solution_ids: &[String],
    classes: &[Vec<usize>],
    pinned: &[String],
    identity: [u8; 32],
    control: &clearra_core_domain::execution_cancellation::ExecutionControl,
) -> Result<Vec<String>, RecoveryMinimumError> {
    use clearra_coverage::{
        cover::exact_minimum_cover_portfolios::{
            ExactMinimumCoverPortfolioPreparationAdvance as Advance,
            ExactMinimumCoverPortfolioPreparationSession as Session,
        },
        pattern::pattern_id::PatternId,
    };
    use sha2::{Digest, Sha256};
    let mut memberships = vec![Vec::new(); solution_ids.len()];
    for (index, support) in classes.iter().enumerate() {
        if support.is_empty() || !support.windows(2).all(|p| p[0] < p[1]) {
            return Err(RecoveryMinimumError::InvalidSolutionIdentity);
        }
        for &candidate in support {
            memberships
                .get_mut(candidate)
                .ok_or(RecoveryMinimumError::InvalidSolutionIdentity)?
                .push(PatternId::new(index));
        }
    }
    let catalog = RecoverySolutionCoverageCatalog {
        input_identity: identity,
        universe_identity: Sha256::digest(
            [b"recovery-support-classes.v1:".as_slice(), &identity].concat(),
        )
        .into(),
        weight_model_identity: Sha256::digest(b"exact-unweighted-cardinality-not-probability")
            .into(),
        pattern_count: classes.len(),
        expected_solution_count: solution_ids.len(),
        enumeration_complete: true,
        coverage_complete: true,
        rows: solution_ids
            .iter()
            .zip(memberships)
            .map(|(solution, bits)| {
                Ok(RecoverySolutionCoverageRow {
                    solution_id: solution.clone(),
                    covered_pairs: PatternBitSet::from_patterns(classes.len(), bits)
                        .map_err(|_| RecoveryMinimumError::PatternCountMismatch)?,
                })
            })
            .collect::<Result<Vec<_>, RecoveryMinimumError>>()?,
    };
    let prepared = catalog.prepare_minimum(identity, pinned)?;
    let ids = prepared.solution_ids;
    let (required, rows) = prepared.input.into_augmented_parts();
    let convert = |e| RecoveryMinimumError::Exact(PinnedMinimumCoverError::Portfolio(e));
    let mut session =
        Session::new_with_memory_guard(&required, &rows, &mut |_| Ok(())).map_err(convert)?;
    let mut work = 0;
    let mut enumerator = loop {
        if control.is_cancelled() {
            return Err(RecoveryMinimumError::Cancelled);
        }
        match session
            .advance_with_memory_guard_and_control(4096, &mut |_| Ok(()), &mut || {
                control.is_cancelled()
            })
            .map_err(convert)?
        {
            Advance::Pending { visited_nodes } => {
                work += visited_nodes;
                control.report_progress("recovery-minimum", work, None);
            }
            Advance::Coverable { enumerator, .. } => break enumerator,
            Advance::Cancelled { .. } => return Err(RecoveryMinimumError::Cancelled),
            _ => return Err(RecoveryMinimumError::IncompleteCatalog),
        }
    };
    loop {
        if control.is_cancelled() {
            return Err(RecoveryMinimumError::Cancelled);
        }
        let page = enumerator
            .next_page_with_control(1, 4096, &mut || control.is_cancelled())
            .map_err(convert)?;
        if let Some(portfolio) = page.portfolios().first() {
            let keys = portfolio
                .row_indices()
                .iter()
                .map(|&index| ids[index].clone())
                .collect::<Vec<_>>();
            if !pinned.iter().all(|key| keys.contains(key)) {
                return Err(RecoveryMinimumError::InvalidSolutionIdentity);
            }
            return Ok(keys);
        }
        if page.enumeration_complete() {
            return Err(RecoveryMinimumError::IncompleteCatalog);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod benchmark {
    #[test]
    #[ignore = "finite exact full P7/P7 benchmark, reports actual host and requested compute slots"]
    fn recovery_minimum_catalog_original_fixture_parallel_benchmark() {
        use clearra_core_domain::{
            board::standard_pc_board::Board256Mask as M, execution_cancellation::ExecutionControl,
        };
        use clearra_forward_search::{
            CrossStageEarlyLimit, RecoveryBuildFields, RecoveryBuildQuery,
        };
        use clearra_rules::profile::rule_profile::RuleProfileId;
        use clearra_scoring::profile::SpinProfileId;
        let mask = |v| M::from_words([v, 0, 0, 0]);
        let early = std::env::var("CLEARRA_EARLY_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok());
        let workers = std::env::var("CLEARRA_BENCH_WORKERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(11);
        let query = RecoveryBuildQuery {
            all_solutions: true,
            minimum_solutions: false,
            required_solution_keys: Vec::new(),
            minimum_source_identity: None,
            fields: RecoveryBuildFields {
                height: 10,
                initial: mask(0xc0383f3fc7),
                middle: mask(0x3ff3fc7c0c038),
                result: mask(0x30483f07f3f8f),
            },
            first_supply: "P7".into(),
            second_supply: "P7".into(),
            early_limit: early.map_or(CrossStageEarlyLimit::Auto, CrossStageEarlyLimit::AtMost),
            allow_piece_exchange: true,
            hold_enabled: true,
            preserve_b2b: true,
            initial_b2b: true,
            rule_profile: RuleProfileId::SrsPlus,
            spin_profile: SpinProfileId::AllSpinPlus,
        };
        let start = std::time::Instant::now();
        eprintln!(
            "recovery_full_fixture_started workers={} host_logical={:?} early={:?}",
            workers,
            std::thread::available_parallelism(),
            early
        );
        let report = crate::native_recovery_build_execution::run_native_recovery_build(
            query,
            workers,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(report.evaluated, 25_401_600);
        assert!(report.solutions_complete);
        assert!(!report.solutions.is_empty());
        for solution in &report.solutions {
            assert!(solution.covered_count > 0);
            if let Some(limit) = early {
                assert!(solution.example.path.actual_early <= limit);
            }
        }
        eprintln!("recovery_full_fixture_completed elapsed_ms={} normal={} recovery={} no_path={} solutions={} states={}",start.elapsed().as_millis(),report.normal_count,report.recovery_count,report.no_path_count,report.solutions.len(),report.states);
    }
}

#[cfg(test)]
mod chain_catalog_tests {
    use super::*;
    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask as Mask, execution_cancellation::ExecutionControl,
    };
    use clearra_forward_search::{CrossStageEarlyLimit, RecoveryChainQuery};
    fn query() -> RecoveryChainQuery {
        RecoveryChainQuery {
            height: 8,
            initial: Mask::EMPTY,
            targets: [0xc03, 0x300c, 0xc030]
                .into_iter()
                .map(|v| Mask::from_words([v, 0, 0, 0]))
                .collect(),
            supplies: vec!["[IO]".into(); 3],
            early_limit: CrossStageEarlyLimit::AtMost(0),
            allow_piece_exchange: false,
            hold_enabled: true,
            preserve_b2b: false,
            initial_b2b: true,
            rule_profile: clearra_rules::profile::rule_profile::RuleProfileId::SrsPlus,
            spin_profile: clearra_scoring::profile::SpinProfileId::AllSpinPlus,
        }
    }
    #[test]
    fn chain_minimum_preserves_success_union_and_all_required_mirrors() {
        let q = query();
        let control = ExecutionControl::default();
        let report = q.catalog(&control).unwrap();
        assert_eq!(report.solutions.len(), 2);
        assert_eq!(report.coverage.possible, 8);
        assert_eq!(report.coverage_classes.len(), 1);
        let minimum = select_recovery_chain_minimum(&q, &report, &[], &control).unwrap();
        assert_eq!(minimum.len(), 1);
        let pins = report
            .solutions
            .iter()
            .map(|s| s.key.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            select_recovery_chain_minimum(&q, &report, &pins, &control).unwrap(),
            pins
        );
        assert_eq!(
            report.coverage.normal_count, 1,
            "minimum constraint classes never replace probability counts"
        );
    }
    #[test]
    fn chain_minimum_rejects_stale_queries_unfinished_catalogs_and_unknown_pins() {
        let q = query();
        let control = ExecutionControl::default();
        let mut report = q.catalog(&control).unwrap();
        let mut other = q.clone();
        other.supplies[0] = "O".into();
        assert!(matches!(
            select_recovery_chain_minimum(&other, &report, &[], &control),
            Err(RecoveryMinimumError::StaleInput)
        ));
        assert!(matches!(
            select_recovery_chain_minimum(&q, &report, &["foreign".into()], &control),
            Err(RecoveryMinimumError::UnknownPinnedSolution(_))
        ));
        report.complete = false;
        assert!(matches!(
            select_recovery_chain_minimum(&q, &report, &[], &control),
            Err(RecoveryMinimumError::IncompleteCatalog)
        ));
    }
}
