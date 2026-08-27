use std::collections::BTreeSet;

use clearra_core_executor::{CoreExecutionResult, SetupCandidateReport, SetupFinderReport};
use clearra_problem::{
    compile_setup_search_conditions, SetupCycleResetBorrowPolicy, SetupLengthPreference,
    SetupPathDetail, SetupQueueInput, SetupSearchQuery,
};
use sha2::{Digest, Sha256};

use crate::setup_ranking_contract::{
    SetupRankingContract, SetupRankingContractError, SetupRankingIdentities, SetupRankingKind,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupRankedCandidateIdentity {
    candidate_id: String,
    condition_id: String,
    setup_id: String,
}

impl SetupRankedCandidateIdentity {
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub fn condition_id(&self) -> &str {
        &self.condition_id
    }

    pub fn setup_id(&self) -> &str {
        &self.setup_id
    }
}

/// Immutable App-owned authority retained after the validated Core result is
/// moved into the compatibility render model.
///
/// The large Setup report remains owned exactly once by `CoreExecutionResult`;
/// this snapshot retains only the query-bound product identities and the
/// canonical candidate identities needed by later product projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupRankedFamilySnapshot {
    kind: SetupRankingKind,
    identities: SetupRankingIdentities,
    resolved_length_preference: SetupLengthPreference,
    candidate_identities: Vec<SetupRankedCandidateIdentity>,
}

impl SetupRankedFamilySnapshot {
    pub const fn kind(&self) -> SetupRankingKind {
        self.kind
    }

