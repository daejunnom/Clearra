// SRP rationale: this module has one behavior-level change reason: validating and atomically encoding typed solution artifacts and explicit portfolio pages.

use std::{
    fmt,
    mem::size_of,
    path::{Path, PathBuf},
};

use clearra_app::{
    AppResponse, AppStatus, PcTilingFamilyV1Result, ProductCapabilityContract,
    ProductCapabilityResultKind,
};
use clearra_core_domain::solution::{
    NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
};
use clearra_host_contract::ProductResultPayloadContent;
use clearra_output::artifact::{
    ArtifactCommit, Ctk3SolutionSetEncoder, FumenSolutionSetEncoder, SolutionArtifactAnnotation,
    SolutionArtifactEncoder, SolutionArtifactEncoding, SolutionArtifactEncodingError,
    SolutionArtifactEntry, SolutionSetArtifact, DEFAULT_MAX_ARTIFACT_BYTES,
};
use sha2::{Digest, Sha256};

use crate::tie_snapshot::ExplicitPortfolioOutput;

use super::RenderFormat;

const RESERVED_TEXT_PREFIX: &str = "solution_artifact_";
const NORMALIZED_TILING_SOURCE_CONTRACT: &str = "normalized-tiling-set";
const PORTFOLIO_PAGE_SOURCE_CONTRACT: &str = "portfolio-alternative-page.v1";
const PORTFOLIO_MEMBER_KEY_ALGORITHM: &str = "portfolio-member-normalized-tiling-key.v1";
const PORTFOLIO_COLORED_FIELD_KEY_ALGORITHM: &str = "clearra-colored-field-key-v1";
const PORTFOLIO_PAGE_IDENTITY_ALGORITHM: &str = "portfolio-page-identity-sha256.v1";
// This is an independent bound on the one owned artifact model built from the
// borrowed execution result. The 512 MiB encoded-output ceiling does not grant
// authority to clone another output-sized key/metric collection in the CLI.
const MAX_MATERIALIZED_ARTIFACT_MODEL_BYTES: u64 = 256 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionArtifactOutputFormat {
    Compact,
    Json,
    Ctk3,
    Fumen,
}

