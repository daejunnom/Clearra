use std::collections::BTreeSet;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_spin_structure_search::{
    MinimalityPolicy, SpinStructureError, SpinStructureOutcome, SpinStructureQuery,
    SpinStructureReport, StructureBoard, StructureOperation,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinStructureSearchIdentities {
    query_sha256: String,
    rule_profile: String,
    spin_profile: String,
    supply_sha256: String,
    universe_sha256: String,
    product_build: String,
}

impl SpinStructureSearchIdentities {
    pub fn query_sha256(&self) -> &str {
        &self.query_sha256
    }

    pub fn rule_profile(&self) -> &str {
        &self.rule_profile
    }

    pub fn spin_profile(&self) -> &str {
        &self.spin_profile
    }

    pub fn supply_sha256(&self) -> &str {
        &self.supply_sha256
    }

    pub fn universe_sha256(&self) -> &str {
        &self.universe_sha256
    }

    pub fn product_build(&self) -> &str {
        &self.product_build
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinStructureSearchCandidateIdentity {
    candidate_id: String,
    mini: bool,
    placement_count: usize,
}

impl SpinStructureSearchCandidateIdentity {
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub const fn mini(&self) -> bool {
        self.mini
    }

    pub const fn placement_count(&self) -> usize {
        self.placement_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinStructureSearchResult {
    identities: SpinStructureSearchIdentities,
    report: SpinStructureReport,
    candidate_identities: Vec<SpinStructureSearchCandidateIdentity>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpinStructureSearchResultError {
    InvalidQuery(SpinStructureError),
    ReportIncomplete,
    MissingReportQuery,
    QueryIdentityMismatch,
    ZeroWorkers,
    LayerOrderInvalid,
    MinimumPlacementMismatch,
    MinimumPieceFamilyMismatch,
    OutcomePartitionMismatch {
        mini_partition: bool,
        candidate_index: usize,
    },
    InvalidOutcome {
        mini_partition: bool,
        candidate_index: usize,
        field: &'static str,
    },
    DuplicateCandidateId,
    CandidateOrderInvalid {
        mini_partition: bool,
        candidate_index: usize,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CanonicalOutcomeKey {
    operations: Vec<StructureOperation>,
    target: StructureOperation,
    target_cleared_rows: u32,
}

impl SpinStructureSearchResult {
    pub fn promote(
        query: &SpinStructureQuery,
        report: SpinStructureReport,
    ) -> Result<Self, SpinStructureSearchResultError> {
        query
            .validate()
            .map_err(SpinStructureSearchResultError::InvalidQuery)?;
        if !report.complete {
            return Err(SpinStructureSearchResultError::ReportIncomplete);
        }
        let report_query = report
            .query
            .as_ref()
            .ok_or(SpinStructureSearchResultError::MissingReportQuery)?;
        if report_query != query {
            return Err(SpinStructureSearchResultError::QueryIdentityMismatch);
        }
        if report.workers_used() == 0 {
            return Err(SpinStructureSearchResultError::ZeroWorkers);
        }
        if report
            .layers
            .windows(2)
            .any(|layers| layers[0].depth >= layers[1].depth)
            || report
                .layers
                .iter()
                .any(|layer| layer.depth > query.placement_limit())
        {
            return Err(SpinStructureSearchResultError::LayerOrderInvalid);
        }
        validate_minimum(query, &report)?;
        let candidate_identities = validate_outcomes(query, &report)?;
        Ok(Self {
            identities: spin_structure_identities(query),
            report,
            candidate_identities,
        })
    }

    pub const fn result_schema(&self) -> &'static str {
        "spin-structure-family.v2"
    }

    pub fn identities(&self) -> &SpinStructureSearchIdentities {
        &self.identities
    }

    pub fn report(&self) -> &SpinStructureReport {
        &self.report
    }

    pub fn into_report(self) -> SpinStructureReport {
        self.report
    }

    pub fn candidate_identities(&self) -> &[SpinStructureSearchCandidateIdentity] {
        &self.candidate_identities
    }

    pub fn candidate_count(&self) -> usize {
        self.candidate_identities.len()
    }
}

fn validate_minimum(
    query: &SpinStructureQuery,
    report: &SpinStructureReport,
) -> Result<(), SpinStructureSearchResultError> {
    let actual_minimum = report
        .outcomes()
        .map(SpinStructureOutcome::placement_count)
        .min()
        .map(|count| count as u8);
    if report.minimum_placements != actual_minimum {
        return Err(SpinStructureSearchResultError::MinimumPlacementMismatch);
    }
    if query.minimality == MinimalityPolicy::MinimumPieceCount
        && actual_minimum.is_some_and(|minimum| {
            report
                .outcomes()
                .any(|outcome| outcome.placement_count() != usize::from(minimum))
        })
    {
        return Err(SpinStructureSearchResultError::MinimumPieceFamilyMismatch);
    }
    Ok(())
}

fn validate_outcomes(
    query: &SpinStructureQuery,
    report: &SpinStructureReport,
) -> Result<Vec<SpinStructureSearchCandidateIdentity>, SpinStructureSearchResultError> {
    let mut all_keys = BTreeSet::new();
    let mut identities = Vec::with_capacity(report.outcome_count());
    for (mini_partition, outcomes) in [(false, &report.regular), (true, &report.mini)] {
        let mut previous_key: Option<CanonicalOutcomeKey> = None;
        for (candidate_index, outcome) in outcomes.iter().enumerate() {
            if outcome.is_mini() != mini_partition {
                return Err(SpinStructureSearchResultError::OutcomePartitionMismatch {
                    mini_partition,
                    candidate_index,
                });
            }
            validate_outcome(query, outcome).map_err(|field| {
                SpinStructureSearchResultError::InvalidOutcome {
                    mini_partition,
                    candidate_index,
                    field,
                }
            })?;
            let key = canonical_outcome_key(outcome);
            if previous_key
                .as_ref()
                .is_some_and(|previous| key < *previous)
            {
                return Err(SpinStructureSearchResultError::CandidateOrderInvalid {
                    mini_partition,
                    candidate_index,
                });
            }
            previous_key = Some(key.clone());
            if !all_keys.insert(key.clone()) {
                return Err(SpinStructureSearchResultError::DuplicateCandidateId);
            }
            identities.push(SpinStructureSearchCandidateIdentity {
                candidate_id: canonical_candidate_id(&key),
                mini: mini_partition,
                placement_count: outcome.placement_count(),
            });
        }
    }
    Ok(identities)
}

fn validate_outcome(
    query: &SpinStructureQuery,
    outcome: &SpinStructureOutcome,
) -> Result<(), &'static str> {
    let operations = outcome.logical_operations();
    if operations.is_empty() || operations.len() != outcome.placement_count() {
        return Err("operation_count");
    }
    if outcome.placement_count() > usize::from(query.placement_limit()) {
        return Err("placement_limit");
    }
    if !operations.contains(&outcome.logical_spin()) {
        return Err("logical_spin_membership");
    }
    if outcome.logical_spin_cleared_rows().count_ones() as u8 != outcome.spin.cleared_lines {
        return Err("spin_cleared_rows");
    }
    if !query.line_requirement.accepts(outcome.spin.cleared_lines) {
        return Err("line_requirement");
    }
    if query.mode.t_only() && outcome.logical_spin().piece() != PieceKind::T {
        return Err("spin_piece");
    }
    if board_outside_height(outcome.board_before_spin, query.height)
        || board_outside_height(outcome.final_board, query.height)
        || operations
            .iter()
            .any(|operation| board_outside_height(operation.mask(), query.height))
    {
        return Err("height");
    }
    for piece in PieceKind::STANDARD_TETROMINOES {
        let used = operations
            .iter()
            .filter(|operation| operation.piece() == piece)
            .count();
        if used > usize::from(query.inventory.count(piece)) {
            return Err("inventory");
        }
    }
    Ok(())
}

fn canonical_outcome_key(outcome: &SpinStructureOutcome) -> CanonicalOutcomeKey {
    let mut operations = outcome.logical_operations().to_vec();
    operations.sort_unstable();
    CanonicalOutcomeKey {
        operations,
        target: outcome.logical_spin(),
        target_cleared_rows: outcome.logical_spin_cleared_rows(),
    }
}

fn canonical_candidate_id(key: &CanonicalOutcomeKey) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.spin-structure-candidate.v1\0");
    hasher.update((key.operations.len() as u128).to_be_bytes());
    for operation in &key.operations {
        hash_operation(&mut hasher, *operation);
    }
    hash_operation(&mut hasher, key.target);
    hasher.update(key.target_cleared_rows.to_be_bytes());
    format!(
        "spin-structure-candidate.v1:{}",
        hex_sha256(hasher.finalize())
    )
}

pub fn spin_structure_search_candidate_id(outcome: &SpinStructureOutcome) -> String {
    canonical_candidate_id(&canonical_outcome_key(outcome))
}

fn spin_structure_identities(query: &SpinStructureQuery) -> SpinStructureSearchIdentities {
    let query_sha256 = canonical_query_sha256(query);
    let supply_sha256 = canonical_supply_sha256(query);
    let mut universe = Sha256::new();
    universe.update(b"clearra.spin-structure-search-universe.v1\0");
    hash_text(&mut universe, &query_sha256);
    hash_text(&mut universe, &supply_sha256);
    hash_text(&mut universe, query.rule_profile.as_str());
    hash_text(&mut universe, query.mode.profile().id().as_str());
    SpinStructureSearchIdentities {
        query_sha256,
        rule_profile: format!(
            "spin-structure-rule-profile.v1:{}",
            query.rule_profile.as_str()
        ),
        spin_profile: format!(
            "spin-structure-spin-profile.v1:{}",
            query.mode.profile().id().as_str()
        ),
        supply_sha256,
        universe_sha256: hex_sha256(universe.finalize()),
        product_build: product_build_identity_component(),
    }
}

fn canonical_query_sha256(query: &SpinStructureQuery) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.spin-structure-search-query.v1\0");
    for word in query.initial_board.words() {
        hasher.update(word.to_be_bytes());
    }
    hasher.update([query.height]);
    hasher.update(query.inventory.counts());
    hash_text(&mut hasher, query.mode.as_str());
    hash_text(&mut hasher, &query.line_requirement.as_str());
    hasher.update([query.fill_bottom, query.fill_top]);
    hash_text(&mut hasher, query.rule_profile.as_str());
    match query.max_placements {
        Some(limit) => hasher.update([1, limit]),
        None => hasher.update([0, 0]),
    }
    hash_text(&mut hasher, query.minimality.as_str());
    hex_sha256(hasher.finalize())
}

fn canonical_supply_sha256(query: &SpinStructureQuery) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.spin-structure-inventory.v1\0");
    hasher.update(query.inventory.counts());
    hex_sha256(hasher.finalize())
}

fn hash_operation(hasher: &mut Sha256, operation: StructureOperation) {
    hasher.update([operation.piece().as_ascii() as u8]);
    hasher.update([operation.rotation().quarter_turns()]);
    hasher.update(operation.x().to_be_bytes());
    hasher.update(operation.y().to_be_bytes());
    for word in operation.mask().words() {
        hasher.update(word.to_be_bytes());
    }
    hasher.update(operation.need_deleted_rows().to_be_bytes());
}

fn board_outside_height(board: StructureBoard, height: u8) -> bool {
    (height..StructureBoard::MAX_HEIGHT).any(|row| board.row_bits(row) != 0)
}

fn product_build_identity_component() -> String {
    let identity = clearra_host_contract::ProductBuildIdentity::current();
    format!(
        "product-build.v1:{}:{}:{}:{}:{}",
        identity.engine_build_id(),
        identity.source_commit(),
        identity.contract_schema_version(),
        identity.supply_semantics_id(),
        identity.artifact_schema_version(),
    )
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
