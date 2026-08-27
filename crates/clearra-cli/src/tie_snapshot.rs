use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

use clearra_app::{
    AppResponse, CoveragePortfolioAlternativeSet, PortfolioAlternativeCheckpoint,
    PortfolioAlternativePage, PortfolioAlternativeSetIdentity, ProductBuildIdentity,
    ProductPageSourceOwner, PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT,
    PORTFOLIO_ALTERNATIVE_SET_CONTRACT, PORTFOLIO_SNAPSHOT_CONTRACT,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::CliErrorCode;

const SNAPSHOT_FILE_CONTRACT: &str = "clearra.portfolio-snapshot-file.v1";
const CURSOR_CONTRACT: &str = "clearra.portfolio-cursor.v1";
const CURSOR_PREFIX: &str = "cpt1";
const MAX_SNAPSHOT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 64 * 1024 * 1024;
const HMAC_BLOCK_BYTES: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExplicitPortfolioMember {
    candidate_id_decimal: String,
    normalized_key: String,
}

impl ExplicitPortfolioMember {
    pub(crate) fn candidate_id_decimal(&self) -> &str {
        &self.candidate_id_decimal
    }

    pub(crate) fn normalized_key(&self) -> &str {
        &self.normalized_key
    }

    #[cfg(test)]
    pub(crate) fn for_test(candidate_id_decimal: &str, normalized_key: &str) -> Self {
        Self {
            candidate_id_decimal: candidate_id_decimal.to_owned(),
            normalized_key: normalized_key.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExplicitPortfolioOutput {
    set_contract: &'static str,
    page_contract: &'static str,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    alternative_index_decimal: Option<String>,
    optimal_cardinality: usize,
    members: Vec<ExplicitPortfolioMember>,
    known_alternative_count_decimal: String,
    total_alternative_count_decimal: Option<String>,
    enumeration_complete: bool,
    cursor: Option<String>,
}

impl ExplicitPortfolioOutput {
    pub(crate) const fn set_contract(&self) -> &'static str {
        self.set_contract
    }

    pub(crate) const fn page_contract(&self) -> &'static str {
        self.page_contract
    }

    pub(crate) fn set_identity_sha256(&self) -> &str {
        &self.set_identity_sha256
    }

    pub(crate) fn candidate_map_sha256(&self) -> &str {
        &self.candidate_map_sha256
    }

    pub(crate) fn alternative_index_decimal(&self) -> Option<&str> {
        self.alternative_index_decimal.as_deref()
    }

    pub(crate) const fn optimal_cardinality(&self) -> usize {
        self.optimal_cardinality
    }

    pub(crate) fn members(&self) -> &[ExplicitPortfolioMember] {
        &self.members
    }

    pub(crate) fn known_alternative_count_decimal(&self) -> &str {
        &self.known_alternative_count_decimal
    }

    pub(crate) fn total_alternative_count_decimal(&self) -> Option<&str> {
        self.total_alternative_count_decimal.as_deref()
    }

    pub(crate) const fn enumeration_complete(&self) -> bool {
        self.enumeration_complete
    }

    pub(crate) fn cursor(&self) -> Option<&str> {
        self.cursor.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        set_identity_sha256: &str,
        candidate_map_sha256: &str,
        alternative_index_decimal: Option<&str>,
        optimal_cardinality: usize,
        members: Vec<ExplicitPortfolioMember>,
        known_alternative_count_decimal: &str,
        total_alternative_count_decimal: Option<&str>,
        enumeration_complete: bool,
        cursor: Option<&str>,
    ) -> Self {
        Self {
            set_contract: PORTFOLIO_ALTERNATIVE_SET_CONTRACT,
            page_contract: PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT,
            set_identity_sha256: set_identity_sha256.to_owned(),
            candidate_map_sha256: candidate_map_sha256.to_owned(),
            alternative_index_decimal: alternative_index_decimal.map(ToOwned::to_owned),
            optimal_cardinality,
            members,
            known_alternative_count_decimal: known_alternative_count_decimal.to_owned(),
            total_alternative_count_decimal: total_alternative_count_decimal.map(ToOwned::to_owned),
            enumeration_complete,
            cursor: cursor.map(ToOwned::to_owned),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TieSnapshotError {
    UnsafePath,
    TargetExists,
    Locked,
    Io,
    Format,
    Tampered,
    StaleCursor,
    QueryMismatch,
    BuildMismatch,
    CandidateMapMismatch,
    Enumeration,
}

impl TieSnapshotError {
    pub(crate) const fn code(self) -> CliErrorCode {
        match self {
            Self::UnsafePath => CliErrorCode::TieSnapshotUnsafePath,
            Self::TargetExists => CliErrorCode::TieSnapshotTargetExists,
            Self::Locked => CliErrorCode::TieSnapshotLocked,
            Self::Io => CliErrorCode::TieSnapshotIo,
            Self::Format | Self::Enumeration => CliErrorCode::TieSnapshotInvalid,
            Self::Tampered => CliErrorCode::TieSnapshotTampered,
            Self::StaleCursor => CliErrorCode::TieSnapshotStale,
            Self::QueryMismatch => CliErrorCode::TieSnapshotQueryMismatch,
            Self::BuildMismatch => CliErrorCode::TieSnapshotBuildMismatch,
            Self::CandidateMapMismatch => CliErrorCode::TieSnapshotCandidateMismatch,
        }
    }

    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::UnsafePath => "tie-snapshot-path-unsafe",
            Self::TargetExists => "tie-snapshot-target-already-exists",
            Self::Locked => "tie-snapshot-concurrent-writer",
            Self::Io => "tie-snapshot-io-failed",
            Self::Format => "tie-snapshot-format-invalid",
            Self::Tampered => "tie-snapshot-authentication-failed",
            Self::StaleCursor => "tie-snapshot-cursor-stale",
            Self::QueryMismatch => "tie-snapshot-query-mismatch",
            Self::BuildMismatch => "tie-snapshot-build-mismatch",
            Self::CandidateMapMismatch => "tie-snapshot-candidate-map-mismatch",
            Self::Enumeration => "tie-snapshot-enumeration-failed",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SnapshotIdentity {
    query: String,
    source: String,
    profile: String,
    universe: String,
    build: String,
}

impl SnapshotIdentity {
    fn from_set(set: &CoveragePortfolioAlternativeSet) -> Self {
        let identity = set.identity();
        Self {
            query: identity.query_identity().to_owned(),
            source: identity.source_identity().to_owned(),
            profile: identity.profile_identity().to_owned(),
            universe: identity.universe_identity().to_owned(),
            build: identity.build_identity().to_owned(),
        }
    }

    fn into_set_identity(self) -> Result<PortfolioAlternativeSetIdentity, TieSnapshotError> {
        PortfolioAlternativeSetIdentity::new(
            self.query,
            self.source,
            self.profile,
            self.universe,
            self.build,
        )
        .map_err(|_| TieSnapshotError::Format)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SnapshotHeader {
    contract: String,
    set_contract: String,
    snapshot_contract: String,
    identity: SnapshotIdentity,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    pattern_count_decimal: String,
    required_words_hex: Vec<String>,
    candidate_keys: Vec<String>,
    #[serde(default)]
    public_candidate_ids_decimal: Option<Vec<String>>,
    coverage_rows_hex: Vec<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SnapshotPage {
    page_contract: String,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    alternative_index_decimal: String,
    candidate_ids_decimal: Vec<String>,
    optimal_cardinality_decimal: String,
    known_alternative_count_decimal: String,
    total_alternative_count_decimal: Option<String>,
    enumeration_complete: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SnapshotCheckpoint {
    snapshot_contract: String,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    optimal_cardinality_decimal: String,
    next_combination_decimal: Option<Vec<String>>,
    known_alternative_count_decimal: String,
    enumeration_complete: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "record_type", rename_all = "kebab-case")]
enum SnapshotPayload {
    Header { value: SnapshotHeader },
    Page { value: SnapshotPage },
    Checkpoint { value: SnapshotCheckpoint },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SignedRecord {
    payload: SnapshotPayload,
    mac_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct CursorPayload {
    contract: String,
    path_identity_sha256: String,
    set_identity_sha256: String,
    candidate_map_sha256: String,
    query_identity: String,
    build_identity: String,
    known_alternative_count_decimal: String,
}

pub(crate) fn initialize_snapshot(
    response: &AppResponse,
    requested_path: &str,
) -> Result<ExplicitPortfolioOutput, TieSnapshotError> {
    let Some(ProductPageSourceOwner::CoveragePortfolio(set)) = response.public_page_source_owner()
    else {
        return Err(TieSnapshotError::Enumeration);
    };
    initialize_snapshot_from_set(&set, requested_path)
}

fn initialize_snapshot_from_set(
    set: &CoveragePortfolioAlternativeSet,
    requested_path: &str,
) -> Result<ExplicitPortfolioOutput, TieSnapshotError> {
    let path = safe_snapshot_path(requested_path, false)?;
    let mut secret = [0_u8; 32];
    getrandom::fill(&mut secret).map_err(|_| TieSnapshotError::Io)?;

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                TieSnapshotError::TargetExists
            } else {
                TieSnapshotError::Io
            }
        })?;
    acquire_exclusive_lock(&file)?;
    reject_file_reparse(&file)?;
    let canonical_path = fs::canonicalize(&path).map_err(|_| TieSnapshotError::Io)?;
    let path_identity_sha256 = path_identity(&canonical_path);

    let header = header_from_set(set)?;
    let page = SnapshotPage::from_page(set.canonical_page());
    let store = set
        .open_store()
        .map_err(|_| TieSnapshotError::Enumeration)?;
    let checkpoint = SnapshotCheckpoint::from_checkpoint(&store.checkpoint());
    append_signed_payload(
        &mut file,
        &SnapshotPayload::Header {
            value: header.clone(),
        },
        &secret,
    )?;
    append_signed_payload(
        &mut file,
        &SnapshotPayload::Page {
            value: page.clone(),
        },
        &secret,
    )?;
    append_signed_payload(
        &mut file,
        &SnapshotPayload::Checkpoint {
            value: checkpoint.clone(),
        },
        &secret,
    )?;
    file.sync_all().map_err(|_| TieSnapshotError::Io)?;

    let cursor = if checkpoint.enumeration_complete {
        None
    } else {
        Some(cursor_for(
            &secret,
            CursorPayload {
                contract: CURSOR_CONTRACT.to_owned(),
                path_identity_sha256,
                set_identity_sha256: header.set_identity_sha256,
                candidate_map_sha256: header.candidate_map_sha256,
                query_identity: header.identity.query,
                build_identity: header.identity.build,
                known_alternative_count_decimal: checkpoint.known_alternative_count_decimal,
            },
        )?)
    };
    output_from_page(set, Some(set.canonical_page()), cursor)
}

pub(crate) fn continue_snapshot(
    requested_path: &str,
    encoded_cursor: &str,
) -> Result<ExplicitPortfolioOutput, TieSnapshotError> {
    let (secret, cursor) = parse_cursor(encoded_cursor)?;
    let path = safe_snapshot_path(requested_path, true)?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|_| TieSnapshotError::Io)?;
    acquire_exclusive_lock(&file)?;
    reject_file_reparse(&file)?;
    let canonical_path = fs::canonicalize(&path).map_err(|_| TieSnapshotError::Io)?;
    if path_identity(&canonical_path) != cursor.path_identity_sha256 {
        return Err(TieSnapshotError::QueryMismatch);
    }

    let records = read_and_authenticate_records(&mut file, &secret)?;
    let (header, first_page, checkpoint) = validated_snapshot_tail(&records)?;
    if header.identity.query != cursor.query_identity {
        return Err(TieSnapshotError::QueryMismatch);
    }
    let current_build = product_build_identity_component(&ProductBuildIdentity::current());
    if header.identity.build != current_build || cursor.build_identity != current_build {
        return Err(TieSnapshotError::BuildMismatch);
    }
    if header.candidate_map_sha256 != cursor.candidate_map_sha256 {
        return Err(TieSnapshotError::CandidateMapMismatch);
    }
    if header.set_identity_sha256 != cursor.set_identity_sha256 {
        return Err(TieSnapshotError::QueryMismatch);
    }
    if checkpoint.known_alternative_count_decimal != cursor.known_alternative_count_decimal {
        return Err(TieSnapshotError::StaleCursor);
    }

    let set = set_from_header(header, first_page)?;
    let restart = checkpoint.to_checkpoint()?;
    let mut store = set
        .resume_store(&restart)
        .map_err(|_| TieSnapshotError::Enumeration)?;
    let advance = store
        .next_page(u64::MAX, &mut || false)
        .map_err(|_| TieSnapshotError::Enumeration)?;
    let next_checkpoint = SnapshotCheckpoint::from_checkpoint(advance.checkpoint());
    if let Some(page) = advance.page() {
        append_signed_payload(
            &mut file,
            &SnapshotPayload::Page {
                value: SnapshotPage::from_page(page),
            },
            &secret,
        )?;
    }
    append_signed_payload(
        &mut file,
        &SnapshotPayload::Checkpoint {
            value: next_checkpoint.clone(),
        },
        &secret,
    )?;
    file.sync_all().map_err(|_| TieSnapshotError::Io)?;

    let next_cursor = if next_checkpoint.enumeration_complete {
        None
    } else {
        Some(cursor_for(
            &secret,
            CursorPayload {
                contract: CURSOR_CONTRACT.to_owned(),
                path_identity_sha256: cursor.path_identity_sha256,
                set_identity_sha256: cursor.set_identity_sha256,
                candidate_map_sha256: cursor.candidate_map_sha256,
                query_identity: cursor.query_identity,
                build_identity: cursor.build_identity,
                known_alternative_count_decimal: next_checkpoint
                    .known_alternative_count_decimal
                    .clone(),
            },
        )?)
    };
    let mut output = output_from_page(&set, advance.page(), next_cursor)?;
    if advance.page().is_none() {
        output.known_alternative_count_decimal =
            next_checkpoint.known_alternative_count_decimal.clone();
        output.enumeration_complete = next_checkpoint.enumeration_complete;
        output.total_alternative_count_decimal = next_checkpoint
            .enumeration_complete
            .then(|| next_checkpoint.known_alternative_count_decimal.clone());
    }
    Ok(output)
}

fn header_from_set(
    set: &CoveragePortfolioAlternativeSet,
) -> Result<SnapshotHeader, TieSnapshotError> {
    let pattern_count = set.required_patterns().pattern_count();
    if set
        .coverage_rows()
        .iter()
        .any(|row| row.pattern_count() != pattern_count)
    {
        return Err(TieSnapshotError::Format);
    }
    let public_candidate_ids = set
        .candidates()
        .iter()
        .map(|candidate| {
            set.public_candidate_id(candidate.candidate_id())
                .ok_or(TieSnapshotError::CandidateMapMismatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let has_distinct_public_candidate_ids = public_candidate_ids
        .iter()
        .zip(set.candidates())
        .any(|(public_id, candidate)| *public_id != candidate.candidate_id());
    Ok(SnapshotHeader {
        contract: SNAPSHOT_FILE_CONTRACT.to_owned(),
        set_contract: set.contract_id().to_owned(),
        snapshot_contract: PORTFOLIO_SNAPSHOT_CONTRACT.to_owned(),
        identity: SnapshotIdentity::from_set(set),
        set_identity_sha256: set.set_identity_sha256().to_owned(),
        candidate_map_sha256: set.candidate_map_sha256().to_owned(),
        pattern_count_decimal: pattern_count.to_string(),
        required_words_hex: words_hex(set.required_patterns()),
        candidate_keys: set
            .candidates()
            .iter()
            .map(|candidate| candidate.normalized_key().to_owned())
            .collect(),
        public_candidate_ids_decimal: has_distinct_public_candidate_ids.then(|| {
            public_candidate_ids
                .iter()
                .map(ToString::to_string)
                .collect()
        }),
        coverage_rows_hex: set.coverage_rows().iter().map(words_hex).collect(),
    })
}

fn set_from_header(
    header: &SnapshotHeader,
    first_page: &SnapshotPage,
) -> Result<CoveragePortfolioAlternativeSet, TieSnapshotError> {
    if header.contract != SNAPSHOT_FILE_CONTRACT
        || header.set_contract != PORTFOLIO_ALTERNATIVE_SET_CONTRACT
        || header.snapshot_contract != PORTFOLIO_SNAPSHOT_CONTRACT
        || first_page.page_contract != PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT
    {
        return Err(TieSnapshotError::Format);
    }
    let pattern_count = parse_usize_decimal(&header.pattern_count_decimal)?;
    let required =
        PatternBitSet::from_words(pattern_count, words_from_hex(&header.required_words_hex)?)
            .map_err(|_| TieSnapshotError::Format)?;
    let rows = header
        .coverage_rows_hex
        .iter()
        .map(|words| {
            PatternBitSet::from_words(pattern_count, words_from_hex(words)?)
                .map_err(|_| TieSnapshotError::Format)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if header.candidate_keys.len() != rows.len() {
        return Err(TieSnapshotError::CandidateMapMismatch);
    }
    let expected_canonical_keys = first_page
        .candidate_ids_decimal
        .iter()
        .map(|candidate_id| {
            let index = parse_u64_decimal(candidate_id)?
                .checked_sub(1)
                .and_then(|index| usize::try_from(index).ok())
                .ok_or(TieSnapshotError::Format)?;
            header
                .candidate_keys
                .get(index)
                .cloned()
                .ok_or(TieSnapshotError::CandidateMapMismatch)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut set = CoveragePortfolioAlternativeSet::new(
        header.identity.clone().into_set_identity()?,
        header.candidate_keys.clone(),
        required,
        rows,
        &expected_canonical_keys,
    )
    .map_err(|_| TieSnapshotError::Enumeration)?;
    if let Some(candidate_ids) = &header.public_candidate_ids_decimal {
        if candidate_ids.len() != header.candidate_keys.len() {
            return Err(TieSnapshotError::CandidateMapMismatch);
        }
        set = set
            .with_public_candidate_ids(
                candidate_ids
                    .iter()
                    .map(|candidate_id| parse_u64_decimal(candidate_id))
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .map_err(|_| TieSnapshotError::CandidateMapMismatch)?;
    }
    if set.set_identity_sha256() != header.set_identity_sha256
        || set.candidate_map_sha256() != header.candidate_map_sha256
    {
        return Err(TieSnapshotError::CandidateMapMismatch);
    }
    if SnapshotPage::from_page(set.canonical_page()).semantic_fields()
        != first_page.semantic_fields()
    {
        return Err(TieSnapshotError::Tampered);
    }
    Ok(set)
}

fn validated_snapshot_tail(
    records: &[SnapshotPayload],
) -> Result<(&SnapshotHeader, &SnapshotPage, &SnapshotCheckpoint), TieSnapshotError> {
    let Some(SnapshotPayload::Header { value: header }) = records.first() else {
        return Err(TieSnapshotError::Format);
    };
    let Some(SnapshotPayload::Page { value: first_page }) = records.get(1) else {
        return Err(TieSnapshotError::Format);
    };
    let Some(SnapshotPayload::Checkpoint { value: checkpoint }) = records.last() else {
        return Err(TieSnapshotError::Format);
    };
    if records.len() < 3
        || records[1..].iter().enumerate().any(|(index, payload)| {
            if index % 2 == 0 {
                !matches!(payload, SnapshotPayload::Page { .. })
            } else {
                !matches!(payload, SnapshotPayload::Checkpoint { .. })
            }
        })
        || header.set_identity_sha256 != checkpoint.set_identity_sha256
        || header.candidate_map_sha256 != checkpoint.candidate_map_sha256
    {
        return Err(TieSnapshotError::Format);
    }
    Ok((header, first_page, checkpoint))
}

impl SnapshotPage {
    fn from_page(page: &PortfolioAlternativePage) -> Self {
        Self {
            page_contract: page.contract_id().to_owned(),
            set_identity_sha256: page.set_identity_sha256().to_owned(),
            candidate_map_sha256: page.candidate_map_sha256().to_owned(),
            alternative_index_decimal: page.alternative_index_decimal().to_owned(),
            candidate_ids_decimal: page
                .portfolio()
                .candidate_ids()
                .iter()
                .map(ToString::to_string)
                .collect(),
            optimal_cardinality_decimal: page.optimal_cardinality().to_string(),
            known_alternative_count_decimal: page.known_alternative_count_decimal().to_owned(),
            total_alternative_count_decimal: page
                .total_alternative_count_decimal()
                .map(ToOwned::to_owned),
            enumeration_complete: page.enumeration_complete(),
        }
    }

    fn semantic_fields(
        &self,
    ) -> (
        &str,
        &str,
        &str,
        &str,
        &[String],
        &str,
        &str,
        Option<&str>,
        bool,
    ) {
        (
            &self.page_contract,
            &self.set_identity_sha256,
            &self.candidate_map_sha256,
            &self.alternative_index_decimal,
            &self.candidate_ids_decimal,
            &self.optimal_cardinality_decimal,
            &self.known_alternative_count_decimal,
            self.total_alternative_count_decimal.as_deref(),
            self.enumeration_complete,
        )
    }
}

impl SnapshotCheckpoint {
    fn from_checkpoint(checkpoint: &PortfolioAlternativeCheckpoint) -> Self {
        Self {
            snapshot_contract: checkpoint.contract_id().to_owned(),
            set_identity_sha256: checkpoint.set_identity_sha256().to_owned(),
            candidate_map_sha256: checkpoint.candidate_map_sha256().to_owned(),
            optimal_cardinality_decimal: checkpoint.optimal_cardinality().to_string(),
            next_combination_decimal: checkpoint
                .next_combination()
                .map(|combination| combination.iter().map(ToString::to_string).collect()),
            known_alternative_count_decimal: checkpoint
                .known_alternative_count_decimal()
                .to_owned(),
            enumeration_complete: checkpoint.enumeration_complete(),
        }
    }

    fn to_checkpoint(&self) -> Result<PortfolioAlternativeCheckpoint, TieSnapshotError> {
        if self.snapshot_contract != PORTFOLIO_SNAPSHOT_CONTRACT {
            return Err(TieSnapshotError::Format);
        }
        let next_combination = self
            .next_combination_decimal
            .as_ref()
            .map(|combination| {
                combination
                    .iter()
                    .map(|value| parse_usize_decimal(value))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        PortfolioAlternativeCheckpoint::from_restart_fields(
            self.set_identity_sha256.clone(),
            self.candidate_map_sha256.clone(),
            parse_usize_decimal(&self.optimal_cardinality_decimal)?,
            next_combination,
            self.known_alternative_count_decimal.clone(),
            self.enumeration_complete,
        )
        .map_err(|_| TieSnapshotError::Format)
    }
}

fn output_from_page(
    set: &CoveragePortfolioAlternativeSet,
    page: Option<&PortfolioAlternativePage>,
    cursor: Option<String>,
) -> Result<ExplicitPortfolioOutput, TieSnapshotError> {
    let members = page
        .map(|page| {
            page.portfolio()
                .candidate_ids()
                .iter()
                .map(|candidate_id| {
                    let index = candidate_id
                        .checked_sub(1)
                        .and_then(|value| usize::try_from(value).ok())
                        .ok_or(TieSnapshotError::CandidateMapMismatch)?;
                    let candidate = set
                        .candidates()
                        .get(index)
                        .filter(|candidate| candidate.candidate_id() == *candidate_id)
                        .ok_or(TieSnapshotError::CandidateMapMismatch)?;
                    Ok(ExplicitPortfolioMember {
                        candidate_id_decimal: set
                            .public_candidate_id(*candidate_id)
                            .ok_or(TieSnapshotError::CandidateMapMismatch)?
                            .to_string(),
                        normalized_key: candidate.normalized_key().to_owned(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let enumeration_complete = page.map_or(cursor.is_none(), |page| page.enumeration_complete());
    let known = page.map_or_else(
        || set.known_alternative_count_decimal().to_owned(),
        |page| page.known_alternative_count_decimal().to_owned(),
    );
    let total = page
        .and_then(PortfolioAlternativePage::total_alternative_count_decimal)
        .map(ToOwned::to_owned)
        .or_else(|| enumeration_complete.then(|| known.clone()));
    Ok(ExplicitPortfolioOutput {
        set_contract: PORTFOLIO_ALTERNATIVE_SET_CONTRACT,
        page_contract: PORTFOLIO_ALTERNATIVE_PAGE_CONTRACT,
        set_identity_sha256: set.set_identity_sha256().to_owned(),
        candidate_map_sha256: set.candidate_map_sha256().to_owned(),
        alternative_index_decimal: page
            .map(PortfolioAlternativePage::alternative_index_decimal)
            .map(ToOwned::to_owned),
        optimal_cardinality: set.optimal_cardinality(),
        members,
        known_alternative_count_decimal: known,
        total_alternative_count_decimal: total,
        enumeration_complete,
        cursor,
    })
}

fn append_signed_payload(
    file: &mut File,
    payload: &SnapshotPayload,
    secret: &[u8; 32],
) -> Result<(), TieSnapshotError> {
    let payload_bytes = serde_json::to_vec(payload).map_err(|_| TieSnapshotError::Format)?;
    let record = SignedRecord {
        payload: payload.clone(),
        mac_sha256: hex(&hmac_sha256(secret, &payload_bytes)),
    };
    let encoded = serde_json::to_vec(&record).map_err(|_| TieSnapshotError::Format)?;
    if encoded.len() > MAX_RECORD_BYTES {
        return Err(TieSnapshotError::Format);
    }
    file.seek(SeekFrom::End(0))
        .map_err(|_| TieSnapshotError::Io)?;
    file.write_all(&encoded).map_err(|_| TieSnapshotError::Io)?;
    file.write_all(b"\n").map_err(|_| TieSnapshotError::Io)
}

fn read_and_authenticate_records(
    file: &mut File,
    secret: &[u8; 32],
) -> Result<Vec<SnapshotPayload>, TieSnapshotError> {
    let length = file.metadata().map_err(|_| TieSnapshotError::Io)?.len();
    if length == 0 || length > MAX_SNAPSHOT_BYTES {
        return Err(TieSnapshotError::Format);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| TieSnapshotError::Io)?;
    let mut reader = BufReader::new(file);
    let mut records = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|_| TieSnapshotError::Io)?;
        if read == 0 {
            break;
        }
        if line.len() > MAX_RECORD_BYTES || !line.ends_with(b"\n") {
            return Err(TieSnapshotError::Format);
        }
        line.pop();
        let record: SignedRecord =
            serde_json::from_slice(&line).map_err(|_| TieSnapshotError::Format)?;
        let payload_bytes =
            serde_json::to_vec(&record.payload).map_err(|_| TieSnapshotError::Format)?;
        let supplied_mac =
            decode_fixed_hex::<32>(&record.mac_sha256).ok_or(TieSnapshotError::Tampered)?;
        if !constant_time_eq(&supplied_mac, &hmac_sha256(secret, &payload_bytes)) {
            return Err(TieSnapshotError::Tampered);
        }
        records.push(record.payload);
    }
    Ok(records)
}

fn cursor_for(secret: &[u8; 32], payload: CursorPayload) -> Result<String, TieSnapshotError> {
    let payload_bytes = serde_json::to_vec(&payload).map_err(|_| TieSnapshotError::Format)?;
    let mac = hmac_sha256(secret, &payload_bytes);
    Ok(format!(
        "{CURSOR_PREFIX}.{}.{}.{}",
        hex(secret),
        hex(&payload_bytes),
        hex(&mac),
    ))
}

fn parse_cursor(encoded: &str) -> Result<([u8; 32], CursorPayload), TieSnapshotError> {
    let mut parts = encoded.split('.');
    if parts.next() != Some(CURSOR_PREFIX) {
        return Err(TieSnapshotError::Tampered);
    }
    let secret = parts
        .next()
        .and_then(decode_fixed_hex::<32>)
        .ok_or(TieSnapshotError::Tampered)?;
    let payload_bytes = parts
        .next()
        .and_then(decode_hex)
        .filter(|bytes| bytes.len() <= 16 * 1024)
        .ok_or(TieSnapshotError::Tampered)?;
    let supplied_mac = parts
        .next()
        .and_then(decode_fixed_hex::<32>)
        .ok_or(TieSnapshotError::Tampered)?;
    if parts.next().is_some()
        || !constant_time_eq(&supplied_mac, &hmac_sha256(&secret, &payload_bytes))
    {
        return Err(TieSnapshotError::Tampered);
    }
    let payload: CursorPayload =
        serde_json::from_slice(&payload_bytes).map_err(|_| TieSnapshotError::Tampered)?;
    if payload.contract != CURSOR_CONTRACT {
        return Err(TieSnapshotError::Tampered);
    }
    Ok((secret, payload))
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut padded = [0_u8; HMAC_BLOCK_BYTES];
    if key.len() > HMAC_BLOCK_BYTES {
        padded[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        padded[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36_u8; HMAC_BLOCK_BYTES];
    let mut outer_pad = [0x5c_u8; HMAC_BLOCK_BYTES];
    for index in 0..HMAC_BLOCK_BYTES {
        inner_pad[index] ^= padded[index];
        outer_pad[index] ^= padded[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn words_hex(bitset: &PatternBitSet) -> Vec<String> {
    (0..bitset.word_count())
        .map(|index| format!("{:016x}", bitset.word_at(index)))
        .collect()
}

fn words_from_hex(words: &[String]) -> Result<Vec<u64>, TieSnapshotError> {
    words
        .iter()
        .map(|word| {
            if word.len() != 16
                || !word
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
            {
                return Err(TieSnapshotError::Format);
            }
            u64::from_str_radix(word, 16).map_err(|_| TieSnapshotError::Format)
        })
        .collect()
}

fn parse_usize_decimal(value: &str) -> Result<usize, TieSnapshotError> {
    if !canonical_decimal(value) {
        return Err(TieSnapshotError::Format);
    }
    value.parse().map_err(|_| TieSnapshotError::Format)
}

fn parse_u64_decimal(value: &str) -> Result<u64, TieSnapshotError> {
    if !canonical_decimal(value) {
        return Err(TieSnapshotError::Format);
    }
    value.parse().map_err(|_| TieSnapshotError::Format)
}

fn canonical_decimal(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn product_build_identity_component(identity: &ProductBuildIdentity) -> String {
    format!(
        "product-build.v1:{}:{}:{}:{}:{}",
        identity.engine_build_id(),
        identity.source_commit(),
        identity.contract_schema_version(),
        identity.supply_semantics_id(),
        identity.artifact_schema_version(),
    )
}

fn safe_snapshot_path(
    requested: &str,
    target_must_exist: bool,
) -> Result<PathBuf, TieSnapshotError> {
    let requested = Path::new(requested);
    if requested.as_os_str().is_empty()
        || requested
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
    {
        return Err(TieSnapshotError::UnsafePath);
    }
    let absolute = if requested.is_absolute() {
        requested.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| TieSnapshotError::Io)?
            .join(requested)
    };
    let file_name = absolute
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or(TieSnapshotError::UnsafePath)?;
    let parent = absolute.parent().ok_or(TieSnapshotError::UnsafePath)?;
    reject_reparse_ancestors(parent)?;
    let canonical_parent = fs::canonicalize(parent).map_err(|_| TieSnapshotError::UnsafePath)?;
    let resolved = canonical_parent.join(file_name);
    match fs::symlink_metadata(&resolved) {
        Ok(metadata) => {
            reject_metadata_reparse(&metadata)?;
            if !target_must_exist {
                return Err(TieSnapshotError::TargetExists);
            }
            if !metadata.is_file() {
                return Err(TieSnapshotError::UnsafePath);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !target_must_exist => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(TieSnapshotError::Io)
        }
        Err(_) => return Err(TieSnapshotError::Io),
    }
    Ok(resolved)
}

fn reject_reparse_ancestors(path: &Path) -> Result<(), TieSnapshotError> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| TieSnapshotError::UnsafePath)?;
        reject_metadata_reparse(&metadata)?;
        if !metadata.is_dir() {
            return Err(TieSnapshotError::UnsafePath);
        }
    }
    Ok(())
}

fn reject_metadata_reparse(metadata: &fs::Metadata) -> Result<(), TieSnapshotError> {
    if metadata.file_type().is_symlink() {
        return Err(TieSnapshotError::UnsafePath);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(TieSnapshotError::UnsafePath);
        }
    }
    Ok(())
}

fn reject_file_reparse(file: &File) -> Result<(), TieSnapshotError> {
    let metadata = file.metadata().map_err(|_| TieSnapshotError::Io)?;
    reject_metadata_reparse(&metadata)
}

fn acquire_exclusive_lock(file: &File) -> Result<(), TieSnapshotError> {
    file.try_lock().map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => TieSnapshotError::Locked,
        std::fs::TryLockError::Error(_) => TieSnapshotError::Io,
    })
}

fn path_identity(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.portfolio-snapshot-path.v1\0");
    update_os_path(&mut hasher, path);
    hex(&hasher.finalize())
}

#[cfg(windows)]
fn update_os_path(hasher: &mut Sha256, path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    for word in path.as_os_str().encode_wide() {
        hasher.update(word.to_le_bytes());
    }
}

#[cfg(unix)]
fn update_os_path(hasher: &mut Sha256, path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    hasher.update(path.as_os_str().as_bytes());
}

#[cfg(not(any(windows, unix)))]
fn update_os_path(hasher: &mut Sha256, path: &Path) {
    hasher.update(path.to_string_lossy().as_bytes());
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Some((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?))
        .collect()
}

fn decode_fixed_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    let decoded = decode_hex(value)?;
    decoded.try_into().ok()
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use clearra_app::{AppContext, AppCoreExecutorService, AppServices, AppStatus};
    use clearra_output::RenderFormat;

    use super::*;
    use crate::{args::CliParser, assemble::CliAppRequestAssembler};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    fn test_directory(label: &str) -> PathBuf {
        let suffix = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "clearra-tie-snapshot-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create isolated snapshot test directory");
        path
    }

    fn remove_test_directory(path: &Path) {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                let metadata = fs::symlink_metadata(&entry_path).expect("test entry metadata");
                if metadata.is_dir() && !metadata.file_type().is_symlink() {
                    fs::remove_dir(&entry_path).expect("remove empty test directory");
                } else {
                    fs::remove_file(&entry_path).expect("remove test snapshot or symlink");
                }
            }
        }
        fs::remove_dir(path).expect("remove isolated snapshot test directory");
    }

    fn test_row(pattern_count: usize, pattern: u32) -> PatternBitSet {
        PatternBitSet::from_pattern_indices(pattern_count, vec![pattern]).expect("coverage row")
    }

    fn tied_test_set() -> CoveragePortfolioAlternativeSet {
        let identity = PortfolioAlternativeSetIdentity::new(
            "query-a",
            "source-a",
            "profile-a",
            "universe-a",
            product_build_identity_component(&ProductBuildIdentity::current()),
        )
        .expect("test set identity");
        let keys = ["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let rows = vec![
            test_row(3, 0),
            test_row(3, 1),
            test_row(3, 2),
            test_row(3, 0),
            test_row(3, 1),
            test_row(3, 2),
        ];
        CoveragePortfolioAlternativeSet::new(
            identity,
            keys,
            PatternBitSet::all(3),
            rows,
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
        )
        .expect("tied test set")
    }

    fn single_portfolio_test_set() -> CoveragePortfolioAlternativeSet {
        let identity = PortfolioAlternativeSetIdentity::new(
            "query-single",
            "source-single",
            "profile-single",
            "universe-single",
            product_build_identity_component(&ProductBuildIdentity::current()),
        )
        .expect("single set identity");
        CoveragePortfolioAlternativeSet::new(
            identity,
            vec!["only".to_owned()],
            PatternBitSet::all(1),
            vec![PatternBitSet::all(1)],
            &["only".to_owned()],
        )
        .expect("single portfolio set")
    }

    #[test]
    fn build_cover_public_page_owner_initializes_the_explicit_snapshot() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let source = "clearra build cover --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --objective min-cover --workers 2";
        let invocation = CliParser::parse(source.split_whitespace())
            .expect("canonical native Build v2 cover command");
        let request =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
                .expect("typed native Build v2 cover app request")
                .request();
        let response = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        )
        .run(request);
        assert_eq!(
            response.status(),
            AppStatus::Success,
            "{:?}",
            response.error()
        );
        let owner_identity = match response.public_page_source_owner() {
            Some(ProductPageSourceOwner::CoveragePortfolio(set)) => {
                set.set_identity_sha256().to_owned()
            }
            _ => panic!("Build v2 cover response must expose its product-owned page source"),
        };

        let directory = test_directory("build-cover-owner");
        let path = directory.join("portfolios.jsonl");
        let output = initialize_snapshot(&response, &path.to_string_lossy())
            .expect("initialize from the Build v2 product owner");

        assert_eq!(output.set_identity_sha256(), owner_identity);
        assert_eq!(output.alternative_index_decimal(), Some("1"));
        assert!(!output.members().is_empty());
        remove_test_directory(&directory);
    }

    #[test]
    fn cursor_mac_rejects_tampering_and_preserves_large_decimal_fields() {
        let secret = [0x5a; 32];
        let payload = CursorPayload {
            contract: CURSOR_CONTRACT.to_owned(),
            path_identity_sha256: "a".repeat(64),
            set_identity_sha256: "b".repeat(64),
            candidate_map_sha256: "c".repeat(64),
            query_identity: "query".to_owned(),
            build_identity: "build".to_owned(),
            known_alternative_count_decimal: "184467440737095516160".to_owned(),
        };
        let encoded = cursor_for(&secret, payload).expect("cursor");
        let (_, decoded) = parse_cursor(&encoded).expect("valid cursor");
        assert_eq!(
            decoded.known_alternative_count_decimal,
            "184467440737095516160"
        );

        let mut tampered = encoded.into_bytes();
        let index = tampered.len() / 2;
        tampered[index] = if tampered[index] == b'a' { b'b' } else { b'a' };
        assert_eq!(
            parse_cursor(std::str::from_utf8(&tampered).unwrap()),
            Err(TieSnapshotError::Tampered)
        );
    }

    #[test]
    fn hmac_sha256_matches_rfc_4231_case_one() {
        let expected = decode_fixed_hex::<32>(
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
        )
        .unwrap();
        assert_eq!(hmac_sha256(&[0x0b; 20], b"Hi There"), expected);
    }

    #[test]
    fn snapshot_is_new_file_only_and_resumes_all_exact_pages() {
        let directory = test_directory("resume");
        let path = directory.join("portfolios.jsonl");
        let path_text = path.to_string_lossy();
        let initial = initialize_snapshot_from_set(&tied_test_set(), &path_text)
            .expect("initialize snapshot");
        assert_eq!(initial.alternative_index_decimal(), Some("1"));
        assert_eq!(initial.known_alternative_count_decimal(), "1");
        assert!(!initial.enumeration_complete());
        let first_cursor = initial.cursor().expect("restart cursor").to_owned();
        let (secret, _) = parse_cursor(&first_cursor).expect("initial cursor");

        assert_eq!(
            initialize_snapshot_from_set(&tied_test_set(), &path_text),
            Err(TieSnapshotError::TargetExists)
        );

        let mut indices = vec!["1".to_owned()];
        let mut cursor = Some(first_cursor.clone());
        while let Some(current) = cursor {
            let page = continue_snapshot(&path_text, &current).expect("resume next exact page");
            if let Some(index) = page.alternative_index_decimal() {
                indices.push(index.to_owned());
            }
            cursor = page.cursor().map(ToOwned::to_owned);
            if cursor.is_none() {
                assert!(page.enumeration_complete());
                assert_eq!(page.total_alternative_count_decimal(), Some("8"));
            }
        }
        assert_eq!(indices, ["1", "2", "3", "4", "5", "6", "7", "8"]);
        assert_eq!(
            continue_snapshot(&path_text, &first_cursor),
            Err(TieSnapshotError::StaleCursor)
        );

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open completed snapshot");
        let records = read_and_authenticate_records(&mut file, &secret)
            .expect("every append/checkpoint is authenticated");
        assert_eq!(records.len(), 17);
        drop(file);
        remove_test_directory(&directory);
    }

    #[test]
    fn snapshot_preserves_product_public_candidate_ids_across_restart() {
        let directory = test_directory("public-candidate-ids");
        let path = directory.join("portfolios.jsonl");
        let path_text = path.to_string_lossy();
        let mapped = tied_test_set()
            .with_public_candidate_ids(vec![101, 205, 309, 401, 505, 609])
            .expect("mapped public candidate identities");

        let initial =
            initialize_snapshot_from_set(&mapped, &path_text).expect("initialize mapped snapshot");
        assert_eq!(
            initial
                .members()
                .iter()
                .map(ExplicitPortfolioMember::candidate_id_decimal)
                .collect::<Vec<_>>(),
            vec!["101", "205", "309"]
        );

        let next = continue_snapshot(
            &path_text,
            initial.cursor().expect("mapped snapshot continuation"),
        )
        .expect("resume mapped snapshot");
        assert_eq!(
            next.members()
                .iter()
                .map(ExplicitPortfolioMember::candidate_id_decimal)
                .collect::<Vec<_>>(),
            vec!["101", "205", "609"]
        );

        remove_test_directory(&directory);
    }

    #[test]
    fn already_complete_initial_snapshot_does_not_mint_a_stale_continuation_cursor() {
        let directory = test_directory("complete-initial");
        let path = directory.join("portfolios.jsonl");
        let path_text = path.to_string_lossy();
        let initial = initialize_snapshot_from_set(&single_portfolio_test_set(), &path_text)
            .expect("initialize already-complete snapshot");

        assert_eq!(initial.alternative_index_decimal(), Some("1"));
        assert_eq!(initial.known_alternative_count_decimal(), "1");
        assert_eq!(initial.total_alternative_count_decimal(), Some("1"));
        assert!(initial.enumeration_complete());
        assert_eq!(initial.cursor(), None);

        remove_test_directory(&directory);
    }

    #[test]
    fn snapshot_and_cursor_tampering_fail_with_typed_rejections() {
        let directory = test_directory("tamper");
        let path = directory.join("portfolios.jsonl");
        let path_text = path.to_string_lossy();
        let initial = initialize_snapshot_from_set(&tied_test_set(), &path_text)
            .expect("initialize snapshot");
        let cursor = initial.cursor().expect("cursor").to_owned();
        let (secret, payload) = parse_cursor(&cursor).expect("valid cursor");

        let mut query_mismatch = payload.clone();
        query_mismatch.query_identity = "query-b".to_owned();
        assert_eq!(
            continue_snapshot(
                &path_text,
                &cursor_for(&secret, query_mismatch).expect("signed query mismatch")
            ),
            Err(TieSnapshotError::QueryMismatch)
        );

        let mut build_mismatch = payload.clone();
        build_mismatch.build_identity = "product-build.v1:wrong".to_owned();
        assert_eq!(
            continue_snapshot(
                &path_text,
                &cursor_for(&secret, build_mismatch).expect("signed build mismatch")
            ),
            Err(TieSnapshotError::BuildMismatch)
        );

        let mut candidate_mismatch = payload;
        candidate_mismatch.candidate_map_sha256 = "0".repeat(64);
        assert_eq!(
            continue_snapshot(
                &path_text,
                &cursor_for(&secret, candidate_mismatch).expect("signed candidate mismatch")
            ),
            Err(TieSnapshotError::CandidateMapMismatch)
        );

        let mut bytes = fs::read(&path).expect("read snapshot for controlled tamper");
        let marker = b"query-a";
        let position = bytes
            .windows(marker.len())
            .position(|window| window == marker)
            .expect("query identity in header");
        bytes[position + marker.len() - 1] = b'b';
        fs::write(&path, bytes).expect("tamper isolated test snapshot");
        assert_eq!(
            continue_snapshot(&path_text, &cursor),
            Err(TieSnapshotError::Tampered)
        );

        remove_test_directory(&directory);
    }

    #[test]
    fn snapshot_requires_an_exclusive_writer_lock() {
        let directory = test_directory("lock");
        let path = directory.join("portfolios.jsonl");
        let path_text = path.to_string_lossy();
        let initial = initialize_snapshot_from_set(&tied_test_set(), &path_text)
            .expect("initialize snapshot");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open snapshot");
        file.lock().expect("hold exclusive test lock");
        assert_eq!(
            continue_snapshot(&path_text, initial.cursor().expect("cursor")),
            Err(TieSnapshotError::Locked)
        );
        drop(file);
        remove_test_directory(&directory);
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_rejects_a_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let directory = test_directory("symlink");
        let real_parent = directory.join("real-parent");
        let linked_parent = directory.join("linked-parent");
        fs::create_dir(&real_parent).expect("real parent");
        symlink(&real_parent, &linked_parent).expect("test symlink");
        let requested = linked_parent.join("portfolios.jsonl");
        assert_eq!(
            initialize_snapshot_from_set(&tied_test_set(), &requested.to_string_lossy()),
            Err(TieSnapshotError::UnsafePath)
        );
        fs::remove_file(&linked_parent).expect("remove test symlink");
        fs::remove_dir(&real_parent).expect("remove real parent");
        remove_test_directory(&directory);
    }
}