impl SolutionArtifactOutputFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "compact" | "compact-v1" => Some(Self::Compact),
            "json" | "json-v1" => Some(Self::Json),
            "ctk3" => Some(Self::Ctk3),
            "fumen" | "v115" => Some(Self::Fumen),
            _ => None,
        }
    }

    pub const fn encoding(self) -> SolutionArtifactEncoding {
        match self {
            Self::Compact => SolutionArtifactEncoding::CompactV1,
            Self::Json => SolutionArtifactEncoding::JsonV1,
            Self::Ctk3 => SolutionArtifactEncoding::Ctk3,
            Self::Fumen => SolutionArtifactEncoding::Fumen,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolutionArtifactOutputRequest {
    target: PathBuf,
    format: SolutionArtifactOutputFormat,
}

impl SolutionArtifactOutputRequest {
    pub fn new(target: impl Into<PathBuf>, format: SolutionArtifactOutputFormat) -> Self {
        Self {
            target: target.into(),
            format,
        }
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub const fn format(&self) -> SolutionArtifactOutputFormat {
        self.format
    }

    pub(crate) fn prepare(
        &self,
        response: &AppResponse,
    ) -> Result<PreparedSolutionArtifact, SolutionArtifactOutputError> {
        let artifact = materialize_response(response)?;
        Ok(PreparedSolutionArtifact {
            target: self.target.clone(),
            format: self.format,
            artifact,
            maximum_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
        })
    }

    pub(crate) fn prepare_explicit_portfolio(
        &self,
        portfolio: &ExplicitPortfolioOutput,
    ) -> Result<PreparedSolutionArtifact, SolutionArtifactOutputError> {
        let artifact = materialize_explicit_portfolio(portfolio)?;
        Ok(PreparedSolutionArtifact {
            target: self.target.clone(),
            format: self.format,
            artifact,
            maximum_bytes: DEFAULT_MAX_ARTIFACT_BYTES,
        })
    }
}

pub(crate) fn encode_response_document(
    response: &AppResponse,
    format: SolutionArtifactOutputFormat,
) -> Result<String, SolutionArtifactOutputError> {
    let artifact = materialize_response(response)?;
    encode_artifact_document(&artifact, format)
}

pub(crate) fn encode_explicit_portfolio_document(
    portfolio: &ExplicitPortfolioOutput,
    format: SolutionArtifactOutputFormat,
) -> Result<String, SolutionArtifactOutputError> {
    let artifact = materialize_explicit_portfolio(portfolio)?;
    encode_artifact_document(&artifact, format)
}

fn encode_artifact_document(
    artifact: &SolutionSetArtifact,
    format: SolutionArtifactOutputFormat,
) -> Result<String, SolutionArtifactOutputError> {
    let encoded = match format {
        SolutionArtifactOutputFormat::Ctk3 => Ctk3SolutionSetEncoder.encode_checked(
            artifact,
            DEFAULT_MAX_ARTIFACT_BYTES,
            &clearra_output::artifact::NeverCancelled,
        ),
        SolutionArtifactOutputFormat::Fumen => FumenSolutionSetEncoder.encode_checked(
            artifact,
            DEFAULT_MAX_ARTIFACT_BYTES,
            &clearra_output::artifact::NeverCancelled,
        ),
        SolutionArtifactOutputFormat::Compact | SolutionArtifactOutputFormat::Json => {
            return Err(SolutionArtifactOutputError::StdoutFormatUnsupported)
        }
    }
    .map_err(map_document_encoding_error)?;
    String::from_utf8(encoded.bytes().to_vec())
        .map_err(|_| SolutionArtifactOutputError::DocumentEncodingFailed)
}

fn map_document_encoding_error(
    error: SolutionArtifactEncodingError,
) -> SolutionArtifactOutputError {
    match error {
        SolutionArtifactEncodingError::Artifact(_) => {
            SolutionArtifactOutputError::ArtifactModelInvalid
        }
        SolutionArtifactEncodingError::CapacityExceeded => {
            SolutionArtifactOutputError::DocumentCapacityExceeded
        }
        SolutionArtifactEncodingError::Cancelled => {
            SolutionArtifactOutputError::DocumentEncodingCancelled
        }
        SolutionArtifactEncodingError::EmptyDocument => SolutionArtifactOutputError::DocumentEmpty,
        SolutionArtifactEncodingError::InvalidDocumentSolutionKey => {
            SolutionArtifactOutputError::DocumentSolutionKeyUnsupported
        }
        SolutionArtifactEncodingError::Ctk3EncodingFailed => {
            SolutionArtifactOutputError::Ctk3EncodingFailed
        }
        SolutionArtifactEncodingError::Ctk3PageLimitExceeded => {
            SolutionArtifactOutputError::Ctk3PageLimitExceeded
        }
        SolutionArtifactEncodingError::FumenEncodingFailed => {
            SolutionArtifactOutputError::FumenEncodingFailed
        }
        SolutionArtifactEncodingError::FumenPageLimitExceeded => {
            SolutionArtifactOutputError::FumenPageLimitExceeded
        }
        SolutionArtifactEncodingError::WriteFailed
        | SolutionArtifactEncodingError::PlanMismatch
        | SolutionArtifactEncodingError::StreamVerificationFailed
        | SolutionArtifactEncodingError::InvalidCompactEnvelope
        | SolutionArtifactEncodingError::UnsupportedCompactContract
        | SolutionArtifactEncodingError::ChecksumMismatch
        | SolutionArtifactEncodingError::UnexpectedEnd
        | SolutionArtifactEncodingError::InvalidPrefixCompression
        | SolutionArtifactEncodingError::NonCanonicalOrder
        | SolutionArtifactEncodingError::InvalidUtf8
        | SolutionArtifactEncodingError::InvalidAnnotation
        | SolutionArtifactEncodingError::TrailingBytes => {
            SolutionArtifactOutputError::DocumentEncodingFailed
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreparedSolutionArtifact {
    target: PathBuf,
    format: SolutionArtifactOutputFormat,
    artifact: SolutionSetArtifact,
    maximum_bytes: u64,
}

impl PreparedSolutionArtifact {
    pub(crate) fn into_pending(
        self,
        stdout: impl Into<String>,
        render_format: RenderFormat,
    ) -> Result<PendingSolutionArtifact, SolutionArtifactOutputError> {
        PendingSolutionArtifact::try_new(
            self.target,
            self.format,
            self.artifact,
            self.maximum_bytes,
            stdout.into(),
            render_format,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingSolutionArtifact {
    target: PathBuf,
    format: SolutionArtifactOutputFormat,
    artifact: SolutionSetArtifact,
    maximum_bytes: u64,
    base_stdout: String,
    render_format: RenderFormat,
}

impl PendingSolutionArtifact {
    pub(crate) fn try_new(
        target: PathBuf,
        format: SolutionArtifactOutputFormat,
        artifact: SolutionSetArtifact,
        maximum_bytes: u64,
        base_stdout: String,
        render_format: RenderFormat,
    ) -> Result<Self, SolutionArtifactOutputError> {
        validate_stdout_contract(&base_stdout, render_format)?;
        Ok(Self {
            target,
            format,
            artifact,
            maximum_bytes,
            base_stdout,
            render_format,
        })
    }

    pub(crate) fn target(&self) -> &Path {
        &self.target
    }

    pub(crate) const fn format(&self) -> SolutionArtifactOutputFormat {
        self.format
    }

    pub(crate) fn artifact(&self) -> &SolutionSetArtifact {
        &self.artifact
    }

    pub(crate) const fn maximum_bytes(&self) -> u64 {
        self.maximum_bytes
    }

    pub(crate) fn committed_stdout(
        &self,
        commit: &ArtifactCommit,
    ) -> Result<String, SolutionArtifactOutputError> {
        if commit.schema() != self.format.encoding().schema()
            || commit.encoding() != self.format.encoding()
            || commit.solution_count() != self.artifact.solution_count()
            || commit.annotated_solution_count() != self.artifact.annotated_solution_count()
            || !commit.target_owned()
            || commit.file_identity().is_none()
            || (commit.encoding().compression() == "none"
                && commit.uncompressed_bytes() != commit.byte_count())
        {
            return Err(SolutionArtifactOutputError::CommitMetadataMismatch);
        }

        match self.render_format {
            RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
                committed_text(&self.base_stdout, commit)
            }
            RenderFormat::Json => committed_json(&self.base_stdout, commit),
            RenderFormat::FumenLike => Err(SolutionArtifactOutputError::StdoutFormatUnsupported),
        }
    }
}

fn validate_stdout_contract(
    stdout: &str,
    render_format: RenderFormat,
) -> Result<(), SolutionArtifactOutputError> {
    match render_format {
        RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
            if stdout
                .lines()
                .any(|line| line.trim_start().starts_with(RESERVED_TEXT_PREFIX))
            {
                return Err(SolutionArtifactOutputError::StdoutMetadataCollision);
            }
            Ok(())
        }
        RenderFormat::Json => {
            let value: serde_json::Value = serde_json::from_str(stdout)
                .map_err(|_| SolutionArtifactOutputError::StdoutJsonInvalid)?;
            let object = value
                .as_object()
                .ok_or(SolutionArtifactOutputError::StdoutJsonInvalid)?;
            if object.contains_key("solution_artifact") {
                return Err(SolutionArtifactOutputError::StdoutMetadataCollision);
            }
            Ok(())
        }
        RenderFormat::FumenLike => Err(SolutionArtifactOutputError::StdoutFormatUnsupported),
    }
}

fn committed_text(
    base: &str,
    commit: &ArtifactCommit,
) -> Result<String, SolutionArtifactOutputError> {
    let mut output = String::with_capacity(base.len().saturating_add(384));
    if !base.is_empty() {
        output.push_str(base);
        output.push('\n');
    }
    output.push_str("solution_artifact_status: committed\n");
    output.push_str("solution_artifact_schema: ");
    output.push_str(commit.schema());
    output.push_str("\nsolution_artifact_encoding: ");
    output.push_str(commit.encoding().keyword());
    output.push_str("\nsolution_artifact_compression: ");
    output.push_str(commit.encoding().compression());
    output.push_str("\nsolution_artifact_bytes: ");
    output.push_str(&commit.byte_count().to_string());
    output.push_str("\nsolution_artifact_checksum: ");
    output.push_str(commit.checksum());
    output.push_str("\nsolution_artifact_uncompressed_bytes: ");
    output.push_str(&commit.uncompressed_bytes().to_string());
    output.push_str("\nsolution_artifact_solution_count: ");
    output.push_str(&commit.solution_count().to_string());
    output.push_str("\nsolution_artifact_annotated_solution_count: ");
    output.push_str(&commit.annotated_solution_count().to_string());
    output.push_str("\nsolution_artifact_target_owned: true");
    let identity = commit
        .file_identity()
        .ok_or(SolutionArtifactOutputError::CommitMetadataMismatch)?;
    output.push_str("\nsolution_artifact_file_identity_kind: ");
    output.push_str(identity.platform());
    output.push_str("\nsolution_artifact_file_identity: ");
    output.push_str(&identity.stable_value());
    Ok(output)
}

fn committed_json(
    base: &str,
    commit: &ArtifactCommit,
) -> Result<String, SolutionArtifactOutputError> {
    let mut value: serde_json::Value =
        serde_json::from_str(base).map_err(|_| SolutionArtifactOutputError::StdoutJsonInvalid)?;
    let object = value
        .as_object_mut()
        .ok_or(SolutionArtifactOutputError::StdoutJsonInvalid)?;
    if object.contains_key("solution_artifact") {
        return Err(SolutionArtifactOutputError::StdoutMetadataCollision);
    }

    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "status".to_owned(),
        serde_json::Value::String("committed".to_owned()),
    );
    metadata.insert(
        "schema".to_owned(),
        serde_json::Value::String(commit.schema().to_owned()),
    );
    metadata.insert(
        "encoding".to_owned(),
        serde_json::Value::String(commit.encoding().keyword().to_owned()),
    );
    metadata.insert(
        "compression".to_owned(),
        serde_json::Value::String(commit.encoding().compression().to_owned()),
    );
    metadata.insert(
        "bytes".to_owned(),
        serde_json::Value::from(commit.byte_count()),
    );
    metadata.insert(
        "checksum".to_owned(),
        serde_json::Value::String(commit.checksum().to_owned()),
    );
    metadata.insert(
        "uncompressed_bytes".to_owned(),
        serde_json::Value::from(commit.uncompressed_bytes()),
    );
    metadata.insert(
        "solution_count".to_owned(),
        serde_json::Value::from(commit.solution_count()),
    );
    metadata.insert(
        "annotated_solution_count".to_owned(),
        serde_json::Value::from(commit.annotated_solution_count()),
    );
    metadata.insert("target_owned".to_owned(), serde_json::Value::Bool(true));
    let identity = commit
        .file_identity()
        .ok_or(SolutionArtifactOutputError::CommitMetadataMismatch)?;
    metadata.insert(
        "file_identity_kind".to_owned(),
        serde_json::Value::String(identity.platform().to_owned()),
    );
    metadata.insert(
        "file_identity".to_owned(),
        serde_json::Value::String(identity.stable_value()),
    );
    object.insert(
        "solution_artifact".to_owned(),
        serde_json::Value::Object(metadata),
    );
    serde_json::to_string(&value).map_err(|_| SolutionArtifactOutputError::StdoutJsonInvalid)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArtifactMetricView<'a> {
    key: &'a str,
    value: &'a str,
    complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArtifactSourceView<'a> {
    source_contract: &'a str,
    key_algorithm: &'a str,
    set_hash_algorithm: &'a str,
    set_hash: &'a str,
    expected_solution_count: usize,
    keys: &'a [String],
}

fn materialize_response(
    response: &AppResponse,
) -> Result<SolutionSetArtifact, SolutionArtifactOutputError> {
    let resource = response.resource_report();
    let execution = resource.execution_availability();
    validate_execution_gate(
        response.status() == AppStatus::Success,
        resource.solver_executed(),
        execution.state().as_str(),
        execution.reason().is_none(),
        resource.result_completeness().as_str(),
        resource.truncated(),
    )?;
    let Some(result) = response
        .render_model()
        .and_then(|model| model.core_result())
    else {
        let direct_build_product = matches!(
            response
                .product_capability_result()
                .map(|product| product.contract()),
            Some(ProductCapabilityContract::BuildCover | ProductCapabilityContract::BuildSetup)
        ) || response.public_result_payload().is_some_and(|payload| {
            matches!(
                payload.content(),
                ProductResultPayloadContent::BuildV2(_)
                    | ProductResultPayloadContent::BuildCoveragePortfolioV2(_)
                    | ProductResultPayloadContent::BuildSetupFamilyV1(_)
            )
        });
        if !direct_build_product {
            return Err(SolutionArtifactOutputError::SolutionSetSurfaceUnavailable);
        }
        return response
            .complete_solution_set_artifact()
            .ok_or(SolutionArtifactOutputError::SolutionSetSurfaceUnavailable);
    };
    if let Some(product) = response.product_capability_result() {
        let typed_tiling_contract = product.contract() == ProductCapabilityContract::PcTiling;
        let typed_tiling_result =
            product.result_kind() == ProductCapabilityResultKind::PcTilingFamilyV1;
        if typed_tiling_contract || typed_tiling_result || product.pc_tiling_family_v1().is_some() {
            if !typed_tiling_contract || !typed_tiling_result {
                return Err(SolutionArtifactOutputError::SolutionSetContractInvalid);
            }
            let family = product
                .pc_tiling_family_v1()
                .ok_or(SolutionArtifactOutputError::SolutionSetContractInvalid)?;
            let availability = result.execution_report().solution_set_availability();
            if !availability.uses_explicit_contract() || !availability.contract_valid() {
                return Err(SolutionArtifactOutputError::SolutionSetContractInvalid);
            }
            if !availability.solution_count_calculated()
                || !availability.solution_set_materialized()
            {
                return Err(SolutionArtifactOutputError::SolutionSetNotMaterialized);
            }
            if !availability.materialized_key_count_matches(result.normalized_solution_keys().len())
            {
                return Err(SolutionArtifactOutputError::SolutionKeyCountMismatch);
            }
            let fields = result.summary_fields();
            return materialize_pc_tiling_family(
                result.normalized_solution_keys(),
                &fields,
                family,
            );
        }
    }
    let availability = result.execution_report().solution_set_availability();
    if !availability.uses_explicit_contract() || !availability.contract_valid() {
        return Err(SolutionArtifactOutputError::SolutionSetContractInvalid);
    }
    if !availability.solution_count_calculated() || !availability.solution_set_materialized() {
        return Err(SolutionArtifactOutputError::SolutionSetNotMaterialized);
    }
    if !availability.solution_keys_complete() {
        return Err(SolutionArtifactOutputError::SolutionKeysIncomplete);
    }
    if !availability.materialized_key_count_matches(result.normalized_solution_keys().len()) {
        return Err(SolutionArtifactOutputError::SolutionKeyCountMismatch);
    }
    let fields = result.summary_fields();
    if let Some(count_complete) = single_field(&fields, "count_complete")? {
        match count_complete {
            "true" => {}
            "false" => return Err(SolutionArtifactOutputError::ResultIncomplete),
            _ => return Err(SolutionArtifactOutputError::SolutionSetContractInvalid),
        }
    }

    let expected_solution_count = required_count(&fields, "normalized_unique_solution_count")?;
    if expected_solution_count != result.normalized_solution_keys().len() {
        return Err(SolutionArtifactOutputError::SolutionKeyCountMismatch);
    }
    let key_algorithm = required_metadata(&fields, "normalized_solution_key_algorithm")?;
    let set_hash_algorithm = required_metadata(&fields, "normalized_solution_set_hash_algorithm")?;
    let source_contract = source_contract(&fields, key_algorithm, set_hash_algorithm)?;
    let set_hash = required_metadata(&fields, "normalized_solution_set_hash")?;
    let actual_set_hash = required_metadata(&fields, "actual_normalized_solution_set_hash")?;
    if set_hash != actual_set_hash {
        return Err(SolutionArtifactOutputError::IdentityMetadataMismatch);
    }

    let view = ArtifactSourceView {
        source_contract,
        key_algorithm,
        set_hash_algorithm,
        set_hash,
        expected_solution_count,
        keys: result.normalized_solution_keys(),
    };
    materialize_artifact(
        view,
        || {
            result
                .solution_probabilities()
                .iter()
                .map(|entry| ArtifactMetricView {
                    key: entry.solution_key(),
                    value: entry.probability(),
                    complete: entry.probability_complete(),
                })
        },
        || {
            result
                .solution_average_scores()
                .iter()
                .map(|entry| ArtifactMetricView {
                    key: entry.solution_key(),
                    value: entry.average_score(),
                    complete: entry.score_complete(),
                })
        },
    )
}

fn materialize_explicit_portfolio(
    portfolio: &ExplicitPortfolioOutput,
) -> Result<SolutionSetArtifact, SolutionArtifactOutputError> {
    let alternative_index = portfolio
        .alternative_index_decimal()
        .filter(|value| is_canonical_nonzero_decimal(value))
        .ok_or(SolutionArtifactOutputError::SolutionSetContractInvalid)?;
    if portfolio.set_contract() != "portfolio-alternative-set.v1"
        || portfolio.page_contract() != PORTFOLIO_PAGE_SOURCE_CONTRACT
        || !is_sha256_hex(portfolio.set_identity_sha256())
        || !is_sha256_hex(portfolio.candidate_map_sha256())
        || !is_canonical_nonzero_decimal(portfolio.known_alternative_count_decimal())
        || compare_canonical_decimals(
            alternative_index,
            portfolio.known_alternative_count_decimal(),
        ) == std::cmp::Ordering::Greater
        || portfolio
            .total_alternative_count_decimal()
            .is_some_and(|total| {
                !is_canonical_nonzero_decimal(total)
                    || compare_canonical_decimals(
                        portfolio.known_alternative_count_decimal(),
                        total,
                    ) == std::cmp::Ordering::Greater
            })
        || if portfolio.enumeration_complete() {
            portfolio.total_alternative_count_decimal()
                != Some(portfolio.known_alternative_count_decimal())
                || portfolio.cursor().is_some()
        } else {
            portfolio.total_alternative_count_decimal().is_some() || portfolio.cursor().is_none()
        }
        || portfolio.members().len() != portfolio.optimal_cardinality()
    {
        return Err(SolutionArtifactOutputError::SolutionSetContractInvalid);
    }

    let mut previous_candidate_id = 0_u64;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(portfolio.members().len())
        .map_err(|_| SolutionArtifactOutputError::ArtifactModelCapacityExceeded)?;
    for member in portfolio.members() {
        let candidate_id_decimal = member.candidate_id_decimal();
        if !is_canonical_nonzero_decimal(candidate_id_decimal) {
            return Err(SolutionArtifactOutputError::SolutionSetContractInvalid);
        }
        let candidate_id = candidate_id_decimal
            .parse::<u64>()
            .ok()
            .filter(|candidate_id| *candidate_id > previous_candidate_id)
            .ok_or(SolutionArtifactOutputError::SolutionSetContractInvalid)?;
        previous_candidate_id = candidate_id;
        entries.push(
            SolutionArtifactEntry::try_new(
                member.normalized_key(),
                SolutionArtifactAnnotation::new(),
            )
            .map_err(|_| SolutionArtifactOutputError::ArtifactModelInvalid)?,
        );
    }
    let key_algorithm = if entries.iter().all(|entry| entry.key().starts_with("ctk1|")) {
        PORTFOLIO_MEMBER_KEY_ALGORITHM
    } else if entries.iter().all(|entry| entry.key().starts_with("cfk1|")) {
        PORTFOLIO_COLORED_FIELD_KEY_ALGORITHM
    } else {
        return Err(SolutionArtifactOutputError::SolutionSetContractInvalid);
    };

    SolutionSetArtifact::try_new(
        PORTFOLIO_PAGE_SOURCE_CONTRACT,
        key_algorithm,
        PORTFOLIO_PAGE_IDENTITY_ALGORITHM,
        portfolio_page_identity_sha256(portfolio, alternative_index),
        portfolio.optimal_cardinality(),
        entries,
    )
    .map_err(|_| SolutionArtifactOutputError::ArtifactModelInvalid)
}

fn portfolio_page_identity_sha256(
    portfolio: &ExplicitPortfolioOutput,
    alternative_index: &str,
) -> String {
    let mut hasher = Sha256::new();
    hash_portfolio_identity_component(
        &mut hasher,
        b"contract",
        PORTFOLIO_PAGE_IDENTITY_ALGORITHM.as_bytes(),
    );
    hash_portfolio_identity_component(
        &mut hasher,
        b"set",
        portfolio.set_identity_sha256().as_bytes(),
    );
    hash_portfolio_identity_component(
        &mut hasher,
        b"candidate-map",
        portfolio.candidate_map_sha256().as_bytes(),
    );
    hash_portfolio_identity_component(
        &mut hasher,
        b"alternative-index",
        alternative_index.as_bytes(),
    );
    for member in portfolio.members() {
        hash_portfolio_identity_component(
            &mut hasher,
            b"candidate-id",
            member.candidate_id_decimal().as_bytes(),
        );
    }
    let digest = hasher.finalize();
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

fn hash_portfolio_identity_component(hasher: &mut Sha256, label: &[u8], value: &[u8]) {
    hasher.update((label.len() as u64).to_be_bytes());
    hasher.update(label);
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_canonical_nonzero_decimal(value: &str) -> bool {
    !value.is_empty()
        && value != "0"
        && !value.starts_with('0')
        && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn compare_canonical_decimals(left: &str, right: &str) -> std::cmp::Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn materialize_pc_tiling_family(
    initial_page_keys: &[String],
    fields: &[(String, String)],
    family: &PcTilingFamilyV1Result,
) -> Result<SolutionSetArtifact, SolutionArtifactOutputError> {
    let count = family.normalized_solution_count();
    let initial_count = count.min(family.initial_page_limit());
    if !family.completeness().family_complete()
        || !family.completeness().initial_page_complete()
        || family.completeness().incomplete_reason() != "none"
        || family.initial_page_keys().len() != initial_count
        || family.initial_page_keys() != initial_page_keys
        || family.completeness().initial_page_covers_family() != (initial_count == count)
    {
        return Err(SolutionArtifactOutputError::ResultIncomplete);
    }

    if required_count(fields, "normalized_unique_solution_count")? != count
        || required_metadata(fields, "normalized_solution_key_algorithm")?
            != family.normalized_solution_key_algorithm()
        || required_metadata(fields, "normalized_solution_set_hash_algorithm")?
            != family.normalized_solution_set_hash_algorithm()
        || required_metadata(fields, "normalized_solution_set_hash")?
            != family.normalized_solution_set_hash()
        || required_metadata(fields, "actual_normalized_solution_set_hash")?
            != family.normalized_solution_set_hash()
        || source_contract(
            fields,
            family.normalized_solution_key_algorithm(),
            family.normalized_solution_set_hash_algorithm(),
        )? != NORMALIZED_TILING_SOURCE_CONTRACT
    {
        return Err(SolutionArtifactOutputError::IdentityMetadataMismatch);
    }

    materialize_paged_pc_tiling_family_with_limit(
        count,
        family.normalized_solution_set_hash(),
        family.initial_page_limit(),
        MAX_MATERIALIZED_ARTIFACT_MODEL_BYTES,
        |offset, limit| family.page_keys(offset, limit),
    )
}

fn materialize_paged_pc_tiling_family_with_limit<Page>(
    expected_solution_count: usize,
    set_hash: &str,
    page_limit: usize,
    maximum_model_bytes: u64,
    mut page_keys: Page,
) -> Result<SolutionSetArtifact, SolutionArtifactOutputError>
where
    Page: FnMut(usize, usize) -> Result<Vec<String>, &'static str>,
{
    if page_limit == 0 {
        return Err(SolutionArtifactOutputError::SolutionSetContractInvalid);
    }
    let entry_storage = expected_solution_count
        .checked_mul(size_of::<SolutionArtifactEntry>())
        .ok_or(SolutionArtifactOutputError::ArtifactModelCapacityExceeded)?;
    let mut model_bytes = u64::try_from(size_of::<SolutionSetArtifact>())
        .map_err(|_| SolutionArtifactOutputError::ArtifactModelCapacityExceeded)?;
    model_bytes = checked_model_add(model_bytes, entry_storage)?;
    for value in [
        NORMALIZED_TILING_SOURCE_CONTRACT,
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
        NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
        set_hash,
    ] {
        model_bytes = checked_model_add(model_bytes, value.len())?;
    }
    if model_bytes > maximum_model_bytes {
        return Err(SolutionArtifactOutputError::ArtifactModelCapacityExceeded);
    }

    let mut entries = Vec::new();
    entries
        .try_reserve_exact(expected_solution_count)
        .map_err(|_| SolutionArtifactOutputError::ArtifactModelCapacityExceeded)?;
    while entries.len() < expected_solution_count {
        let offset = entries.len();
        let expected_page_count = (expected_solution_count - offset).min(page_limit);
        let page = page_keys(offset, expected_page_count)
            .map_err(|_| SolutionArtifactOutputError::TilingFamilyPageUnavailable)?;
        if page.len() != expected_page_count {
            return Err(SolutionArtifactOutputError::SolutionKeyCountMismatch);
        }
        for key in page {
            if entries
                .last()
                .is_some_and(|entry: &SolutionArtifactEntry| entry.key() >= key.as_str())
            {
                return Err(SolutionArtifactOutputError::SolutionKeyOrderInvalid);
            }
            model_bytes = checked_model_add(model_bytes, key.len())?;
            if model_bytes > maximum_model_bytes {
                return Err(SolutionArtifactOutputError::ArtifactModelCapacityExceeded);
            }
            entries.push(
                SolutionArtifactEntry::try_new(key, SolutionArtifactAnnotation::new())
                    .map_err(|_| SolutionArtifactOutputError::ArtifactModelInvalid)?,
            );
        }
    }

    SolutionSetArtifact::try_new(
        NORMALIZED_TILING_SOURCE_CONTRACT,
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
        NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
        set_hash,
        expected_solution_count,
        entries,
    )
    .map_err(|_| SolutionArtifactOutputError::ArtifactModelInvalid)
}

fn materialize_artifact<'a, ProbabilityRows, ScoreRows, ProbabilityIter, ScoreIter>(
    source: ArtifactSourceView<'a>,
    probability_rows: ProbabilityRows,
    score_rows: ScoreRows,
) -> Result<SolutionSetArtifact, SolutionArtifactOutputError>
where
    ProbabilityRows: Fn() -> ProbabilityIter,
    ScoreRows: Fn() -> ScoreIter,
    ProbabilityIter: Iterator<Item = ArtifactMetricView<'a>>,
    ScoreIter: Iterator<Item = ArtifactMetricView<'a>>,
{
    materialize_artifact_with_limit(
        source,
        probability_rows,
        score_rows,
        MAX_MATERIALIZED_ARTIFACT_MODEL_BYTES,
    )
}

fn materialize_artifact_with_limit<'a, ProbabilityRows, ScoreRows, ProbabilityIter, ScoreIter>(
    source: ArtifactSourceView<'a>,
    probability_rows: ProbabilityRows,
    score_rows: ScoreRows,
    maximum_model_bytes: u64,
) -> Result<SolutionSetArtifact, SolutionArtifactOutputError>
where
    ProbabilityRows: Fn() -> ProbabilityIter,
    ScoreRows: Fn() -> ScoreIter,
    ProbabilityIter: Iterator<Item = ArtifactMetricView<'a>>,
    ScoreIter: Iterator<Item = ArtifactMetricView<'a>>,
{
    if source.expected_solution_count != source.keys.len() {
        return Err(SolutionArtifactOutputError::SolutionKeyCountMismatch);
    }
    validate_solution_key_order(source.keys)?;
    validate_metric_sequence(source.keys, probability_rows())?;
    validate_metric_sequence(source.keys, score_rows())?;
    let model_bytes = planned_materialized_model_bytes(source, probability_rows(), score_rows())?;
    if model_bytes > maximum_model_bytes {
        return Err(SolutionArtifactOutputError::ArtifactModelCapacityExceeded);
    }

    let mut probabilities = probability_rows().peekable();
    let mut scores = score_rows().peekable();
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(source.keys.len())
        .map_err(|_| SolutionArtifactOutputError::ArtifactModelCapacityExceeded)?;
    for key in source.keys {
        let mut annotation = SolutionArtifactAnnotation::new();
        if let Some(row) = take_metric_for_key(key, &mut probabilities) {
            if row.complete {
                annotation = annotation
                    .with_pc_probability(row.value)
                    .map_err(|_| SolutionArtifactOutputError::AnnotationInvalid)?;
            }
        }
        if let Some(row) = take_metric_for_key(key, &mut scores) {
            if row.complete {
                annotation = annotation
                    .with_average_score(row.value)
                    .map_err(|_| SolutionArtifactOutputError::AnnotationInvalid)?;
            }
        }
        entries.push(
            SolutionArtifactEntry::try_new(key.as_str(), annotation)
                .map_err(|_| SolutionArtifactOutputError::ArtifactModelInvalid)?,
        );
    }
    #[cfg(debug_assertions)]
    {
        // Keep the iterator mutation outside `debug_assert!`: macro arguments
        // are not evaluated in release builds, so state transitions must never
        // be hidden inside the assertion itself.
        let probabilities_exhausted = probabilities.next().is_none();
        let scores_exhausted = scores.next().is_none();
        debug_assert!(probabilities_exhausted);
        debug_assert!(scores_exhausted);
    }

    SolutionSetArtifact::try_new(
        source.source_contract,
        source.key_algorithm,
        source.set_hash_algorithm,
        source.set_hash,
        source.expected_solution_count,
        entries,
    )
    .map_err(|_| SolutionArtifactOutputError::ArtifactModelInvalid)
}

fn validate_solution_key_order(keys: &[String]) -> Result<(), SolutionArtifactOutputError> {
    if keys
        .windows(2)
        .any(|pair| pair[0].as_str() >= pair[1].as_str())
    {
        Err(SolutionArtifactOutputError::SolutionKeyOrderInvalid)
    } else {
        Ok(())
    }
}

fn validate_metric_sequence<'a>(
    keys: &[String],
    rows: impl Iterator<Item = ArtifactMetricView<'a>>,
) -> Result<(), SolutionArtifactOutputError> {
    let mut key_index = 0_usize;
    let mut previous_key = None;
    for row in rows {
        if let Some(previous) = previous_key {
            if previous == row.key {
                return Err(SolutionArtifactOutputError::DuplicateAnnotationKey);
            }
            if previous > row.key {
                return Err(SolutionArtifactOutputError::AnnotationOrderInvalid);
            }
        }
        previous_key = Some(row.key);
        while key_index < keys.len() && keys[key_index].as_str() < row.key {
            key_index += 1;
        }
        if keys.get(key_index).map(String::as_str) != Some(row.key) {
            return Err(SolutionArtifactOutputError::UnknownAnnotationKey);
        }
    }
    Ok(())
}

fn planned_materialized_model_bytes<'a>(
    source: ArtifactSourceView<'_>,
    probability_rows: impl Iterator<Item = ArtifactMetricView<'a>>,
    score_rows: impl Iterator<Item = ArtifactMetricView<'a>>,
) -> Result<u64, SolutionArtifactOutputError> {
    let entry_storage = source
        .keys
        .len()
        .checked_mul(size_of::<SolutionArtifactEntry>())
        .ok_or(SolutionArtifactOutputError::ArtifactModelCapacityExceeded)?;
    let mut bytes = u64::try_from(size_of::<SolutionSetArtifact>())
        .map_err(|_| SolutionArtifactOutputError::ArtifactModelCapacityExceeded)?;
    bytes = checked_model_add(bytes, entry_storage)?;
    for value in [
        source.source_contract,
        source.key_algorithm,
        source.set_hash_algorithm,
        source.set_hash,
    ] {
        bytes = checked_model_add(bytes, value.len())?;
    }
    for key in source.keys {
        bytes = checked_model_add(bytes, key.len())?;
    }
    for row in probability_rows.chain(score_rows) {
        if row.complete {
            bytes = checked_model_add(bytes, row.value.len())?;
        }
    }
    Ok(bytes)
}

fn checked_model_add(bytes: u64, additional: usize) -> Result<u64, SolutionArtifactOutputError> {
    bytes
        .checked_add(
            u64::try_from(additional)
                .map_err(|_| SolutionArtifactOutputError::ArtifactModelCapacityExceeded)?,
        )
        .ok_or(SolutionArtifactOutputError::ArtifactModelCapacityExceeded)
}

fn take_metric_for_key<'a, I>(
    key: &str,
    rows: &mut std::iter::Peekable<I>,
) -> Option<ArtifactMetricView<'a>>
where
    I: Iterator<Item = ArtifactMetricView<'a>>,
{
    if rows.peek().is_some_and(|row| row.key == key) {
        rows.next()
    } else {
        None
    }
}

fn validate_execution_gate(
    response_successful: bool,
    solver_executed: bool,
    availability_state: &str,
    availability_reason_is_none: bool,
    result_completeness: &str,
    truncated: bool,
) -> Result<(), SolutionArtifactOutputError> {
    if !response_successful {
        return Err(SolutionArtifactOutputError::ResponseNotSuccessful);
    }
    if !solver_executed {
        return Err(SolutionArtifactOutputError::SolverNotExecuted);
    }
    if availability_state != "available" || !availability_reason_is_none {
        return Err(SolutionArtifactOutputError::ExecutionUnavailable);
    }
    if result_completeness != "complete" {
        return Err(SolutionArtifactOutputError::ResultIncomplete);
    }
    if truncated {
        return Err(SolutionArtifactOutputError::ResultTruncated);
    }
    Ok(())
}

fn required_count(
    fields: &[(String, String)],
    key: &str,
) -> Result<usize, SolutionArtifactOutputError> {
    single_field(fields, key)?
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(SolutionArtifactOutputError::IdentityMetadataMissing)
}

fn required_metadata<'a>(
    fields: &'a [(String, String)],
    key: &str,
) -> Result<&'a str, SolutionArtifactOutputError> {
    single_field(fields, key)?
        .filter(|value| !value.is_empty() && *value != "not-calculated")
        .ok_or(SolutionArtifactOutputError::IdentityMetadataMissing)
}

fn source_contract<'a>(
    fields: &'a [(String, String)],
    key_algorithm: &str,
    set_hash_algorithm: &str,
) -> Result<&'a str, SolutionArtifactOutputError> {
    let canonical_algorithms = key_algorithm == NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM
        && set_hash_algorithm == NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM;
    if !canonical_algorithms {
        return Err(SolutionArtifactOutputError::IdentityMetadataMismatch);
    }
    match single_field(fields, "actual_solution_set_contract")? {
        Some(value) if value.is_empty() || value == "not-calculated" => {
            Err(SolutionArtifactOutputError::IdentityMetadataMissing)
        }
        Some(value) => {
            if value != NORMALIZED_TILING_SOURCE_CONTRACT {
                return Err(SolutionArtifactOutputError::IdentityMetadataMismatch);
            }
            Ok(value)
        }
        None => Ok(NORMALIZED_TILING_SOURCE_CONTRACT),
    }
}

