//! Finite host DTOs for complete, native solution-set document artifacts.
//!
//! The solver and Rust document encoders remain the authority. Hosts may
//! transport an already encoded document, but they cannot rebuild a document
//! from solution keys or substitute an initial-field-only page.

pub const SOLUTION_SET_ARTIFACT_CONTRACT: &str = "solution-set-artifact.v2";
pub const HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES: u64 = 8 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionSetArtifactPayloadError {
    ContractInvalid,
    SourceIdentityInvalid,
    SelectionInvalid,
    SolutionCountInvalid,
    FormatSetInvalid,
    FormatInvalid,
    AvailabilityInvalid,
    DocumentInvalid,
    DocumentTooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SolutionSetArtifactFormatPayload {
    format: String,
    state: String,
    unavailable_reason: Option<String>,
    media_type: Option<String>,
    filename: Option<String>,
    byte_length: Option<u64>,
    sha256: Option<String>,
    page_count: Option<u64>,
    document: Option<String>,
}

impl SolutionSetArtifactFormatPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_available(
        format: impl Into<String>,
        media_type: impl Into<String>,
        filename: impl Into<String>,
        byte_length: u64,
        sha256: impl Into<String>,
        page_count: u64,
        document: impl Into<String>,
    ) -> Result<Self, SolutionSetArtifactPayloadError> {
        let payload = Self {
            format: format.into(),
            state: "available".to_owned(),
            unavailable_reason: None,
            media_type: Some(media_type.into()),
            filename: Some(filename.into()),
            byte_length: Some(byte_length),
            sha256: Some(sha256.into()),
            page_count: Some(page_count),
            document: Some(document.into()),
        };
        payload.validate()?;
        Ok(payload)
    }

    pub fn try_unavailable(
        format: impl Into<String>,
        reason: impl Into<String>,
    ) -> Result<Self, SolutionSetArtifactPayloadError> {
        let payload = Self {
            format: format.into(),
            state: "unavailable".to_owned(),
            unavailable_reason: Some(reason.into()),
            media_type: None,
            filename: None,
            byte_length: None,
            sha256: None,
            page_count: None,
            document: None,
        };
        payload.validate()?;
        Ok(payload)
    }

    fn validate(&self) -> Result<(), SolutionSetArtifactPayloadError> {
        if !matches!(self.format.as_str(), "ctk3" | "fumen") {
            return Err(SolutionSetArtifactPayloadError::FormatInvalid);
        }
        match self.state.as_str() {
            "available" => {
                if self.unavailable_reason.is_some() {
                    return Err(SolutionSetArtifactPayloadError::AvailabilityInvalid);
                }
                let (media_type, filename, byte_length, sha256, page_count, document) = match (
                    self.media_type.as_deref(),
                    self.filename.as_deref(),
                    self.byte_length,
                    self.sha256.as_deref(),
                    self.page_count,
                    self.document.as_deref(),
                ) {
                    (
                        Some(media_type),
                        Some(filename),
                        Some(byte_length),
                        Some(sha256),
                        Some(page_count),
                        Some(document),
                    ) => (
                        media_type,
                        filename,
                        byte_length,
                        sha256,
                        page_count,
                        document,
                    ),
                    _ => return Err(SolutionSetArtifactPayloadError::AvailabilityInvalid),
                };
                if byte_length == 0
                    || byte_length > HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES
                    || usize::try_from(byte_length).ok() != Some(document.len())
                {
                    return Err(SolutionSetArtifactPayloadError::DocumentTooLarge);
                }
                if page_count == 0
                    || !is_sha256_hex(sha256)
                    || !valid_document_shape(self.format.as_str(), media_type, filename, document)
                {
                    return Err(SolutionSetArtifactPayloadError::DocumentInvalid);
                }
            }
            "unavailable" => {
                if !matches!(
                    self.unavailable_reason.as_deref(),
                    Some(
                        "empty-solution-set"
                            | "unsupported-solution-key"
                            | "page-limit-exceeded"
                            | "encoding-failed"
                            | "transport-byte-limit-exceeded"
                    )
                ) || self.media_type.is_some()
                    || self.filename.is_some()
                    || self.byte_length.is_some()
                    || self.sha256.is_some()
                    || self.page_count.is_some()
                    || self.document.is_some()
                {
                    return Err(SolutionSetArtifactPayloadError::AvailabilityInvalid);
                }
            }
            _ => return Err(SolutionSetArtifactPayloadError::AvailabilityInvalid),
        }
        Ok(())
    }

    pub fn format(&self) -> &str {
        &self.format
    }
    pub fn state(&self) -> &str {
        &self.state
    }
    pub fn unavailable_reason(&self) -> Option<&str> {
        self.unavailable_reason.as_deref()
    }
    pub fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }
    pub fn filename(&self) -> Option<&str> {
        self.filename.as_deref()
    }
    pub const fn byte_length(&self) -> Option<u64> {
        self.byte_length
    }
    pub fn sha256(&self) -> Option<&str> {
        self.sha256.as_deref()
    }
    pub const fn page_count(&self) -> Option<u64> {
        self.page_count
    }
    pub fn document(&self) -> Option<&str> {
        self.document.as_deref()
    }

    pub const fn available(&self) -> bool {
        self.document.is_some()
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes =
            (self.format.capacity() as u128).checked_add(self.state.capacity() as u128)?;
        for value in [
            self.unavailable_reason.as_ref(),
            self.media_type.as_ref(),
            self.filename.as_ref(),
            self.sha256.as_ref(),
            self.document.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            bytes = bytes.checked_add(value.capacity() as u128)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SolutionSetArtifactPayload {
    contract: String,
    source_result_kind: String,
    source_solution_set_contract: String,
    selection_kind: String,
    selection_id: String,
    page_source_identity_sha256: Option<String>,
    normalized_key_algorithm: String,
    normalized_set_hash_algorithm: String,
    normalized_set_hash: String,
    solution_count: u64,
    completeness: String,
    formats: Vec<SolutionSetArtifactFormatPayload>,
}

impl SolutionSetArtifactPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        source_result_kind: impl Into<String>,
        source_solution_set_contract: impl Into<String>,
        selection_kind: impl Into<String>,
        selection_id: impl Into<String>,
        page_source_identity_sha256: Option<String>,
        normalized_key_algorithm: impl Into<String>,
        normalized_set_hash_algorithm: impl Into<String>,
        normalized_set_hash: impl Into<String>,
        solution_count: u64,
        formats: Vec<SolutionSetArtifactFormatPayload>,
    ) -> Result<Self, SolutionSetArtifactPayloadError> {
        let payload = Self {
            contract: SOLUTION_SET_ARTIFACT_CONTRACT.to_owned(),
            source_result_kind: source_result_kind.into(),
            source_solution_set_contract: source_solution_set_contract.into(),
            selection_kind: selection_kind.into(),
            selection_id: selection_id.into(),
            page_source_identity_sha256,
            normalized_key_algorithm: normalized_key_algorithm.into(),
            normalized_set_hash_algorithm: normalized_set_hash_algorithm.into(),
            normalized_set_hash: normalized_set_hash.into(),
            solution_count,
            completeness: "complete".to_owned(),
            formats,
        };
        payload.validate()?;
        Ok(payload)
    }

    fn validate(&self) -> Result<(), SolutionSetArtifactPayloadError> {
        if self.contract != SOLUTION_SET_ARTIFACT_CONTRACT || self.completeness != "complete" {
            return Err(SolutionSetArtifactPayloadError::ContractInvalid);
        }
        for value in [
            self.source_result_kind.as_str(),
            self.source_solution_set_contract.as_str(),
            self.normalized_key_algorithm.as_str(),
            self.normalized_set_hash_algorithm.as_str(),
            self.normalized_set_hash.as_str(),
        ] {
            if !valid_identity(value) {
                return Err(SolutionSetArtifactPayloadError::SourceIdentityInvalid);
            }
        }
        if !matches!(
            self.selection_kind.as_str(),
            "solution-family" | "portfolio-alternative" | "canonical-result"
        ) || !valid_identity(&self.selection_id)
        {
            return Err(SolutionSetArtifactPayloadError::SelectionInvalid);
        }
        if self
            .page_source_identity_sha256
            .as_deref()
            .is_some_and(|value| !is_sha256_hex(value))
        {
            return Err(SolutionSetArtifactPayloadError::SourceIdentityInvalid);
        }
        if self.solution_count == 0 {
            return Err(SolutionSetArtifactPayloadError::SolutionCountInvalid);
        }
        if self.formats.len() != 2
            || self.formats[0].format() != "ctk3"
            || self.formats[1].format() != "fumen"
            || !self
                .formats
                .iter()
                .any(SolutionSetArtifactFormatPayload::available)
        {
            return Err(SolutionSetArtifactPayloadError::FormatSetInvalid);
        }
        self.formats.iter().try_for_each(Self::validate_format)
    }

    fn validate_format(
        format: &SolutionSetArtifactFormatPayload,
    ) -> Result<(), SolutionSetArtifactPayloadError> {
        format.validate()
    }

    pub fn contract(&self) -> &str {
        &self.contract
    }
    pub fn source_result_kind(&self) -> &str {
        &self.source_result_kind
    }
    pub fn source_solution_set_contract(&self) -> &str {
        &self.source_solution_set_contract
    }
    pub fn selection_kind(&self) -> &str {
        &self.selection_kind
    }
    pub fn selection_id(&self) -> &str {
        &self.selection_id
    }
    pub fn page_source_identity_sha256(&self) -> Option<&str> {
        self.page_source_identity_sha256.as_deref()
    }
    pub fn normalized_key_algorithm(&self) -> &str {
        &self.normalized_key_algorithm
    }
    pub fn normalized_set_hash_algorithm(&self) -> &str {
        &self.normalized_set_hash_algorithm
    }
    pub fn normalized_set_hash(&self) -> &str {
        &self.normalized_set_hash
    }
    pub const fn solution_count(&self) -> u64 {
        self.solution_count
    }
    pub fn completeness(&self) -> &str {
        &self.completeness
    }
    pub fn formats(&self) -> &[SolutionSetArtifactFormatPayload] {
        &self.formats
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = [
            self.contract.capacity(),
            self.source_result_kind.capacity(),
            self.source_solution_set_contract.capacity(),
            self.selection_kind.capacity(),
            self.selection_id.capacity(),
            self.normalized_key_algorithm.capacity(),
            self.normalized_set_hash_algorithm.capacity(),
            self.normalized_set_hash.capacity(),
            self.completeness.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })?;
        if let Some(identity) = &self.page_source_identity_sha256 {
            bytes = bytes.checked_add(identity.capacity() as u128)?;
        }
        bytes =
            bytes
                .checked_add((self.formats.capacity() as u128).checked_mul(
                    core::mem::size_of::<SolutionSetArtifactFormatPayload>() as u128,
                )?)?;
        for format in &self.formats {
            bytes = bytes.checked_add(format.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

fn valid_document_shape(format: &str, media_type: &str, filename: &str, document: &str) -> bool {
    match format {
        "ctk3" => {
            media_type == "application/vnd.clearra.ctk3"
                && filename.ends_with(".ctk3")
                && (document.starts_with("ctk3_") || document.starts_with("ctk3b_"))
        }
        "fumen" => {
            media_type == "text/plain;charset=utf-8"
                && filename.ends_with(".fumen")
                && document.starts_with("v115@")
        }
        _ => false,
    }
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available(format: &str) -> SolutionSetArtifactFormatPayload {
        let (media, filename, document) = if format == "ctk3" {
            (
                "application/vnd.clearra.ctk3",
                "clearra-solutions.ctk3",
                "ctk3_test",
            )
        } else {
            (
                "text/plain;charset=utf-8",
                "clearra-solutions.fumen",
                "v115@test",
            )
        };
        SolutionSetArtifactFormatPayload::try_available(
            format,
            media,
            filename,
            document.len() as u64,
            "0".repeat(64),
            1,
            document,
        )
        .expect("available format")
    }

    #[test]
    fn sidecar_requires_a_complete_nonempty_source_and_the_closed_format_set() {
        let payload = SolutionSetArtifactPayload::try_new(
            "pc-tiling-family.v1",
            "normalized-tiling-set",
            "solution-family",
            "cts1:test",
            None,
            "normalized-tiling-key-v1",
            "normalized-tiling-set-hash-v1",
            "cts1:test",
            1,
            vec![
                available("ctk3"),
                SolutionSetArtifactFormatPayload::try_unavailable("fumen", "page-limit-exceeded")
                    .unwrap(),
            ],
        )
        .expect("valid sidecar");
        assert_eq!(payload.contract(), SOLUTION_SET_ARTIFACT_CONTRACT);
        assert_eq!(payload.formats().len(), 2);

        assert_eq!(
            SolutionSetArtifactPayload::try_new(
                "pc-tiling-family.v1",
                "normalized-tiling-set",
                "solution-family",
                "cts1:test",
                None,
                "normalized-tiling-key-v1",
                "normalized-tiling-set-hash-v1",
                "cts1:test",
                0,
                vec![available("ctk3"), available("fumen")],
            ),
            Err(SolutionSetArtifactPayloadError::SolutionCountInvalid)
        );
    }

    #[test]
    fn unavailable_format_cannot_smuggle_a_partial_document() {
        let mut invalid = SolutionSetArtifactFormatPayload::try_unavailable(
            "ctk3",
            "transport-byte-limit-exceeded",
        )
        .unwrap();
        invalid.document = Some("ctk3_partial".to_owned());
        assert_eq!(
            invalid.validate(),
            Err(SolutionSetArtifactPayloadError::AvailabilityInvalid)
        );
    }
}
