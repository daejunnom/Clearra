use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_problem::{
    SetupCandidatePriority, SetupCycleResetBorrowPolicy, SetupHoldPolicy, SetupLengthPreference,
    SetupQueueInput, SetupSearchQuery,
};
use sha2::{Digest, Sha256};

const SETUP_QUERY_IDENTITY_SCHEMA: &[u8] = b"clearra.setup-ranking-query.v1\0";
const SETUP_SUPPLY_IDENTITY_SCHEMA: &[u8] = b"clearra.setup-ranking-supply.v1\0";
const SETUP_UNIVERSE_IDENTITY_SCHEMA: &[u8] = b"clearra.setup-ranking-universe.v1\0";

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SetupRankingKind {
    Joint,
    Build,
    ConditionalPc,
}

impl SetupRankingKind {
    pub const fn capability_id(self) -> &'static str {
        match self {
            Self::Joint => "setup.joint",
            Self::Build => "setup.build",
            Self::ConditionalPc => "setup.pc",
        }
    }

    pub const fn result_schema(self) -> &'static str {
        match self {
            Self::Joint => "setup-joint-ranking.v2",
            Self::Build => "setup-build-ranking.v2",
            Self::ConditionalPc => "setup-pc-ranking.v2",
        }
    }

    pub const fn candidate_priority(self) -> SetupCandidatePriority {
        match self {
            Self::Joint => SetupCandidatePriority::All,
            Self::Build => SetupCandidatePriority::BuildProbabilityFirst,
            Self::ConditionalPc => SetupCandidatePriority::PcProbabilityFirst,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupRankingIdentities {
    query_sha256: String,
    rule_profile: String,
    supply_sha256: String,
    universe_sha256: String,
    product_build: String,
}

impl SetupRankingIdentities {
    pub fn query_sha256(&self) -> &str {
        &self.query_sha256
    }

    pub fn rule_profile(&self) -> &str {
        &self.rule_profile
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

#[derive(Clone, Debug, PartialEq)]
pub struct SetupRankingContract {
    kind: SetupRankingKind,
    expected_query: SetupSearchQuery,
    identities: SetupRankingIdentities,
    resolved_length_preference: SetupLengthPreference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupRankingContractError {
    CandidatePriorityMismatch {
        expected: SetupCandidatePriority,
        actual: SetupCandidatePriority,
    },
    PathDetailIsNotRankedFamily,
    QueryIdentityMismatch,
}

impl SetupRankingContract {
    pub fn bind(
        kind: SetupRankingKind,
        query: &SetupSearchQuery,
    ) -> Result<Self, SetupRankingContractError> {
        let expected = kind.candidate_priority();
        let actual = query.candidate_priority();
        if actual != expected {
            return Err(SetupRankingContractError::CandidatePriorityMismatch { expected, actual });
        }
        if query.path_detail().is_some() {
            return Err(SetupRankingContractError::PathDetailIsNotRankedFamily);
        }
        Ok(Self {
            kind,
            expected_query: query.clone(),
            identities: setup_ranking_identities(query),
            resolved_length_preference: query
                .length_preference()
                .resolve(query.candidate_priority()),
        })
    }

    pub fn validate_query(
        &self,
        query: &SetupSearchQuery,
    ) -> Result<(), SetupRankingContractError> {
        if query.candidate_priority() != self.kind.candidate_priority() {
            return Err(SetupRankingContractError::CandidatePriorityMismatch {
                expected: self.kind.candidate_priority(),
                actual: query.candidate_priority(),
            });
        }
        if query.path_detail().is_some() {
            return Err(SetupRankingContractError::PathDetailIsNotRankedFamily);
        }
        let actual_identities = setup_ranking_identities(query);
        if query != &self.expected_query
            || actual_identities != self.identities
            || query
                .length_preference()
                .resolve(query.candidate_priority())
                != self.resolved_length_preference
        {
            return Err(SetupRankingContractError::QueryIdentityMismatch);
        }
        Ok(())
    }

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
}

fn setup_ranking_identities(query: &SetupSearchQuery) -> SetupRankingIdentities {
    let query_sha256 = canonical_query_sha256(query);
    let supply_sha256 = canonical_supply_sha256(query);
    let mut universe = Sha256::new();
    universe.update(SETUP_UNIVERSE_IDENTITY_SCHEMA);
    hash_text(&mut universe, &query_sha256);
    hash_text(&mut universe, &supply_sha256);
    hash_text(&mut universe, query.rule().id().as_str());
    hash_text(
        &mut universe,
        query.queue_observation_policy().coverage_semantics(),
    );
    hash_usize(&mut universe, query.limits().max_patterns());
    SetupRankingIdentities {
        query_sha256,
        rule_profile: format!("setup-rule-profile.v1:{}", query.rule().id().as_str()),
        supply_sha256,
        universe_sha256: hex_sha256(universe.finalize()),
        product_build: product_build_identity_component(),
    }
}

fn canonical_query_sha256(query: &SetupSearchQuery) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SETUP_QUERY_IDENTITY_SCHEMA);
    hasher.update(query.board_size().width().to_be_bytes());
    hasher.update(query.board_size().height().to_be_bytes());
    hasher.update([query.target().lines()]);
    hash_text(&mut hasher, query.rule().id().as_str());
    hash_queue(&mut hasher, query.queue());
    hash_hold_policy(&mut hasher, query.hold_policy());
    hash_pieces(&mut hasher, query.piece_budget().allowed_pieces());
    hasher.update([query.piece_budget().max_piece_count()]);
    hash_probability(
        &mut hasher,
        query
            .probability_filter()
            .min_probability()
            .map(|value| value.get()),
    );
    hash_probability(
        &mut hasher,
        query
            .probability_filter()
            .max_probability()
            .map(|value| value.get()),
    );
    hash_text(&mut hasher, query.grouping_mode().as_str());
    let limits = query.limits();
    for value in [
        limits.max_shape_families(),
        limits.max_tiling_variants_per_family(),
        limits.max_build_variants_per_tiling(),
        limits.max_results(),
        limits.max_patterns(),
        limits.post_pc_retained_trace_limit(),
    ] {
        hash_usize(&mut hasher, value);
    }
    hash_pieces(&mut hasher, query.residue().pieces());
    hash_text(
        &mut hasher,
        match query.cycle_reset_borrow_policy() {
            SetupCycleResetBorrowPolicy::ForbidPostCyclePieceUse => "forbid",
            SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse => "allow",
        },
    );
    hash_text(&mut hasher, query.candidate_priority().keyword());
    hash_text(&mut hasher, query.length_preference().keyword());
    hasher.update([query.max_setup_pieces()]);
    hash_text(&mut hasher, query.search_mode().keyword());
    hash_text(&mut hasher, query.queue_observation_policy().keyword());
    match query.next_cycle_remaining_pieces() {
        Some(pieces) => {
            hasher.update([1]);
            hash_pieces(&mut hasher, pieces);
        }
        None => hasher.update([0]),
    }
    match query.path_detail() {
        Some(detail) => {
            hasher.update([1]);
            hasher.update(detail.board_mask().to_be_bytes());
            hasher.update(detail.deleted_rows().to_be_bytes());
            hasher.update(detail.placement_rows().to_be_bytes());
            hash_text(&mut hasher, detail.condition_id());
        }
        None => hasher.update([0]),
    }
    hasher.update([u8::from(query.tablebase_requested())]);
    hex_sha256(hasher.finalize())
}

fn canonical_supply_sha256(query: &SetupSearchQuery) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SETUP_SUPPLY_IDENTITY_SCHEMA);
    hash_queue(&mut hasher, query.queue());
    hash_hold_policy(&mut hasher, query.hold_policy());
    hash_pieces(&mut hasher, query.residue().pieces());
    hash_text(
        &mut hasher,
        match query.cycle_reset_borrow_policy() {
            SetupCycleResetBorrowPolicy::ForbidPostCyclePieceUse => "forbid",
            SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse => "allow",
        },
    );
    match query.next_cycle_remaining_pieces() {
        Some(pieces) => {
            hasher.update([1]);
            hash_pieces(&mut hasher, pieces);
        }
        None => hasher.update([0]),
    }
    hash_text(&mut hasher, query.queue_observation_policy().keyword());
    hex_sha256(hasher.finalize())
}

fn hash_queue(hasher: &mut Sha256, queue: &SetupQueueInput) {
    match queue {
        SetupQueueInput::FixedSequence(sequence) => {
            hash_text(hasher, "fixed");
            hash_pieces(hasher, sequence.pieces());
        }
        SetupQueueInput::BagAlignedPattern(pattern) => {
            hash_text(hasher, "bag-aligned-pattern");
            hash_pieces(hasher, pattern.pieces());
        }
        SetupQueueInput::Observed(queue) => {
            hash_text(hasher, "observed");
            hash_pieces(hasher, queue.pieces());
        }
    }
}

fn hash_hold_policy(hasher: &mut Sha256, policy: SetupHoldPolicy) {
    match policy {
        SetupHoldPolicy::Disabled => hash_text(hasher, "disabled"),
        SetupHoldPolicy::EnabledEmpty => hash_text(hasher, "enabled-empty"),
        SetupHoldPolicy::EnabledWithPiece(piece) => {
            hash_text(hasher, "enabled-with-piece");
            hasher.update([piece.as_ascii() as u8]);
        }
    }
}

fn hash_pieces(hasher: &mut Sha256, pieces: &[PieceKind]) {
    hash_usize(hasher, pieces.len());
    for piece in pieces {
        hasher.update([piece.as_ascii() as u8]);
    }
}

fn hash_probability(hasher: &mut Sha256, value: Option<f64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_bits().to_be_bytes());
        }
        None => hasher.update([0]),
    }
}

fn hash_usize(hasher: &mut Sha256, value: usize) {
    hasher.update((value as u128).to_be_bytes());
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hash_usize(hasher, value.len());
    hasher.update(value.as_bytes());
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