fn single_field<'a>(
    fields: &'a [(String, String)],
    key: &str,
) -> Result<Option<&'a str>, SolutionArtifactOutputError> {
    let mut values = fields
        .iter()
        .filter_map(|(field_key, value)| (field_key == key).then_some(value.as_str()));
    let value = values.next();
    if values.next().is_some() {
        Err(SolutionArtifactOutputError::SolutionSetContractInvalid)
    } else {
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SolutionArtifactOutputError {
    ResponseNotSuccessful,
    SolverNotExecuted,
    ExecutionUnavailable,
    ResultIncomplete,
    ResultTruncated,
    SolutionSetSurfaceUnavailable,
    SolutionSetContractInvalid,
    SolutionSetNotMaterialized,
    SolutionKeysIncomplete,
    SolutionKeyCountMismatch,
    SolutionKeyOrderInvalid,
    IdentityMetadataMissing,
    IdentityMetadataMismatch,
    UnknownAnnotationKey,
    DuplicateAnnotationKey,
    AnnotationOrderInvalid,
    AnnotationInvalid,
    ArtifactModelCapacityExceeded,
    ArtifactModelInvalid,
    DocumentEncodingFailed,
    DocumentCapacityExceeded,
    DocumentEncodingCancelled,
    DocumentEmpty,
    DocumentSolutionKeyUnsupported,
    Ctk3EncodingFailed,
    Ctk3PageLimitExceeded,
    FumenEncodingFailed,
    FumenPageLimitExceeded,
    TilingFamilyPageUnavailable,
    StdoutFormatUnsupported,
    StdoutJsonInvalid,
    StdoutMetadataCollision,
    CommitMetadataMismatch,
}

impl SolutionArtifactOutputError {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ResponseNotSuccessful => "artifact-response-not-successful",
            Self::SolverNotExecuted => "artifact-solver-not-executed",
            Self::ExecutionUnavailable => "artifact-execution-unavailable",
            Self::ResultIncomplete => "artifact-result-incomplete",
            Self::ResultTruncated => "artifact-result-truncated",
            Self::SolutionSetSurfaceUnavailable => "artifact-solution-set-surface-unavailable",
            Self::SolutionSetContractInvalid => "artifact-solution-set-contract-invalid",
            Self::SolutionSetNotMaterialized => "artifact-solution-set-not-materialized",
            Self::SolutionKeysIncomplete => "artifact-solution-keys-incomplete",
            Self::SolutionKeyCountMismatch => "artifact-solution-key-count-mismatch",
            Self::SolutionKeyOrderInvalid => "artifact-solution-key-order-invalid",
            Self::IdentityMetadataMissing => "artifact-identity-metadata-missing",
            Self::IdentityMetadataMismatch => "artifact-identity-metadata-mismatch",
            Self::UnknownAnnotationKey => "artifact-annotation-key-unknown",
            Self::DuplicateAnnotationKey => "artifact-annotation-key-duplicated",
            Self::AnnotationOrderInvalid => "artifact-annotation-order-invalid",
            Self::AnnotationInvalid => "artifact-annotation-invalid",
            Self::ArtifactModelCapacityExceeded => "artifact-model-capacity-exceeded",
            Self::ArtifactModelInvalid => "artifact-model-invalid",
            Self::DocumentEncodingFailed => "artifact-document-encoding-failed",
            Self::DocumentCapacityExceeded => "artifact-document-capacity-exceeded",
            Self::DocumentEncodingCancelled => "artifact-document-encoding-cancelled",
            Self::DocumentEmpty => "artifact-document-empty",
            Self::DocumentSolutionKeyUnsupported => "artifact-document-solution-key-unsupported",
            Self::Ctk3EncodingFailed => "artifact-ctk3-encoding-failed",
            Self::Ctk3PageLimitExceeded => "artifact-ctk3-page-limit-exceeded",
            Self::FumenEncodingFailed => "artifact-fumen-encoding-failed",
            Self::FumenPageLimitExceeded => "artifact-fumen-page-limit-exceeded",
            Self::TilingFamilyPageUnavailable => "artifact-tiling-family-page-unavailable",
            Self::StdoutFormatUnsupported => "artifact-stdout-format-unsupported",
            Self::StdoutJsonInvalid => "artifact-stdout-json-invalid",
            Self::StdoutMetadataCollision => "artifact-stdout-metadata-collision",
            Self::CommitMetadataMismatch => "artifact-commit-metadata-mismatch",
        }
    }
}