    pub const fn capability_id(&self) -> &'static str {
        self.kind.capability_id()
    }

    pub const fn result_schema(&self) -> &'static str {
        self.kind.result_schema()
    }

    pub fn identities(&self) -> &SetupRankingIdentities {
        &self.identities
    }

    pub const fn resolved_length_preference(&self) -> SetupLengthPreference {
        self.resolved_length_preference
    }

    pub fn candidate_identities(&self) -> &[SetupRankedCandidateIdentity] {
        &self.candidate_identities
    }

    pub fn candidate_count(&self) -> usize {
        self.candidate_identities.len()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetupRankedFamilyResult {
    contract: SetupRankingContract,
    core_result: CoreExecutionResult,
    candidate_identities: Vec<SetupRankedCandidateIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupRankedFamilyResultError {
    Contract(SetupRankingContractError),
    MissingSetupReport,
    ReportIncomplete,
    ReportQueryMismatch(&'static str),
    QueryConditionCompileFailed,
    ConditionCountMismatch,
    ConditionIdentityMismatch {
        condition_index: usize,
        field: &'static str,
    },
    HoldConditionIncomplete {
        condition_index: usize,
    },
    HoldConditionTruncated {
        condition_index: usize,
    },
    HoldConditionCountMismatch {
        condition_index: usize,
    },
    DuplicateConditionId,
    InvalidCandidate {
        condition_index: usize,
        candidate_index: usize,
        field: &'static str,
    },
    DuplicateCandidateId,
    RankingOrderInvalid {
        condition_index: usize,
        candidate_index: usize,
    },
    CoreFieldMissing(&'static str),
    CoreFieldDuplicated(&'static str),
    CoreFieldMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
}

impl From<SetupRankingContractError> for SetupRankedFamilyResultError {
    fn from(error: SetupRankingContractError) -> Self {
        Self::Contract(error)
    }
}

impl SetupRankedFamilyResult {
    pub fn from_core_result(
        contract: SetupRankingContract,
        query: &SetupSearchQuery,
        core_result: CoreExecutionResult,
    ) -> Result<Self, SetupRankedFamilyResultError> {
        contract.validate_query(query)?;
        let report = core_result
            .setup_finder_report()
            .ok_or(SetupRankedFamilyResultError::MissingSetupReport)?;
        validate_report_identity(query, report)?;
        validate_core_fields(query, report, &core_result)?;
        let candidate_identities = validate_candidates(contract.kind(), query, report)?;
        Ok(Self {
            contract,
            core_result,
            candidate_identities,
        })
    }

    pub fn contract(&self) -> &SetupRankingContract {
        &self.contract
    }

    pub fn report(&self) -> &SetupFinderReport {
        self.core_result
            .setup_finder_report()
            .expect("validated ranked-family results retain their setup report")
    }

    pub fn core_result(&self) -> &CoreExecutionResult {
        &self.core_result
    }

    pub fn into_core_result_and_snapshot(self) -> (CoreExecutionResult, SetupRankedFamilySnapshot) {
        let Self {
            contract,
            core_result,
            candidate_identities,
        } = self;
        let snapshot = SetupRankedFamilySnapshot {
            kind: contract.kind(),
            identities: contract.identities().clone(),
            resolved_length_preference: contract.resolved_length_preference(),
            candidate_identities,
        };
        (core_result, snapshot)
    }

    pub fn candidate_identities(&self) -> &[SetupRankedCandidateIdentity] {
        &self.candidate_identities
    }

    pub fn candidate_count(&self) -> usize {
        self.candidate_identities.len()
    }
}

fn validate_report_identity(
    query: &SetupSearchQuery,
    report: &SetupFinderReport,
) -> Result<(), SetupRankedFamilyResultError> {
    if !report.complete() {
        return Err(SetupRankedFamilyResultError::ReportIncomplete);
    }
    if report.search_mode() != query.search_mode() {
        return Err(SetupRankedFamilyResultError::ReportQueryMismatch(
            "search_mode",
        ));
    }
    if report.queue_observation_policy() != query.queue_observation_policy() {
        return Err(SetupRankedFamilyResultError::ReportQueryMismatch(
            "queue_observation_policy",
        ));
    }
    if report.cycle() != query.residue().cycle().unwrap_or_default() {
        return Err(SetupRankedFamilyResultError::ReportQueryMismatch("cycle"));
    }
    if report.remaining_pieces() != pieces_string(query.residue().pieces()) {
        return Err(SetupRankedFamilyResultError::ReportQueryMismatch(
            "remaining_pieces",
        ));
    }
    if report.queue_based_pieces() != queue_based_pieces(query.queue()) {
        return Err(SetupRankedFamilyResultError::ReportQueryMismatch(
            "queue_based_pieces",
        ));
    }
    if report.next_cycle_remaining_pieces()
        != pieces_string(query.next_cycle_remaining_pieces().unwrap_or(&[]))
    {
        return Err(SetupRankedFamilyResultError::ReportQueryMismatch(
            "next_cycle_remaining_pieces",
        ));
    }
    let borrow_enabled =
        query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse;
    if report.post_cycle_borrow_enabled() != borrow_enabled {
        return Err(SetupRankedFamilyResultError::ReportQueryMismatch(
            "post_cycle_borrow_enabled",
        ));
    }
    let expected_conditions = compile_setup_search_conditions(query)
        .map_err(|_| SetupRankedFamilyResultError::QueryConditionCompileFailed)?;
    if report.hold_conditions().len() != expected_conditions.len() {
        return Err(SetupRankedFamilyResultError::ConditionCountMismatch);
    }
    for (condition_index, (actual, expected)) in report
        .hold_conditions()
        .iter()
        .zip(&expected_conditions)
        .enumerate()
    {
        for (matches, field) in [
            (
                actual.condition_id() == expected.condition_id(),
                "condition_id",
            ),
            (
                actual.initial_hold() == expected.initial_hold(),
                "initial_hold",
            ),
            (
                actual.pattern_expression() == expected.pattern_expression(),
                "pattern_expression",
            ),
        ] {
            if !matches {
                return Err(SetupRankedFamilyResultError::ConditionIdentityMismatch {
                    condition_index,
                    field,
                });
            }
        }
    }
    Ok(())
}

fn validate_core_fields(
    query: &SetupSearchQuery,
    report: &SetupFinderReport,
    result: &CoreExecutionResult,
) -> Result<(), SetupRankedFamilyResultError> {
    require_field(result, "status", "setup-finder-complete")?;
    require_field(result, "count_complete", "true")?;
    require_field(result, "probability_complete", "true")?;
    require_field(result, "resource_truncated", "false")?;
    require_field(result, "resource_truncation_reason", "none")?;
    require_field(
        result,
        "setup_coverage_semantics",
        query.queue_observation_policy().coverage_semantics(),
    )?;
    require_field(
        result,
        "queue_knowledge",
        query.queue_observation_policy().keyword(),
    )?;
    let visible_piece_count = query
        .queue_observation_policy()
        .visible_piece_count()
        .map_or_else(|| "all".to_owned(), |count| count.to_string());
    require_field(result, "visible_piece_count", &visible_piece_count)?;
    require_field(result, "setup_search_mode", query.search_mode().keyword())?;
    require_field(result, "remaining_pieces", report.remaining_pieces())?;
    require_field(result, "queue_based_pieces", report.queue_based_pieces())?;
    require_field(
        result,
        "next_cycle_remaining_pieces",
        report.next_cycle_remaining_pieces(),
    )?;
    require_field(result, "setup_cycle", &report.cycle().to_string())?;
    require_field(
        result,
        "setup_candidate_priority",
        query.candidate_priority().keyword(),
    )?;
    require_field(
        result,
        "setup_length_preference",
        query.length_preference().keyword(),
    )?;
    require_field(
        result,
        "geometry_candidate_family_count",
        report.geometry_family_count(),
    )?;
    require_field(
        result,
        "partial_build_node_count",
        &report.partial_build_node_count().to_string(),
    )?;
    require_field(
        result,
        "tablebase_requested",
        &query.tablebase_requested().to_string(),
    )?;
    require_field(
        result,
        "normalized_solution_key_algorithm",
        "clearra-setup-candidate-key-v2-exact-partial-state",
    )?;
    require_field(
        result,
        "normalized_solution_set_hash_algorithm",
        "clearra-setup-candidate-set-fnv64-v1",
    )?;

    let candidate_count = report
        .hold_conditions()
        .iter()
        .try_fold(0_usize, |count, condition| {
            count.checked_add(condition.candidates().len())
        })
        .ok_or_else(|| SetupRankedFamilyResultError::CoreFieldMismatch {
            field: "unique_solution_count",
            expected: "finite usize".to_owned(),
            actual: "overflow".to_owned(),
        })?;
    require_field(
        result,
        "unique_solution_count",
        &candidate_count.to_string(),
    )?;
    require_field(
        result,
        "normalized_unique_solution_count",
        &candidate_count.to_string(),
    )?;
    require_field(
        result,
        "solution_found",
        &(candidate_count != 0).to_string(),
    )?;
    let expected_set_hash = setup_candidate_set_hash(report);
    require_field(result, "normalized_solution_set_hash", &expected_set_hash)?;
    require_field(
        result,
        "actual_normalized_solution_set_hash",
        &expected_set_hash,
    )?;
    Ok(())
}

fn validate_candidates(
    kind: SetupRankingKind,
    query: &SetupSearchQuery,
    report: &SetupFinderReport,
) -> Result<Vec<SetupRankedCandidateIdentity>, SetupRankedFamilyResultError> {
    let mut condition_ids = BTreeSet::new();
    let mut canonical_ids = BTreeSet::new();
    let mut identities = Vec::new();
    for (condition_index, condition) in report.hold_conditions().iter().enumerate() {
        if !condition.complete() {
            return Err(SetupRankedFamilyResultError::HoldConditionIncomplete { condition_index });
        }
        if condition.result_truncated() {
            return Err(SetupRankedFamilyResultError::HoldConditionTruncated { condition_index });
        }
        if condition.candidate_count() != condition.candidates().len() {
            return Err(SetupRankedFamilyResultError::HoldConditionCountMismatch {
                condition_index,
            });
        }
        if condition.condition_id().is_empty()
            || !condition_ids.insert(condition.condition_id().to_owned())
        {
            return Err(SetupRankedFamilyResultError::DuplicateConditionId);
        }

        let mut previous_primary = None;
        let mut source_ids = BTreeSet::new();
        for (candidate_index, candidate) in condition.candidates().iter().enumerate() {
            validate_candidate(condition.pattern_count(), query, candidate).map_err(|field| {
                SetupRankedFamilyResultError::InvalidCandidate {
                    condition_index,
                    candidate_index,
                    field,
                }
            })?;
            if !source_ids.insert(candidate.setup_id()) {
                return Err(SetupRankedFamilyResultError::InvalidCandidate {
                    condition_index,
                    candidate_index,
                    field: "duplicate_setup_id",
                });
            }
            let primary = primary_probability(kind, candidate).ok_or(
                SetupRankedFamilyResultError::InvalidCandidate {
                    condition_index,
                    candidate_index,
                    field: "primary_probability",
                },
            )?;
            if previous_primary.is_some_and(|previous: f64| primary > previous) {
                return Err(SetupRankedFamilyResultError::RankingOrderInvalid {
                    condition_index,
                    candidate_index,
                });
            }
            previous_primary = Some(primary);

            let candidate_id = setup_ranked_candidate_id(condition.condition_id(), candidate);
            if !canonical_ids.insert(candidate_id.clone()) {
                return Err(SetupRankedFamilyResultError::DuplicateCandidateId);
            }
            identities.push(SetupRankedCandidateIdentity {
                candidate_id,
                condition_id: condition.condition_id().to_owned(),
                setup_id: candidate.setup_id().to_owned(),
            });
        }
    }
    Ok(identities)
}

fn validate_candidate(
    pattern_count: usize,
    query: &SetupSearchQuery,
    candidate: &SetupCandidateReport,
) -> Result<(), &'static str> {
    let detail =
        SetupPathDetail::from_setup_id(candidate.setup_id(), "identity").ok_or("setup_id")?;
    if detail.board_mask() != candidate.board_mask() {
        return Err("board_mask");
    }
    if candidate.min_locks() > candidate.max_locks()
        || candidate.max_locks() > query.max_setup_pieces()
    {
        return Err("lock_range");
    }
    if candidate.joint_covered_patterns() > candidate.build_covered_patterns()
        || candidate.build_covered_patterns() > pattern_count
    {
        return Err("coverage_counts");
    }
    let build = canonical_probability(candidate.build_probability()).ok_or("build_probability")?;
    let joint = canonical_probability(candidate.joint_probability()).ok_or("joint_probability")?;
    let conditional = canonical_probability(candidate.conditional_pc_probability())
        .ok_or("conditional_pc_probability")?;
    if joint > build + 1.0e-12 {
        return Err("joint_probability_exceeds_build");
    }
    let expected_conditional = if build == 0.0 { 0.0 } else { joint / build };
    if (conditional - expected_conditional).abs() > 2.0e-9 {
        return Err("conditional_pc_probability_relation");
    }
    let representative_len = candidate.representative_path().len();
    if representative_len < usize::from(candidate.min_locks())
        || representative_len > usize::from(candidate.max_locks())
    {
        return Err("representative_path_length");
    }
    Ok(())
}

fn primary_probability(kind: SetupRankingKind, candidate: &SetupCandidateReport) -> Option<f64> {
    canonical_probability(match kind {
        SetupRankingKind::Joint => candidate.joint_probability(),
        SetupRankingKind::Build => candidate.build_probability(),
        SetupRankingKind::ConditionalPc => candidate.conditional_pc_probability(),
    })
}

fn canonical_probability(value: &str) -> Option<f64> {
    let parsed = value.parse::<f64>().ok()?;
    (parsed.is_finite() && (0.0..=1.0).contains(&parsed) && probability_string(parsed) == value)
        .then_some(parsed)
}

fn probability_string(value: f64) -> String {
    let mut output = format!("{:.12}", value.clamp(0.0, 1.0));
    while output.ends_with('0') {
        output.pop();
    }
    if output.ends_with('.') {
        output.push('0');
    }
    output
}

fn require_field(
    result: &CoreExecutionResult,
    field: &'static str,
    expected: &str,
) -> Result<(), SetupRankedFamilyResultError> {
    match result.field_occurrence_count(field) {
        0 => return Err(SetupRankedFamilyResultError::CoreFieldMissing(field)),
        1 => {}
        _ => return Err(SetupRankedFamilyResultError::CoreFieldDuplicated(field)),
    }
    let actual = result
        .unique_field(field)
        .ok_or(SetupRankedFamilyResultError::CoreFieldMissing(field))?;
    if actual != expected {
        return Err(SetupRankedFamilyResultError::CoreFieldMismatch {
            field,
            expected: expected.to_owned(),
            actual: actual.to_owned(),
        });
    }
    Ok(())
}

fn setup_candidate_set_hash(report: &SetupFinderReport) -> String {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;
    let mut conditions = report.hold_conditions().iter().collect::<Vec<_>>();
    conditions.sort_unstable_by(|left, right| left.condition_id().cmp(right.condition_id()));
    let mut hash = FNV_OFFSET;
    for condition in conditions {
        // The core solution-set authority hashes candidate board identities in
        // ascending board order. Ranked-family presentation order is allowed
        // to differ by capability, so it must not affect the solution-set
        // identity validated here.
        let mut candidates = condition.candidates().iter().collect::<Vec<_>>();
        candidates.sort_unstable_by_key(|candidate| candidate.board_mask());
        for candidate in candidates {
            for byte in condition
                .condition_id()
                .bytes()
                .chain(core::iter::once(b'|'))
            {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            for shift in (0..10).rev() {
                let nibble = ((candidate.board_mask() >> (shift * 4)) & 0x0f) as u8;
                let byte = if nibble < 10 {
                    b'0' + nibble
                } else {
                    b'a' + nibble - 10
                };
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
            hash ^= 0;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("css1:{hash:016x}")
}

pub fn setup_ranked_candidate_id(condition_id: &str, candidate: &SetupCandidateReport) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.setup-candidate.v1\0");
    hash_text(&mut hasher, condition_id);
    hash_text(&mut hasher, candidate.setup_id());
    format!("setup-candidate.v1:{}", hex_sha256(hasher.finalize()))
}

fn queue_based_pieces(queue: &SetupQueueInput) -> String {
    queue
        .as_fixed_sequence()
        .map(|queue| pieces_string(queue.pieces()))
        .unwrap_or_default()
}

fn pieces_string(pieces: &[clearra_core_domain::piece::piece_kind::PieceKind]) -> String {
    pieces.iter().map(|piece| piece.as_ascii()).collect()
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u128).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn hex_sha256(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