impl fmt::Display for SolutionArtifactOutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for SolutionArtifactOutputError {}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        sync::atomic::{AtomicU64, Ordering},
    };

    use clearra_app::{
        encode_ctk3_compact, AppContext, AppCoreExecutorService, AppServices, AppStatus, Ctk3Color,
        Ctk3Document, Ctk3Page, Ctk3Piece,
    };
    use clearra_core_domain::solution::{
        NormalizedTilingSolutionKey, NormalizedTilingSolutionSetHasher, PiecePlacementMask,
    };
    use clearra_output::artifact::{
        CompactSolutionSetEncoder, FileIdentity, JsonSolutionSetEncoder, NeverCancelled,
        SolutionArtifactEncoder,
    };

    use super::*;
    use crate::{
        args::CliParser,
        assemble::CliAppRequestAssembler,
        tie_snapshot::{initialize_snapshot, ExplicitPortfolioMember},
    };

    static NEXT_BUILD_ARTIFACT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct TestMetricRow {
        key: String,
        value: String,
        complete: bool,
    }

    impl TestMetricRow {
        fn view(&self) -> ArtifactMetricView<'_> {
            ArtifactMetricView {
                key: &self.key,
                value: &self.value,
                complete: self.complete,
            }
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct TestSource {
        source_contract: String,
        key_algorithm: String,
        set_hash_algorithm: String,
        set_hash: String,
        expected_solution_count: usize,
        keys: Vec<String>,
        probabilities: Vec<TestMetricRow>,
        scores: Vec<TestMetricRow>,
    }

    impl TestSource {
        fn view(&self) -> ArtifactSourceView<'_> {
            ArtifactSourceView {
                source_contract: &self.source_contract,
                key_algorithm: &self.key_algorithm,
                set_hash_algorithm: &self.set_hash_algorithm,
                set_hash: &self.set_hash,
                expected_solution_count: self.expected_solution_count,
                keys: &self.keys,
            }
        }

        fn artifact(&self) -> Result<SolutionSetArtifact, SolutionArtifactOutputError> {
            materialize_artifact(
                self.view(),
                || self.probabilities.iter().map(TestMetricRow::view),
                || self.scores.iter().map(TestMetricRow::view),
            )
        }
    }

    fn source() -> TestSource {
        TestSource {
            source_contract: "test-solution-set".to_owned(),
            key_algorithm: "key-v1".to_owned(),
            set_hash_algorithm: "hash-v1".to_owned(),
            set_hash: "hash:1".to_owned(),
            expected_solution_count: 2,
            keys: vec!["solution-a".to_owned(), "solution-b".to_owned()],
            probabilities: vec![TestMetricRow {
                key: "solution-a".to_owned(),
                value: "0.5".to_owned(),
                complete: true,
            }],
            scores: vec![TestMetricRow {
                key: "solution-b".to_owned(),
                value: "1200".to_owned(),
                complete: false,
            }],
        }
    }

    fn explicit_portfolio(
        alternative_index: &str,
        members: Vec<ExplicitPortfolioMember>,
    ) -> ExplicitPortfolioOutput {
        ExplicitPortfolioOutput::for_test(
            &"a".repeat(64),
            &"b".repeat(64),
            Some(alternative_index),
            members.len(),
            members,
            "2",
            Some("2"),
            true,
            None,
        )
    }

    fn explicit_portfolio_members() -> Vec<ExplicitPortfolioMember> {
        (0..2_u64)
            .map(|initial| {
                let key = NormalizedTilingSolutionKey::from_placements(
                    initial,
                    Vec::<PiecePlacementMask>::new(),
                )
                .expect("canonical portfolio member");
                ExplicitPortfolioMember::for_test(&(initial + 1).to_string(), key.as_str())
            })
            .collect()
    }

    fn build_v2_artifact_document() -> String {
        let mut cells = vec![Ctk3Color::Empty; 40];
        cells[0..4].fill(Ctk3Color::Piece(Ctk3Piece::I));
        encode_ctk3_compact(&Ctk3Document::new(10, vec![Ctk3Page::new(4, cells)]))
            .expect("one-piece Build v2 artifact document")
    }

    fn build_v2_artifact_cases() -> Vec<(String, bool)> {
        let document = build_v2_artifact_document();
        let target = |path: &str, suffix: &str| {
            format!(
                "clearra build {path} --target-format ctk3 --target-document {document} --queue I --no-hold {suffix} --workers 2"
            )
        };
        let supplied = |path: &str, suffix: &str| {
            format!(
                "clearra build evaluate {path} --solution-format ctk3 --solution-document {document} --queue I --no-hold {suffix} --workers 2"
            )
        };
        vec![
            (
                "clearra build cover --base-mask 0 --target-mask 15 --height 4 --queue I --no-hold --objective min-cover --workers 2".to_owned(),
                true,
            ),
            (target("setup", "--objective unique"), true),
            (target("congruent", "--objective all"), false),
            (
                target("congruent-cover", "--objective min-cover"),
                true,
            ),
            (target("setup-cover", "--objective min-cover"), true),
            (
                target("setup-cover-percent", "--objective unique"),
                false,
            ),
            (
                target(
                    "setup-cover-score",
                    "--objective max-score-cover --score-profile guideline --initial-b2b 9",
                ),
                true,
            ),
            (supplied("cover", "--objective all"), false),
            (supplied("minimals", "--objective min-cover"), true),
            (
                supplied(
                    "score",
                    "--objective max-score-cover --score-profile tetrio --initial-b2b 0",
                ),
                true,
            ),
            (supplied("b2b-cover", "--objective all"), false),
            (supplied("cover-percent", "--objective unique"), false),
        ]
    }

    #[test]
    fn direct_build_products_materialize_only_authorized_solution_artifacts_in_every_encoding() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let app = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        for (source, solution_bearing) in build_v2_artifact_cases() {
            let invocation = CliParser::parse(source.split_whitespace())
                .unwrap_or_else(|error| panic!("parse native CLI {source}: {error:?}"));
            let request =
                CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
                    .unwrap_or_else(|output| panic!("assemble {source}: {}", output.stderr()))
                    .request();
            let response = app.run(request);
            assert_eq!(
                response.status(),
                AppStatus::Success,
                "{source}: {:?}",
                response.error()
            );

            if !solution_bearing {
                assert_eq!(
                    materialize_response(&response),
                    Err(SolutionArtifactOutputError::SolutionSetSurfaceUnavailable),
                    "{source}"
                );
                for format in [
                    SolutionArtifactOutputFormat::Compact,
                    SolutionArtifactOutputFormat::Json,
                    SolutionArtifactOutputFormat::Ctk3,
                    SolutionArtifactOutputFormat::Fumen,
                ] {
                    let error = SolutionArtifactOutputRequest::new("unused", format)
                        .prepare(&response)
                        .expect_err(
                            "candidate-only and probability results own no solution artifact",
                        );
                    assert_eq!(
                        error,
                        SolutionArtifactOutputError::SolutionSetSurfaceUnavailable,
                        "{source} {format:?}"
                    );
                }
                continue;
            }

            let compact = SolutionArtifactOutputRequest::new(
                "unused.csa",
                SolutionArtifactOutputFormat::Compact,
            )
            .prepare(&response)
            .unwrap_or_else(|error| panic!("compact artifact {source}: {error}"));
            assert!(compact.artifact.solution_count() > 0, "{source}");
            CompactSolutionSetEncoder
                .encode_checked(
                    &compact.artifact,
                    DEFAULT_MAX_ARTIFACT_BYTES,
                    &NeverCancelled,
                )
                .unwrap_or_else(|error| panic!("compact encode {source}: {error:?}"));

            let json = SolutionArtifactOutputRequest::new(
                "unused.json",
                SolutionArtifactOutputFormat::Json,
            )
            .prepare(&response)
            .unwrap_or_else(|error| panic!("JSON artifact {source}: {error}"));
            assert_eq!(
                json.artifact.solution_count(),
                compact.artifact.solution_count()
            );
            JsonSolutionSetEncoder
                .encode_checked(&json.artifact, DEFAULT_MAX_ARTIFACT_BYTES, &NeverCancelled)
                .unwrap_or_else(|error| panic!("JSON encode {source}: {error:?}"));

            let ctk3 = encode_response_document(&response, SolutionArtifactOutputFormat::Ctk3)
                .unwrap_or_else(|error| panic!("CTK3 artifact {source}: {error}"));
            assert!(ctk3.starts_with("ctk3_"), "{source}: {ctk3}");
            let fumen = encode_response_document(&response, SolutionArtifactOutputFormat::Fumen)
                .unwrap_or_else(|error| panic!("Fumen artifact {source}: {error}"));
            assert!(fumen.starts_with("v115@"), "{source}: {fumen}");
        }
    }

    #[test]
    fn explicit_build_score_portfolio_keeps_its_colored_key_authority_in_every_encoding() {
        let _resource_guard =
            crate::execution_resource_test_support::execution_resource_test_guard();
        let document = build_v2_artifact_document();
        let source = format!(
            "clearra build setup-cover-score --target-format ctk3 --target-document {document} --queue I --no-hold --objective max-score-cover --score-profile tetrio --initial-b2b 0 --workers 2"
        );
        let invocation = CliParser::parse(source.split_whitespace()).expect("native score CLI");
        let request =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
                .expect("typed Build score request")
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

        let suffix = NEXT_BUILD_ARTIFACT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "clearra-build-artifact-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).expect("isolated Build artifact directory");
        let snapshot_path = directory.join("score-portfolios.jsonl");
        let portfolio = initialize_snapshot(&response, &snapshot_path.to_string_lossy())
            .expect("Build score product-owned portfolio");
        let artifact = materialize_explicit_portfolio(&portfolio)
            .expect("explicit colored-field portfolio artifact");

        assert_eq!(
            artifact.normalized_key_algorithm(),
            PORTFOLIO_COLORED_FIELD_KEY_ALGORITHM
        );
        assert!(artifact.solution_count() > 0);
        assert!(artifact
            .entries()
            .iter()
            .all(|entry| entry.key().starts_with("cfk1|")));
        CompactSolutionSetEncoder
            .encode_checked(&artifact, DEFAULT_MAX_ARTIFACT_BYTES, &NeverCancelled)
            .expect("compact colored portfolio");
        JsonSolutionSetEncoder
            .encode_checked(&artifact, DEFAULT_MAX_ARTIFACT_BYTES, &NeverCancelled)
            .expect("JSON colored portfolio");
        let ctk3 =
            encode_explicit_portfolio_document(&portfolio, SolutionArtifactOutputFormat::Ctk3)
                .expect("CTK3 colored portfolio");
        assert!(ctk3.starts_with("ctk3_"));
        let fumen =
            encode_explicit_portfolio_document(&portfolio, SolutionArtifactOutputFormat::Fumen)
                .expect("Fumen colored portfolio");
        assert!(fumen.starts_with("v115@"));

        std::fs::remove_file(&snapshot_path).expect("remove isolated snapshot");
        std::fs::remove_dir(&directory).expect("remove isolated artifact directory");
    }

    fn native_json_commit(artifact: &SolutionSetArtifact) -> ArtifactCommit {
        let plan = JsonSolutionSetEncoder
            .measure_checked(artifact, DEFAULT_MAX_ARTIFACT_BYTES, &NeverCancelled)
            .expect("plan");
        let mut bytes = Vec::new();
        let receipt = JsonSolutionSetEncoder
            .encode_into(artifact, &plan, &mut bytes, &NeverCancelled)
            .expect("receipt");
        ArtifactCommit::from_native_receipt(
            &receipt,
            FileIdentity::Linux {
                device: 0x11,
                inode: 0x22,
            },
        )
    }

    #[test]
    fn typed_source_keeps_complete_annotations_out_of_solution_identity() {
        let artifact = source().artifact().expect("artifact");
        assert_eq!(artifact.entries()[0].key(), "solution-a");
        assert_eq!(
            artifact.entries()[0].annotation().pc_probability(),
            Some("0.5")
        );
        assert!(artifact.entries()[1].annotation().is_empty());
        assert_eq!(artifact.normalized_set_hash(), "hash:1");
    }

    #[test]
    fn explicit_portfolio_artifact_identity_binds_the_current_page_and_ordered_members() {
        let first =
            materialize_explicit_portfolio(&explicit_portfolio("1", explicit_portfolio_members()))
                .expect("first portfolio artifact");
        let second =
            materialize_explicit_portfolio(&explicit_portfolio("2", explicit_portfolio_members()))
                .expect("second portfolio artifact");

        assert_eq!(
            first.source_solution_set_contract(),
            PORTFOLIO_PAGE_SOURCE_CONTRACT
        );
        assert_eq!(
            first.normalized_set_hash_algorithm(),
            PORTFOLIO_PAGE_IDENTITY_ALGORITHM
        );
        assert!(is_sha256_hex(first.normalized_set_hash()));
        assert_ne!(
            first.normalized_set_hash(),
            second.normalized_set_hash(),
            "the alternative index is part of the current portfolio artifact identity"
        );
        assert_eq!(first.solution_count(), 2);
        assert!(first.entries()[0].key() < first.entries()[1].key());
    }

    #[test]
    fn explicit_portfolio_artifact_revalidates_boundary_metadata_and_member_ids() {
        let members = explicit_portfolio_members();
        let invalid_digest = ExplicitPortfolioOutput::for_test(
            &"A".repeat(64),
            &"b".repeat(64),
            Some("1"),
            members.len(),
            members.clone(),
            "1",
            Some("1"),
            true,
            None,
        );
        assert_eq!(
            materialize_explicit_portfolio(&invalid_digest),
            Err(SolutionArtifactOutputError::SolutionSetContractInvalid)
        );

        let noncanonical_id = explicit_portfolio(
            "1",
            vec![
                ExplicitPortfolioMember::for_test("01", members[0].normalized_key()),
                ExplicitPortfolioMember::for_test("2", members[1].normalized_key()),
            ],
        );
        assert_eq!(
            materialize_explicit_portfolio(&noncanonical_id),
            Err(SolutionArtifactOutputError::SolutionSetContractInvalid)
        );

        let inconsistent_completion = ExplicitPortfolioOutput::for_test(
            &"a".repeat(64),
            &"b".repeat(64),
            Some("1"),
            members.len(),
            members,
            "1",
            None,
            true,
            None,
        );
        assert_eq!(
            materialize_explicit_portfolio(&inconsistent_completion),
            Err(SolutionArtifactOutputError::SolutionSetContractInvalid)
        );
    }

    #[test]
    fn annotation_rows_fail_closed_on_unknown_or_duplicate_keys() {
        let mut unknown = source();
        unknown.probabilities[0].key = "not-a-solution".to_owned();
        assert_eq!(
            unknown.artifact(),
            Err(SolutionArtifactOutputError::UnknownAnnotationKey)
        );

        let mut duplicate = source();
        duplicate
            .probabilities
            .push(duplicate.probabilities[0].clone());
        assert_eq!(
            duplicate.artifact(),
            Err(SolutionArtifactOutputError::DuplicateAnnotationKey)
        );

        let mut descending = source();
        descending.probabilities = vec![
            TestMetricRow {
                key: "solution-b".to_owned(),
                value: "0.5".to_owned(),
                complete: true,
            },
            TestMetricRow {
                key: "solution-a".to_owned(),
                value: "0.5".to_owned(),
                complete: true,
            },
        ];
        assert_eq!(
            descending.artifact(),
            Err(SolutionArtifactOutputError::AnnotationOrderInvalid)
        );
    }

    #[test]
    fn source_and_annotation_sequences_must_keep_canonical_strict_order() {
        let mut unsorted = source();
        unsorted.keys.swap(0, 1);
        assert_eq!(
            unsorted.artifact(),
            Err(SolutionArtifactOutputError::SolutionKeyOrderInvalid)
        );

        let mut duplicated = source();
        duplicated.keys[1] = duplicated.keys[0].clone();
        assert_eq!(
            duplicated.artifact(),
            Err(SolutionArtifactOutputError::SolutionKeyOrderInvalid)
        );
    }

    #[test]
    fn model_capacity_is_checked_before_any_final_entry_is_constructed() {
        let source = source();
        let planned = planned_materialized_model_bytes(
            source.view(),
            source.probabilities.iter().map(TestMetricRow::view),
            source.scores.iter().map(TestMetricRow::view),
        )
        .expect("checked plan");
        let result = materialize_artifact_with_limit(
            source.view(),
            || source.probabilities.iter().map(TestMetricRow::view),
            || source.scores.iter().map(TestMetricRow::view),
            planned - 1,
        );
        assert_eq!(
            result,
            Err(SolutionArtifactOutputError::ArtifactModelCapacityExceeded)
        );
        assert!(source.artifact().is_ok(), "source is otherwise valid");
    }

    #[test]
    fn large_source_view_and_metric_iteration_have_constant_auxiliary_ownership() {
        let keys = (0..100_000)
            .map(|index| format!("solution-{index:06}"))
            .collect::<Vec<_>>();
        let view = ArtifactSourceView {
            source_contract: "test-solution-set",
            key_algorithm: "key-v1",
            set_hash_algorithm: "hash-v1",
            set_hash: "hash:large",
            expected_solution_count: keys.len(),
            keys: &keys,
        };
        let empty = TestSource {
            source_contract: "test-solution-set".to_owned(),
            key_algorithm: "key-v1".to_owned(),
            set_hash_algorithm: "hash-v1".to_owned(),
            set_hash: "hash:empty".to_owned(),
            expected_solution_count: 0,
            keys: Vec::new(),
            probabilities: Vec::new(),
            scores: Vec::new(),
        };
        assert_eq!(
            std::mem::size_of_val(&view),
            std::mem::size_of_val(&empty.view()),
            "the borrowed source view must not own a collection proportional to the set"
        );
        let planned = planned_materialized_model_bytes(
            view,
            std::iter::empty(),
            keys.iter().step_by(2).map(|key| ArtifactMetricView {
                key,
                value: "1",
                complete: true,
            }),
        )
        .expect("large checked plan");
        assert!(planned > keys.iter().map(String::len).sum::<usize>() as u64);
        assert!(planned < MAX_MATERIALIZED_ARTIFACT_MODEL_BYTES);
    }

    #[test]
    fn typed_tiling_family_pages_more_than_one_hundred_keys_in_exact_order() {
        const SOLUTION_COUNT: usize = 137;
        const PAGE_LIMIT: usize = 19;
        let canonical = (0..SOLUTION_COUNT)
            .map(|index| {
                NormalizedTilingSolutionKey::from_placements(
                    index as u64,
                    Vec::<PiecePlacementMask>::new(),
                )
                .expect("canonical test identity")
            })
            .collect::<Vec<_>>();
        let mut hasher = NormalizedTilingSolutionSetHasher::default();
        for key in &canonical {
            hasher.update_canonical_key(key);
        }
        let set_hash = hasher.finish();
        let keys = canonical
            .iter()
            .map(|key| key.as_str().to_owned())
            .collect::<Vec<_>>();
        let next_offset = Cell::new(0_usize);

        let artifact = materialize_paged_pc_tiling_family_with_limit(
            SOLUTION_COUNT,
            &set_hash,
            PAGE_LIMIT,
            MAX_MATERIALIZED_ARTIFACT_MODEL_BYTES,
            |offset, limit| {
                assert_eq!(offset, next_offset.get());
                assert!(limit <= PAGE_LIMIT);
                next_offset.set(offset + limit);
                Ok(keys[offset..offset + limit].to_vec())
            },
        )
        .expect("complete paged tiling artifact");

        assert_eq!(next_offset.get(), SOLUTION_COUNT);
        assert_eq!(artifact.solution_count(), SOLUTION_COUNT);
        assert_eq!(artifact.normalized_set_hash(), set_hash);
        assert!(artifact
            .entries()
            .iter()
            .map(SolutionArtifactEntry::key)
            .eq(keys.iter().map(String::as_str)));
    }

    #[test]
    fn canonical_algorithms_supply_only_the_existing_contract_when_the_backend_omits_it() {
        let fields = Vec::new();
        assert_eq!(
            source_contract(
                &fields,
                NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
                NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
            ),
            Ok(NORMALIZED_TILING_SOURCE_CONTRACT)
        );

        assert_eq!(
            source_contract(&fields, "parallel-key-v1", "parallel-hash-v1"),
            Err(SolutionArtifactOutputError::IdentityMetadataMismatch)
        );
    }

    #[test]
    fn declared_source_contract_must_match_the_canonical_algorithm_pair() {
        let canonical = vec![(
            "actual_solution_set_contract".to_owned(),
            NORMALIZED_TILING_SOURCE_CONTRACT.to_owned(),
        )];
        assert_eq!(
            source_contract(&canonical, "parallel-key-v1", "parallel-hash-v1"),
            Err(SolutionArtifactOutputError::IdentityMetadataMismatch)
        );

        let incompatible = vec![(
            "actual_solution_set_contract".to_owned(),
            "parallel-solution-set".to_owned(),
        )];
        assert_eq!(
            source_contract(
                &incompatible,
                NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
                NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
            ),
            Err(SolutionArtifactOutputError::IdentityMetadataMismatch)
        );
    }

    #[test]
    fn committed_json_metadata_is_added_only_from_a_sink_commit() {
        let artifact = source().artifact().expect("artifact");
        let pending = PendingSolutionArtifact::try_new(
            PathBuf::from("solutions.json"),
            SolutionArtifactOutputFormat::Json,
            artifact.clone(),
            DEFAULT_MAX_ARTIFACT_BYTES,
            "{\"kind\":\"pc\"}".to_owned(),
            RenderFormat::Json,
        )
        .expect("pending");
        assert!(!pending.base_stdout.contains("committed"));
        let commit = native_json_commit(&artifact);
        let stdout = pending.committed_stdout(&commit).expect("stdout");
        let value: serde_json::Value = serde_json::from_str(&stdout).expect("JSON");

        assert_eq!(value["solution_artifact"]["status"], "committed");
        assert_eq!(
            value["solution_artifact"]["schema"],
            "solution-set-artifact.v1"
        );
        assert_eq!(value["solution_artifact"]["solution_count"], 2);
        assert_eq!(value["solution_artifact"]["target_owned"], true);
        assert_eq!(value["solution_artifact"]["compression"], "none");
        assert_eq!(
            value["solution_artifact"]["uncompressed_bytes"],
            value["solution_artifact"]["bytes"]
        );
        assert_eq!(commit.uncompressed_bytes(), commit.byte_count());
        assert_eq!(
            value["solution_artifact"]["file_identity_kind"],
            "linux-device-inode"
        );
    }

    #[test]
    fn fumen_like_stdout_and_reserved_metadata_collisions_are_rejected_before_commit() {
        let artifact = source().artifact().expect("artifact");
        assert_eq!(
            PendingSolutionArtifact::try_new(
                PathBuf::from("solutions.csa"),
                SolutionArtifactOutputFormat::Compact,
                artifact.clone(),
                DEFAULT_MAX_ARTIFACT_BYTES,
                "v115@fake".to_owned(),
                RenderFormat::FumenLike,
            ),
            Err(SolutionArtifactOutputError::StdoutFormatUnsupported)
        );
        assert_eq!(
            PendingSolutionArtifact::try_new(
                PathBuf::from("solutions.csa"),
                SolutionArtifactOutputFormat::Compact,
                artifact,
                DEFAULT_MAX_ARTIFACT_BYTES,
                "solution_artifact_status: fake".to_owned(),
                RenderFormat::Text,
            ),
            Err(SolutionArtifactOutputError::StdoutMetadataCollision)
        );
    }

    #[test]
    fn artifact_execution_gate_requires_every_independent_success_axis() {
        assert_eq!(
            validate_execution_gate(true, true, "available", true, "complete", false),
            Ok(())
        );
        for (actual, expected) in [
            (
                validate_execution_gate(false, true, "available", true, "complete", false),
                SolutionArtifactOutputError::ResponseNotSuccessful,
            ),
            (
                validate_execution_gate(true, false, "available", true, "complete", false),
                SolutionArtifactOutputError::SolverNotExecuted,
            ),
            (
                validate_execution_gate(true, true, "unavailable", false, "complete", false),
                SolutionArtifactOutputError::ExecutionUnavailable,
            ),
            (
                validate_execution_gate(true, true, "available", false, "complete", false),
                SolutionArtifactOutputError::ExecutionUnavailable,
            ),
            (
                validate_execution_gate(true, true, "available", true, "incomplete", false),
                SolutionArtifactOutputError::ResultIncomplete,
            ),
            (
                validate_execution_gate(true, true, "available", true, "complete", true),
                SolutionArtifactOutputError::ResultTruncated,
            ),
        ] {
            assert_eq!(actual, Err(expected));
        }
    }

    #[test]
    fn complete_empty_materialized_source_remains_a_valid_artifact() {
        let artifact = TestSource {
            source_contract: "test-solution-set".to_owned(),
            key_algorithm: "key-v1".to_owned(),
            set_hash_algorithm: "hash-v1".to_owned(),
            set_hash: "hash:empty".to_owned(),
            expected_solution_count: 0,
            keys: Vec::new(),
            probabilities: Vec::new(),
            scores: Vec::new(),
        }
        .artifact()
        .expect("empty complete artifact");
        assert_eq!(artifact.solution_count(), 0);
        assert!(artifact.entries().is_empty());
    }
}
