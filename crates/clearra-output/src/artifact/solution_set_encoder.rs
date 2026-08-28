// SRP rationale: this module has one change reason: deterministic publication encoding for complete solution sets.
use std::{fmt, io, io::Write, str};

use clearra_platform_fs::{NeverCancelled, PublicationCheckpoint, PublicationControl};

use super::{
    solution_comment_layout::{SolutionArtifactAnnotation, SolutionCommentLayout},
    solution_document::{
        encode_ctk3_solution_set_into, encode_fumen_solution_set_checked, SolutionDocumentError,
        SolutionDocumentStreamError,
    },
    solution_set_artifact::{
        SolutionArtifactEntry, SolutionSetArtifact, SolutionSetArtifactError, MAX_ARTIFACT_ENTRIES,
        MAX_ARTIFACT_KEY_BYTES, SOLUTION_SET_ARTIFACT_SCHEMA_V1, SOLUTION_SET_ARTIFACT_SCHEMA_V2,
    },
};

const COMPACT_MAGIC: &[u8; 8] = b"CLRASSA\0";
const COMPACT_VERSION: u16 = 1;
const COMPACT_FLAG_COMPLETE: u16 = 1;
const COMPACT_COMPRESSION: &str = "key-prefix-v1";
pub const DEFAULT_MAX_ARTIFACT_BYTES: u64 = 512 << 20;
pub const MAX_IN_MEMORY_ARTIFACT_BYTES: u64 = 8 << 20;
const MAX_METADATA_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SolutionArtifactEncoding {
    CompactV1,
    JsonV1,
    Ctk3,
    Fumen,
}

impl SolutionArtifactEncoding {
    pub const fn schema(self) -> &'static str {
        match self {
            Self::CompactV1 | Self::JsonV1 => SOLUTION_SET_ARTIFACT_SCHEMA_V1,
            Self::Ctk3 | Self::Fumen => SOLUTION_SET_ARTIFACT_SCHEMA_V2,
        }
    }

    pub const fn keyword(self) -> &'static str {
        match self {
            Self::CompactV1 => "compact-v1",
            Self::JsonV1 => "json-v1",
            Self::Ctk3 => "ctk3",
            Self::Fumen => "fumen",
        }
    }

    pub const fn compression(self) -> &'static str {
        match self {
            Self::CompactV1 => COMPACT_COMPRESSION,
            Self::JsonV1 => "none",
            Self::Ctk3 => "ctk3-revision-native",
            Self::Fumen => "fumen-v115",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedSolutionSetArtifact {
    encoding: SolutionArtifactEncoding,
    bytes: Vec<u8>,
    checksum: String,
    uncompressed_bytes: u64,
    solution_count: usize,
    annotated_solution_count: usize,
}

impl EncodedSolutionSetArtifact {
    fn new(bytes: Vec<u8>, receipt: &ArtifactEncodingReceipt) -> Self {
        Self {
            encoding: receipt.encoding,
            bytes,
            checksum: receipt.checksum.clone(),
            uncompressed_bytes: receipt.uncompressed_bytes,
            solution_count: receipt.solution_count,
            annotated_solution_count: receipt.annotated_solution_count,
        }
    }

    pub const fn schema(&self) -> &'static str {
        self.encoding.schema()
    }

    pub const fn encoding(&self) -> SolutionArtifactEncoding {
        self.encoding
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn checksum(&self) -> &str {
        &self.checksum
    }

    pub const fn uncompressed_bytes(&self) -> u64 {
        self.uncompressed_bytes
    }

    pub fn encoded_bytes(&self) -> usize {
        self.bytes.len()
    }

    pub const fn solution_count(&self) -> usize {
        self.solution_count
    }

    pub const fn annotated_solution_count(&self) -> usize {
        self.annotated_solution_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactEncodingPlan {
    encoding: SolutionArtifactEncoding,
    byte_count: u64,
    checksum: String,
    uncompressed_bytes: u64,
    solution_count: usize,
    annotated_solution_count: usize,
}

impl ArtifactEncodingPlan {
    pub const fn encoding(&self) -> SolutionArtifactEncoding {
        self.encoding
    }

    pub const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub fn checksum(&self) -> &str {
        &self.checksum
    }

    pub const fn uncompressed_bytes(&self) -> u64 {
        self.uncompressed_bytes
    }

    pub const fn solution_count(&self) -> usize {
        self.solution_count
    }

    pub const fn annotated_solution_count(&self) -> usize {
        self.annotated_solution_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactEncodingReceipt {
    encoding: SolutionArtifactEncoding,
    byte_count: u64,
    checksum: String,
    uncompressed_bytes: u64,
    solution_count: usize,
    annotated_solution_count: usize,
}

impl ArtifactEncodingReceipt {
    pub const fn schema(&self) -> &'static str {
        self.encoding.schema()
    }

    pub const fn encoding(&self) -> SolutionArtifactEncoding {
        self.encoding
    }

    pub const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    pub fn checksum(&self) -> &str {
        &self.checksum
    }

    pub const fn uncompressed_bytes(&self) -> u64 {
        self.uncompressed_bytes
    }

    pub const fn solution_count(&self) -> usize {
        self.solution_count
    }

    pub const fn annotated_solution_count(&self) -> usize {
        self.annotated_solution_count
    }

    fn as_plan(&self) -> ArtifactEncodingPlan {
        ArtifactEncodingPlan {
            encoding: self.encoding,
            byte_count: self.byte_count,
            checksum: self.checksum.clone(),
            uncompressed_bytes: self.uncompressed_bytes,
            solution_count: self.solution_count,
            annotated_solution_count: self.annotated_solution_count,
        }
    }
}

pub trait SolutionArtifactEncoder {
    fn encoding(&self) -> SolutionArtifactEncoding;

    fn measure_checked(
        &self,
        artifact: &SolutionSetArtifact,
        maximum_bytes: u64,
        control: &dyn PublicationControl,
    ) -> Result<ArtifactEncodingPlan, SolutionArtifactEncodingError> {
        let mut discard = io::sink();
        encode_stream(
            self.encoding(),
            artifact,
            &mut discard,
            maximum_bytes,
            control,
            EncodingPass::Measure,
        )
        .map(|receipt| receipt.as_plan())
    }

    fn encode_into(
        &self,
        artifact: &SolutionSetArtifact,
        plan: &ArtifactEncodingPlan,
        output: &mut dyn Write,
        control: &dyn PublicationControl,
    ) -> Result<ArtifactEncodingReceipt, SolutionArtifactEncodingError> {
        if plan.encoding != self.encoding() {
            return Err(SolutionArtifactEncodingError::PlanMismatch);
        }
        let receipt = encode_stream(
            self.encoding(),
            artifact,
            output,
            plan.byte_count,
            control,
            EncodingPass::Write,
        )?;
        if receipt.as_plan() != *plan {
            return Err(SolutionArtifactEncodingError::PlanMismatch);
        }
        Ok(receipt)
    }

    fn encode(
        &self,
        artifact: &SolutionSetArtifact,
    ) -> Result<EncodedSolutionSetArtifact, SolutionArtifactEncodingError> {
        self.encode_checked(artifact, MAX_IN_MEMORY_ARTIFACT_BYTES, &NeverCancelled)
    }

    /// Materializes a verified in-memory document under an explicit caller
    /// limit. Native CLI stdout uses this boundary because it must retain one
    /// complete `String` until dispatch, while file publication continues to
    /// stream directly into its atomic staging handle.
    fn encode_checked(
        &self,
        artifact: &SolutionSetArtifact,
        maximum_bytes: u64,
        control: &dyn PublicationControl,
    ) -> Result<EncodedSolutionSetArtifact, SolutionArtifactEncodingError> {
        if maximum_bytes == 0 || maximum_bytes > DEFAULT_MAX_ARTIFACT_BYTES {
            return Err(SolutionArtifactEncodingError::CapacityExceeded);
        }
        let plan = self.measure_checked(artifact, maximum_bytes, control)?;
        if plan.encoding() != self.encoding() {
            return Err(SolutionArtifactEncodingError::PlanMismatch);
        }
        if plan.byte_count() > maximum_bytes {
            return Err(SolutionArtifactEncodingError::CapacityExceeded);
        }
        let capacity = usize::try_from(plan.byte_count)
            .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(capacity)
            .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?;
        let receipt = {
            let mut verifier =
                ArtifactStreamVerifier::new(&mut bytes, self.encoding(), plan.byte_count());
            let receipt = self.encode_into(artifact, &plan, &mut verifier, control)?;
            verifier.verify(&plan, &receipt)?;
            receipt
        };
        Ok(EncodedSolutionSetArtifact::new(bytes, &receipt))
    }
}

/// Sink-owned verifier for bytes emitted by an encoder implementation.
///
/// Encoders are an extension boundary and their receipt is not publication
/// authority. A sink wraps its actual destination with this verifier so a
/// buggy or adversarial encoder cannot return an honest receipt while writing
/// different bytes of the same length. Compact-v1 deliberately excludes its
/// four-byte trailing checksum from the payload CRC; JSON-v1 covers every
/// emitted byte.
pub(crate) struct ArtifactStreamVerifier<'a> {
    output: &'a mut dyn Write,
    encoding: SolutionArtifactEncoding,
    expected_bytes: u64,
    byte_count: u64,
    crc: u32,
    compact_tail: [u8; 4],
    compact_tail_length: usize,
}

impl<'a> ArtifactStreamVerifier<'a> {
    pub(crate) fn new(
        output: &'a mut dyn Write,
        encoding: SolutionArtifactEncoding,
        expected_bytes: u64,
    ) -> Self {
        Self {
            output,
            encoding,
            expected_bytes,
            byte_count: 0,
            crc: u32::MAX,
            compact_tail: [0; 4],
            compact_tail_length: 0,
        }
    }

    pub(crate) fn verify(
        self,
        plan: &ArtifactEncodingPlan,
        receipt: &ArtifactEncodingReceipt,
    ) -> Result<(), SolutionArtifactEncodingError> {
        if receipt.as_plan() != *plan
            || self.encoding != plan.encoding
            || self.byte_count != plan.byte_count
            || self.byte_count != self.expected_bytes
        {
            return Err(SolutionArtifactEncodingError::StreamVerificationFailed);
        }
        let checksum = match self.encoding {
            SolutionArtifactEncoding::CompactV1 => {
                if self.compact_tail_length != self.compact_tail.len() {
                    return Err(SolutionArtifactEncodingError::StreamVerificationFailed);
                }
                let checksum = !self.crc;
                if u32::from_le_bytes(self.compact_tail) != checksum {
                    return Err(SolutionArtifactEncodingError::StreamVerificationFailed);
                }
                checksum
            }
            SolutionArtifactEncoding::JsonV1
            | SolutionArtifactEncoding::Ctk3
            | SolutionArtifactEncoding::Fumen => !self.crc,
        };
        if receipt.checksum != format!("crc32:{checksum:08x}") {
            return Err(SolutionArtifactEncodingError::StreamVerificationFailed);
        }
        Ok(())
    }

    fn observe(&mut self, bytes: &[u8]) {
        match self.encoding {
            SolutionArtifactEncoding::CompactV1 => self.observe_compact(bytes),
            SolutionArtifactEncoding::JsonV1
            | SolutionArtifactEncoding::Ctk3
            | SolutionArtifactEncoding::Fumen => {
                self.crc = crc32_update(self.crc, bytes);
            }
        }
    }

    fn observe_compact(&mut self, mut bytes: &[u8]) {
        if self.compact_tail_length < self.compact_tail.len() {
            let fill = (self.compact_tail.len() - self.compact_tail_length).min(bytes.len());
            let end = self.compact_tail_length + fill;
            self.compact_tail[self.compact_tail_length..end].copy_from_slice(&bytes[..fill]);
            self.compact_tail_length = end;
            bytes = &bytes[fill..];
        }
        if bytes.is_empty() {
            return;
        }
        debug_assert_eq!(self.compact_tail_length, self.compact_tail.len());
        if bytes.len() >= self.compact_tail.len() {
            self.crc = crc32_update(self.crc, &self.compact_tail);
            let payload_end = bytes.len() - self.compact_tail.len();
            self.crc = crc32_update(self.crc, &bytes[..payload_end]);
            self.compact_tail.copy_from_slice(&bytes[payload_end..]);
        } else {
            let displaced = bytes.len();
            self.crc = crc32_update(self.crc, &self.compact_tail[..displaced]);
            self.compact_tail.copy_within(displaced.., 0);
            let tail_start = self.compact_tail.len() - displaced;
            self.compact_tail[tail_start..].copy_from_slice(bytes);
        }
    }
}

impl Write for ArtifactStreamVerifier<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let requested = u64::try_from(bytes.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "write length overflow"))?;
        let next = self
            .byte_count
            .checked_add(requested)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "byte count overflow"))?;
        if next > self.expected_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "encoder wrote beyond its checked plan",
            ));
        }
        let written = self.output.write(bytes)?;
        if written > bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "writer returned an impossible byte count",
            ));
        }
        self.byte_count = self
            .byte_count
            .checked_add(u64::try_from(written).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "written byte count overflow")
            })?)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "byte count overflow"))?;
        self.observe(&bytes[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompactSolutionSetEncoder;

impl CompactSolutionSetEncoder {
    pub fn decode(bytes: &[u8]) -> Result<SolutionSetArtifact, SolutionArtifactEncodingError> {
        decode_compact(bytes)
    }
}

impl SolutionArtifactEncoder for CompactSolutionSetEncoder {
    fn encoding(&self) -> SolutionArtifactEncoding {
        SolutionArtifactEncoding::CompactV1
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JsonSolutionSetEncoder;

impl SolutionArtifactEncoder for JsonSolutionSetEncoder {
    fn encoding(&self) -> SolutionArtifactEncoding {
        SolutionArtifactEncoding::JsonV1
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Ctk3SolutionSetEncoder;

impl SolutionArtifactEncoder for Ctk3SolutionSetEncoder {
    fn encoding(&self) -> SolutionArtifactEncoding {
        SolutionArtifactEncoding::Ctk3
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FumenSolutionSetEncoder;

impl SolutionArtifactEncoder for FumenSolutionSetEncoder {
    fn encoding(&self) -> SolutionArtifactEncoding {
        SolutionArtifactEncoding::Fumen
    }
}

fn encode_stream(
    encoding: SolutionArtifactEncoding,
    artifact: &SolutionSetArtifact,
    output: &mut dyn Write,
    maximum_bytes: u64,
    control: &dyn PublicationControl,
    pass: EncodingPass,
) -> Result<ArtifactEncodingReceipt, SolutionArtifactEncodingError> {
    if control.cancelled_at(pass.before_checkpoint()) {
        return Err(SolutionArtifactEncodingError::Cancelled);
    }
    let mut output = EncodingWriter::new(output, maximum_bytes);
    match encoding {
        SolutionArtifactEncoding::CompactV1 => {
            let uncompressed_bytes = uncompressed_size_checked(artifact)?;
            encode_compact_into(artifact, &mut output, control, pass)?;
            let checksum = output.checksum();
            output.emit_without_checksum(&checksum.to_le_bytes())?;
            Ok(encoding_receipt(
                encoding,
                output.byte_count(),
                checksum,
                uncompressed_bytes,
                artifact,
            ))
        }
        SolutionArtifactEncoding::JsonV1 => {
            encode_json_into(artifact, &mut output, control, pass)?;
            let checksum = output.checksum();
            let uncompressed_bytes = output.byte_count();
            Ok(encoding_receipt(
                encoding,
                output.byte_count(),
                checksum,
                uncompressed_bytes,
                artifact,
            ))
        }
        SolutionArtifactEncoding::Ctk3 => {
            encode_ctk3_solution_set_into(
                artifact,
                |bytes| output.emit(bytes),
                |completed| encoding_checkpoint(control, pass, completed),
            )
            .map_err(map_document_stream_error)?;
            let checksum = output.checksum();
            let uncompressed_bytes = output.byte_count();
            Ok(encoding_receipt(
                encoding,
                output.byte_count(),
                checksum,
                uncompressed_bytes,
                artifact,
            ))
        }
        SolutionArtifactEncoding::Fumen => {
            let document = encode_fumen_solution_set_checked(artifact, |completed| {
                encoding_checkpoint(control, pass, completed)
            })
            .map_err(map_document_stream_error)?;
            output.emit(document.as_bytes())?;
            let checksum = output.checksum();
            let uncompressed_bytes = output.byte_count();
            Ok(encoding_receipt(
                encoding,
                output.byte_count(),
                checksum,
                uncompressed_bytes,
                artifact,
            ))
        }
    }
}

fn map_document_stream_error(
    error: SolutionDocumentStreamError<SolutionArtifactEncodingError>,
) -> SolutionArtifactEncodingError {
    match error {
        SolutionDocumentStreamError::Sink(error) => error,
        SolutionDocumentStreamError::Document(error) => match error {
            SolutionDocumentError::EmptySolutionSet => SolutionArtifactEncodingError::EmptyDocument,
            SolutionDocumentError::InvalidCanonicalKey => {
                SolutionArtifactEncodingError::InvalidDocumentSolutionKey
            }
            SolutionDocumentError::CapacityExceeded => {
                SolutionArtifactEncodingError::CapacityExceeded
            }
            SolutionDocumentError::Ctk3EncodingFailed => {
                SolutionArtifactEncodingError::Ctk3EncodingFailed
            }
            SolutionDocumentError::Ctk3PageLimitExceeded => {
                SolutionArtifactEncodingError::Ctk3PageLimitExceeded
            }
            SolutionDocumentError::FumenEncodingFailed => {
                SolutionArtifactEncodingError::FumenEncodingFailed
            }
            SolutionDocumentError::FumenPageLimitExceeded => {
                SolutionArtifactEncodingError::FumenPageLimitExceeded
            }
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EncodingPass {
    Measure,
    Write,
}

impl EncodingPass {
    const fn before_checkpoint(self) -> PublicationCheckpoint {
        match self {
            Self::Measure => PublicationCheckpoint::BeforeMeasure,
            Self::Write => PublicationCheckpoint::BeforeEncoding,
        }
    }

    const fn progress_checkpoint(self, completed_units: u64) -> PublicationCheckpoint {
        match self {
            Self::Measure => PublicationCheckpoint::MeasuringProgress { completed_units },
            Self::Write => PublicationCheckpoint::EncodingProgress { completed_units },
        }
    }
}

fn encoding_receipt(
    encoding: SolutionArtifactEncoding,
    byte_count: u64,
    checksum: u32,
    uncompressed_bytes: u64,
    artifact: &SolutionSetArtifact,
) -> ArtifactEncodingReceipt {
    ArtifactEncodingReceipt {
        encoding,
        byte_count,
        checksum: format!("crc32:{checksum:08x}"),
        uncompressed_bytes,
        solution_count: artifact.solution_count(),
        annotated_solution_count: artifact.annotated_solution_count(),
    }
}

fn encode_compact_into(
    artifact: &SolutionSetArtifact,
    output: &mut EncodingWriter<'_>,
    control: &dyn PublicationControl,
    pass: EncodingPass,
) -> Result<(), SolutionArtifactEncodingError> {
    output.emit(COMPACT_MAGIC)?;
    output.emit(&COMPACT_VERSION.to_le_bytes())?;
    output.emit(&COMPACT_FLAG_COMPLETE.to_le_bytes())?;
    output.emit(&0_u32.to_le_bytes())?;
    emit_compact_string(output, SOLUTION_SET_ARTIFACT_SCHEMA_V1)?;
    emit_compact_string(output, artifact.source_solution_set_contract())?;
    emit_compact_string(output, artifact.normalized_key_algorithm())?;
    emit_compact_string(output, artifact.normalized_set_hash_algorithm())?;
    emit_compact_string(output, artifact.normalized_set_hash())?;
    output.emit(
        &u64::try_from(artifact.solution_count())
            .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?
            .to_le_bytes(),
    )?;

    let mut previous = "";
    for (index, entry) in artifact.entries().iter().enumerate() {
        encoding_checkpoint(control, pass, index)?;
        let prefix = common_prefix_bytes(previous.as_bytes(), entry.key().as_bytes());
        output.emit(
            &u32::try_from(prefix)
                .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?
                .to_le_bytes(),
        )?;
        emit_compact_bytes(output, &entry.key().as_bytes()[prefix..])?;
        let annotation = entry.annotation();
        let mut flags = 0_u8;
        if annotation.pc_probability().is_some() {
            flags |= 1;
        }
        if annotation.average_score().is_some() {
            flags |= 2;
        }
        output.emit(&[flags])?;
        if let Some(value) = annotation.pc_probability() {
            emit_compact_string(output, value)?;
        }
        if let Some(value) = annotation.average_score() {
            emit_compact_string(output, value)?;
        }
        previous = entry.key();
    }
    Ok(())
}

fn emit_compact_string(
    output: &mut EncodingWriter<'_>,
    value: &str,
) -> Result<(), SolutionArtifactEncodingError> {
    emit_compact_bytes(output, value.as_bytes())
}

fn emit_compact_bytes(
    output: &mut EncodingWriter<'_>,
    value: &[u8],
) -> Result<(), SolutionArtifactEncodingError> {
    output.emit(
        &u32::try_from(value.len())
            .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?
            .to_le_bytes(),
    )?;
    output.emit(value)
}

fn encode_json_into(
    artifact: &SolutionSetArtifact,
    output: &mut EncodingWriter<'_>,
    control: &dyn PublicationControl,
    pass: EncodingPass,
) -> Result<(), SolutionArtifactEncodingError> {
    output.emit(b"{\"schema\":")?;
    emit_json_string(output, SOLUTION_SET_ARTIFACT_SCHEMA_V1)?;
    output.emit(b",\"encoding\":\"json-v1\",\"compression\":\"none\"")?;
    output.emit(b",\"completeness\":\"complete\"")?;
    output.emit(b",\"comment_authority\":")?;
    emit_json_string(output, SolutionCommentLayout::authority())?;
    output.emit(b",\"source_solution_set_contract\":")?;
    emit_json_string(output, artifact.source_solution_set_contract())?;
    output.emit(b",\"normalized_key_algorithm\":")?;
    emit_json_string(output, artifact.normalized_key_algorithm())?;
    output.emit(b",\"normalized_set_hash_algorithm\":")?;
    emit_json_string(output, artifact.normalized_set_hash_algorithm())?;
    output.emit(b",\"normalized_set_hash\":")?;
    emit_json_string(output, artifact.normalized_set_hash())?;
    output.emit(b",\"solution_count\":")?;
    emit_decimal_usize(output, artifact.solution_count())?;
    output.emit(b",\"solutions\":[")?;
    for (index, entry) in artifact.entries().iter().enumerate() {
        encoding_checkpoint(control, pass, index)?;
        if index != 0 {
            output.emit(b",")?;
        }
        output.emit(b"{\"key\":")?;
        emit_json_string(output, entry.key())?;
        output.emit(b",\"annotation\":")?;
        emit_annotation_json(output, entry.annotation())?;
        output.emit(b",\"comment\":")?;
        if let Some(comment) = SolutionCommentLayout::render(entry.annotation()) {
            emit_json_string(output, &comment)?;
        } else {
            output.emit(b"null")?;
        }
        output.emit(b"}")?;
    }
    output.emit(b"]}")
}

fn emit_annotation_json(
    output: &mut EncodingWriter<'_>,
    annotation: &SolutionArtifactAnnotation,
) -> Result<(), SolutionArtifactEncodingError> {
    output.emit(b"{\"pc_probability\":")?;
    emit_optional_json_number(output, annotation.pc_probability())?;
    output.emit(b",\"average_score\":")?;
    emit_optional_json_number(output, annotation.average_score())?;
    output.emit(b"}")
}

fn emit_optional_json_number(
    output: &mut EncodingWriter<'_>,
    value: Option<&str>,
) -> Result<(), SolutionArtifactEncodingError> {
    output.emit(value.map_or(b"null".as_slice(), str::as_bytes))
}

fn emit_json_string(
    output: &mut EncodingWriter<'_>,
    value: &str,
) -> Result<(), SolutionArtifactEncodingError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    output.emit(b"\"")?;
    for character in value.chars() {
        match character {
            '"' => output.emit(b"\\\"")?,
            '\\' => output.emit(b"\\\\")?,
            '\u{08}' => output.emit(b"\\b")?,
            '\u{0c}' => output.emit(b"\\f")?,
            '\n' => output.emit(b"\\n")?,
            '\r' => output.emit(b"\\r")?,
            '\t' => output.emit(b"\\t")?,
            character if character <= '\u{1f}' => {
                let value = u32::from(character) as usize;
                output.emit(&[
                    b'\\',
                    b'u',
                    b'0',
                    b'0',
                    HEX[(value >> 4) & 0x0f],
                    HEX[value & 0x0f],
                ])?;
            }
            character => {
                let mut buffer = [0_u8; 4];
                output.emit(character.encode_utf8(&mut buffer).as_bytes())?;
            }
        }
    }
    output.emit(b"\"")
}

fn emit_decimal_usize(
    output: &mut EncodingWriter<'_>,
    mut value: usize,
) -> Result<(), SolutionArtifactEncodingError> {
    let mut digits = [0_u8; 40];
    let mut cursor = digits.len();
    loop {
        cursor -= 1;
        digits[cursor] = b'0' + u8::try_from(value % 10).expect("decimal digit");
        value /= 10;
        if value == 0 {
            break;
        }
    }
    output.emit(&digits[cursor..])
}

fn encoding_checkpoint(
    control: &dyn PublicationControl,
    pass: EncodingPass,
    completed_units: usize,
) -> Result<(), SolutionArtifactEncodingError> {
    let completed_units = u64::try_from(completed_units)
        .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?;
    if control.cancelled_at(pass.progress_checkpoint(completed_units)) {
        Err(SolutionArtifactEncodingError::Cancelled)
    } else {
        Ok(())
    }
}

struct EncodingWriter<'a> {
    output: &'a mut dyn Write,
    maximum_bytes: u64,
    byte_count: u64,
    crc: u32,
}

impl<'a> EncodingWriter<'a> {
    fn new(output: &'a mut dyn Write, maximum_bytes: u64) -> Self {
        Self {
            output,
            maximum_bytes,
            byte_count: 0,
            crc: u32::MAX,
        }
    }

    fn emit(&mut self, bytes: &[u8]) -> Result<(), SolutionArtifactEncodingError> {
        self.reserve(bytes.len())?;
        self.output
            .write_all(bytes)
            .map_err(|_| SolutionArtifactEncodingError::WriteFailed)?;
        self.crc = crc32_update(self.crc, bytes);
        Ok(())
    }

    fn emit_without_checksum(&mut self, bytes: &[u8]) -> Result<(), SolutionArtifactEncodingError> {
        self.reserve(bytes.len())?;
        self.output
            .write_all(bytes)
            .map_err(|_| SolutionArtifactEncodingError::WriteFailed)
    }

    fn reserve(&mut self, length: usize) -> Result<(), SolutionArtifactEncodingError> {
        let length =
            u64::try_from(length).map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?;
        let next = self
            .byte_count
            .checked_add(length)
            .ok_or(SolutionArtifactEncodingError::CapacityExceeded)?;
        if next > self.maximum_bytes {
            return Err(SolutionArtifactEncodingError::CapacityExceeded);
        }
        self.byte_count = next;
        Ok(())
    }

    const fn byte_count(&self) -> u64 {
        self.byte_count
    }

    const fn checksum(&self) -> u32 {
        !self.crc
    }
}

fn decode_compact(bytes: &[u8]) -> Result<SolutionSetArtifact, SolutionArtifactEncodingError> {
    if bytes.len() < COMPACT_MAGIC.len() + 2 + 2 + 4 + 8 + 4
        || u64::try_from(bytes.len())
            .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?
            > DEFAULT_MAX_ARTIFACT_BYTES
    {
        return Err(SolutionArtifactEncodingError::InvalidCompactEnvelope);
    }
    let (payload, encoded_checksum) = bytes.split_at(bytes.len() - 4);
    let expected_checksum = u32::from_le_bytes(
        encoded_checksum
            .try_into()
            .map_err(|_| SolutionArtifactEncodingError::InvalidCompactEnvelope)?,
    );
    if crc32(payload) != expected_checksum {
        return Err(SolutionArtifactEncodingError::ChecksumMismatch);
    }

    let mut reader = CompactReader::new(payload);
    if reader.take(COMPACT_MAGIC.len())? != COMPACT_MAGIC {
        return Err(SolutionArtifactEncodingError::InvalidCompactEnvelope);
    }
    if reader.u16()? != COMPACT_VERSION
        || reader.u16()? != COMPACT_FLAG_COMPLETE
        || reader.u32()? != 0
    {
        return Err(SolutionArtifactEncodingError::UnsupportedCompactContract);
    }
    if reader.string(MAX_METADATA_BYTES)? != SOLUTION_SET_ARTIFACT_SCHEMA_V1 {
        return Err(SolutionArtifactEncodingError::UnsupportedCompactContract);
    }
    let source_contract = reader.string(MAX_METADATA_BYTES)?;
    let key_algorithm = reader.string(MAX_METADATA_BYTES)?;
    let hash_algorithm = reader.string(MAX_METADATA_BYTES)?;
    let set_hash = reader.string(MAX_METADATA_BYTES)?;
    let count = usize::try_from(reader.u64()?)
        .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?;
    if count > MAX_ARTIFACT_ENTRIES {
        return Err(SolutionArtifactEncodingError::CapacityExceeded);
    }

    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count)
        .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?;
    let mut previous = String::new();
    for _ in 0..count {
        let prefix = usize::try_from(reader.u32()?)
            .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?;
        if prefix > previous.len() {
            return Err(SolutionArtifactEncodingError::InvalidPrefixCompression);
        }
        let suffix = reader.bytes(MAX_ARTIFACT_KEY_BYTES)?;
        let mut key_bytes = previous.as_bytes()[..prefix].to_vec();
        if key_bytes.len().saturating_add(suffix.len()) > MAX_ARTIFACT_KEY_BYTES {
            return Err(SolutionArtifactEncodingError::CapacityExceeded);
        }
        key_bytes.extend_from_slice(suffix);
        let key =
            String::from_utf8(key_bytes).map_err(|_| SolutionArtifactEncodingError::InvalidUtf8)?;
        if !previous.is_empty() && previous >= key {
            return Err(SolutionArtifactEncodingError::NonCanonicalOrder);
        }

        let flags = reader.byte()?;
        if flags & !3 != 0 {
            return Err(SolutionArtifactEncodingError::UnsupportedCompactContract);
        }
        let mut annotation = SolutionArtifactAnnotation::new();
        if flags & 1 != 0 {
            annotation = annotation
                .with_pc_probability(reader.string(128)?)
                .map_err(|_| SolutionArtifactEncodingError::InvalidAnnotation)?;
        }
        if flags & 2 != 0 {
            annotation = annotation
                .with_average_score(reader.string(128)?)
                .map_err(|_| SolutionArtifactEncodingError::InvalidAnnotation)?;
        }
        entries.push(
            SolutionArtifactEntry::try_new(key.clone(), annotation)
                .map_err(SolutionArtifactEncodingError::Artifact)?,
        );
        previous = key;
    }
    if !reader.is_empty() {
        return Err(SolutionArtifactEncodingError::TrailingBytes);
    }
    SolutionSetArtifact::try_new(
        source_contract,
        key_algorithm,
        hash_algorithm,
        set_hash,
        count,
        entries,
    )
    .map_err(SolutionArtifactEncodingError::Artifact)
}

struct CompactReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> CompactReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], SolutionArtifactEncodingError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(SolutionArtifactEncodingError::CapacityExceeded)?;
        let value = self
            .bytes
            .get(self.cursor..end)
            .ok_or(SolutionArtifactEncodingError::UnexpectedEnd)?;
        self.cursor = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, SolutionArtifactEncodingError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, SolutionArtifactEncodingError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(
            |_| SolutionArtifactEncodingError::UnexpectedEnd,
        )?))
    }

    fn u32(&mut self) -> Result<u32, SolutionArtifactEncodingError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(
            |_| SolutionArtifactEncodingError::UnexpectedEnd,
        )?))
    }

    fn u64(&mut self) -> Result<u64, SolutionArtifactEncodingError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(
            |_| SolutionArtifactEncodingError::UnexpectedEnd,
        )?))
    }

    fn bytes(&mut self, maximum: usize) -> Result<&'a [u8], SolutionArtifactEncodingError> {
        let length = usize::try_from(self.u32()?)
            .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?;
        if length > maximum {
            return Err(SolutionArtifactEncodingError::CapacityExceeded);
        }
        self.take(length)
    }

    fn string(&mut self, maximum: usize) -> Result<String, SolutionArtifactEncodingError> {
        str::from_utf8(self.bytes(maximum)?)
            .map(ToOwned::to_owned)
            .map_err(|_| SolutionArtifactEncodingError::InvalidUtf8)
    }

    fn is_empty(&self) -> bool {
        self.cursor == self.bytes.len()
    }
}

fn common_prefix_bytes(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count()
}

fn uncompressed_size_checked(
    artifact: &SolutionSetArtifact,
) -> Result<u64, SolutionArtifactEncodingError> {
    // Comparable compact-v1 envelope size when every key is stored in full:
    // fixed header, length-prefixed schema/metadata, count, entries, checksum.
    let mut size = 0_u64;
    for length in [
        COMPACT_MAGIC.len(),
        2 + 2 + 4,
        4 + SOLUTION_SET_ARTIFACT_SCHEMA_V1.len(),
        4 + artifact.source_solution_set_contract().len(),
        4 + artifact.normalized_key_algorithm().len(),
        4 + artifact.normalized_set_hash_algorithm().len(),
        4 + artifact.normalized_set_hash().len(),
        8,
        4,
    ] {
        size = checked_size_add(size, length)?;
    }
    for entry in artifact.entries() {
        size = checked_size_add(size, 4)?;
        size = checked_size_add(size, entry.key().len())?;
        size = checked_size_add(size, 1)?;
        if let Some(value) = entry.annotation().pc_probability() {
            size = checked_size_add(size, 4)?;
            size = checked_size_add(size, value.len())?;
        }
        if let Some(value) = entry.annotation().average_score() {
            size = checked_size_add(size, 4)?;
            size = checked_size_add(size, value.len())?;
        }
    }
    Ok(size)
}

fn checked_size_add(current: u64, additional: usize) -> Result<u64, SolutionArtifactEncodingError> {
    current
        .checked_add(
            u64::try_from(additional)
                .map_err(|_| SolutionArtifactEncodingError::CapacityExceeded)?,
        )
        .ok_or(SolutionArtifactEncodingError::CapacityExceeded)
}

fn crc32(bytes: &[u8]) -> u32 {
    !crc32_update(u32::MAX, bytes)
}

fn crc32_update(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320_u32 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    crc
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolutionArtifactEncodingError {
    Artifact(SolutionSetArtifactError),
    CapacityExceeded,
    WriteFailed,
    Cancelled,
    PlanMismatch,
    StreamVerificationFailed,
    InvalidCompactEnvelope,
    UnsupportedCompactContract,
    ChecksumMismatch,
    UnexpectedEnd,
    InvalidPrefixCompression,
    NonCanonicalOrder,
    InvalidUtf8,
    InvalidAnnotation,
    TrailingBytes,
    EmptyDocument,
    InvalidDocumentSolutionKey,
    Ctk3EncodingFailed,
    Ctk3PageLimitExceeded,
    FumenEncodingFailed,
    FumenPageLimitExceeded,
}

impl SolutionArtifactEncodingError {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Artifact(_) => "artifact-model-invalid",
            Self::CapacityExceeded => "artifact-capacity-exceeded",
            Self::WriteFailed => "artifact-write-failed",
            Self::Cancelled => "artifact-encoding-cancelled",
            Self::PlanMismatch => "artifact-plan-mismatch",
            Self::StreamVerificationFailed => "artifact-stream-verification-failed",
            Self::InvalidCompactEnvelope => "artifact-compact-envelope-invalid",
            Self::UnsupportedCompactContract => "artifact-compact-contract-unsupported",
            Self::ChecksumMismatch => "artifact-checksum-mismatch",
            Self::UnexpectedEnd => "artifact-unexpected-end",
            Self::InvalidPrefixCompression => "artifact-prefix-compression-invalid",
            Self::NonCanonicalOrder => "artifact-order-noncanonical",
            Self::InvalidUtf8 => "artifact-utf8-invalid",
            Self::InvalidAnnotation => "artifact-annotation-invalid",
            Self::TrailingBytes => "artifact-trailing-bytes",
            Self::EmptyDocument => "artifact-document-empty",
            Self::InvalidDocumentSolutionKey => "artifact-document-solution-key-unsupported",
            Self::Ctk3EncodingFailed => "artifact-ctk3-encoding-failed",
            Self::Ctk3PageLimitExceeded => "artifact-ctk3-page-limit-exceeded",
            Self::FumenEncodingFailed => "artifact-fumen-encoding-failed",
            Self::FumenPageLimitExceeded => "artifact-fumen-page-limit-exceeded",
        }
    }
}

impl fmt::Display for SolutionArtifactEncodingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Artifact(_) => "solution artifact model is invalid",
            Self::CapacityExceeded => "solution artifact capacity is exceeded",
            Self::WriteFailed => "solution artifact output write failed",
            Self::Cancelled => "solution artifact encoding was cancelled",
            Self::PlanMismatch => "solution artifact encoding plan does not match output",
            Self::StreamVerificationFailed => {
                "solution artifact bytes do not match the checked receipt"
            }
            Self::InvalidCompactEnvelope => "compact solution artifact envelope is invalid",
            Self::UnsupportedCompactContract => "compact solution artifact contract is unsupported",
            Self::ChecksumMismatch => "compact solution artifact checksum does not match",
            Self::UnexpectedEnd => "compact solution artifact ended unexpectedly",
            Self::InvalidPrefixCompression => "compact solution artifact prefix is invalid",
            Self::NonCanonicalOrder => "compact solution artifact keys are not canonical",
            Self::InvalidUtf8 => "compact solution artifact text is not UTF-8",
            Self::InvalidAnnotation => "compact solution annotation is invalid",
            Self::TrailingBytes => "compact solution artifact has trailing bytes",
            Self::EmptyDocument => "empty solution sets have no document pages",
            Self::InvalidDocumentSolutionKey => {
                "solution key cannot be represented as a colored document page"
            }
            Self::Ctk3EncodingFailed => "native CTK3 solution document encoding failed",
            Self::Ctk3PageLimitExceeded => "native CTK3 logical page limit exceeded",
            Self::FumenEncodingFailed => "native Fumen solution document encoding failed",
            Self::FumenPageLimitExceeded => "native Fumen page limit exceeded",
        })
    }
}

impl std::error::Error for SolutionArtifactEncodingError {}

#[cfg(test)]
pub(crate) struct SameLengthForgedCompactEncoder;

#[cfg(test)]
impl SolutionArtifactEncoder for SameLengthForgedCompactEncoder {
    fn encoding(&self) -> SolutionArtifactEncoding {
        SolutionArtifactEncoding::CompactV1
    }

    fn encode_into(
        &self,
        artifact: &SolutionSetArtifact,
        plan: &ArtifactEncodingPlan,
        output: &mut dyn Write,
        control: &dyn PublicationControl,
    ) -> Result<ArtifactEncodingReceipt, SolutionArtifactEncodingError> {
        let mut forged = FlipFirstByteWriter {
            output,
            flipped: false,
        };
        CompactSolutionSetEncoder.encode_into(artifact, plan, &mut forged, control)
    }
}

#[cfg(test)]
pub(crate) struct LimitIgnoringEncoder;

#[cfg(test)]
impl SolutionArtifactEncoder for LimitIgnoringEncoder {
    fn encoding(&self) -> SolutionArtifactEncoding {
        SolutionArtifactEncoding::JsonV1
    }

    fn measure_checked(
        &self,
        artifact: &SolutionSetArtifact,
        _maximum_bytes: u64,
        control: &dyn PublicationControl,
    ) -> Result<ArtifactEncodingPlan, SolutionArtifactEncodingError> {
        JsonSolutionSetEncoder.measure_checked(artifact, u64::MAX, control)
    }
}

#[cfg(test)]
struct FlipFirstByteWriter<'a> {
    output: &'a mut dyn Write,
    flipped: bool,
}

#[cfg(test)]
impl Write for FlipFirstByteWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.flipped || bytes.is_empty() {
            return self.output.write(bytes);
        }
        self.output.write_all(&[bytes[0] ^ 1])?;
        self.output.write_all(&bytes[1..])?;
        self.flipped = true;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use clearra_core_domain::solution::{NormalizedTilingSolutionKey, PiecePlacementMask};
    use clearra_ctk3::{CTK3_BUNDLE_PREFIX, CTK3_MAX_SEGMENT_PAGES};
    use clearra_fumen::FUMEN_MAX_PAGES;

    use super::*;

    fn artifact() -> SolutionSetArtifact {
        let annotated = SolutionArtifactAnnotation::new()
            .with_pc_probability("0.5")
            .expect("probability")
            .with_average_score("1200")
            .expect("score");
        SolutionSetArtifact::try_new(
            "test-solution-set",
            "test-key-v1",
            "test-set-hash-v1",
            "cts1:1234567890abcdef",
            3,
            vec![
                SolutionArtifactEntry::try_new(
                    "ctk1|initial=0000000000000000|placements=I:000000000000000f",
                    SolutionArtifactAnnotation::new(),
                )
                .expect("entry"),
                SolutionArtifactEntry::try_new(
                    "ctk1|initial=0000000000000000|placements=O:0000000000000033",
                    annotated,
                )
                .expect("entry"),
                SolutionArtifactEntry::try_new(
                    "ctk1|initial=0000000000000000|placements=T:0000000000000027",
                    SolutionArtifactAnnotation::new(),
                )
                .expect("entry"),
            ],
        )
        .expect("artifact")
    }

    #[test]
    fn compact_prefix_encoding_roundtrips_and_is_smaller_than_raw_keys() {
        let source = artifact();
        let encoded = CompactSolutionSetEncoder.encode(&source).expect("encoded");
        let decoded = CompactSolutionSetEncoder::decode(encoded.bytes()).expect("decoded");

        assert_eq!(decoded, source);
        assert_eq!(encoded.encoding(), SolutionArtifactEncoding::CompactV1);
        assert!(encoded
            .bytes()
            .windows(SOLUTION_SET_ARTIFACT_SCHEMA_V1.len())
            .any(|window| window == SOLUTION_SET_ARTIFACT_SCHEMA_V1.as_bytes()));
        assert!((encoded.encoded_bytes() as u64) < encoded.uncompressed_bytes());
        assert_eq!(encoded.annotated_solution_count(), 1);
    }

    #[test]
    fn every_single_bit_corruption_is_rejected_by_checksum() {
        let encoded = CompactSolutionSetEncoder
            .encode(&artifact())
            .expect("encoded");
        for index in 0..encoded.bytes().len() {
            for bit in 0..8 {
                let mut corrupted = encoded.bytes().to_vec();
                corrupted[index] ^= 1 << bit;
                assert!(
                    CompactSolutionSetEncoder::decode(&corrupted).is_err(),
                    "index={index} bit={bit}"
                );
            }
        }
    }

    #[test]
    fn forged_valid_checksum_cannot_bypass_magic_version_schema_or_trailing_byte_checks() {
        let encoded = CompactSolutionSetEncoder
            .encode(&artifact())
            .expect("encoded");

        let mut wrong_magic = encoded.bytes().to_vec();
        wrong_magic[0] ^= 1;
        refresh_checksum(&mut wrong_magic);
        assert_eq!(
            CompactSolutionSetEncoder::decode(&wrong_magic),
            Err(SolutionArtifactEncodingError::InvalidCompactEnvelope)
        );

        let mut wrong_version = encoded.bytes().to_vec();
        wrong_version[COMPACT_MAGIC.len()] = 2;
        refresh_checksum(&mut wrong_version);
        assert_eq!(
            CompactSolutionSetEncoder::decode(&wrong_version),
            Err(SolutionArtifactEncodingError::UnsupportedCompactContract)
        );

        let schema_offset = COMPACT_MAGIC.len() + 2 + 2 + 4 + 4;
        let mut wrong_schema = encoded.bytes().to_vec();
        wrong_schema[schema_offset] ^= 1;
        refresh_checksum(&mut wrong_schema);
        assert_eq!(
            CompactSolutionSetEncoder::decode(&wrong_schema),
            Err(SolutionArtifactEncodingError::UnsupportedCompactContract)
        );

        let mut trailing = encoded.bytes()[..encoded.bytes().len() - 4].to_vec();
        trailing.push(0);
        let checksum = crc32(&trailing);
        trailing.extend_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            CompactSolutionSetEncoder::decode(&trailing),
            Err(SolutionArtifactEncodingError::TrailingBytes)
        );
    }

    fn refresh_checksum(bytes: &mut [u8]) {
        let payload_length = bytes.len() - 4;
        let checksum = crc32(&bytes[..payload_length]);
        bytes[payload_length..].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn json_envelope_keeps_typed_annotations_and_comment_non_authority() {
        let encoded = JsonSolutionSetEncoder.encode(&artifact()).expect("JSON");
        let json = str::from_utf8(encoded.bytes()).expect("UTF-8 JSON");
        assert!(json.contains("\"schema\":\"solution-set-artifact.v1\""));
        assert!(json.contains("\"comment_authority\":\"annotation-only\""));
        assert!(json.contains("\"pc_probability\":0.5"));
        assert!(json.contains("\"average_score\":1200"));
        assert!(json.contains("authority=annotation-only\\npc_probability=0.5"));
    }

    #[test]
    fn native_documents_publish_the_v2_artifact_contract_without_changing_v1_envelopes() {
        let source = artifact();
        let compact = CompactSolutionSetEncoder.encode(&source).expect("compact");
        let json = JsonSolutionSetEncoder.encode(&source).expect("JSON");
        let ctk3 = Ctk3SolutionSetEncoder.encode(&source).expect("CTK3");
        let fumen = FumenSolutionSetEncoder.encode(&source).expect("Fumen");

        assert_eq!(compact.schema(), SOLUTION_SET_ARTIFACT_SCHEMA_V1);
        assert_eq!(json.schema(), SOLUTION_SET_ARTIFACT_SCHEMA_V1);
        assert_eq!(ctk3.schema(), SOLUTION_SET_ARTIFACT_SCHEMA_V2);
        assert_eq!(fumen.schema(), SOLUTION_SET_ARTIFACT_SCHEMA_V2);
        assert!(ctk3.bytes().starts_with(b"ctk3_"));
        assert!(fumen.bytes().starts_with(b"v115@"));
    }

    fn golden_artifact() -> SolutionSetArtifact {
        SolutionSetArtifact::try_new(
            "test-solution-set",
            "key-v1",
            "hash-v1",
            "hash:1",
            1,
            vec![
                SolutionArtifactEntry::try_new("solution-a", SolutionArtifactAnnotation::new())
                    .expect("entry"),
            ],
        )
        .expect("golden artifact")
    }

    fn native_document_artifact(solution_count: usize) -> SolutionSetArtifact {
        let entries = (0..solution_count)
            .map(|index| {
                let key = NormalizedTilingSolutionKey::from_placements(
                    index as u64,
                    Vec::<PiecePlacementMask>::new(),
                )
                .expect("canonical native document key");
                SolutionArtifactEntry::try_new(key.as_str(), SolutionArtifactAnnotation::new())
                    .expect("native document entry")
            })
            .collect::<Vec<_>>();
        SolutionSetArtifact::try_new(
            "test-native-document-set",
            "test-native-key-v1",
            "test-native-hash-v1",
            format!("hash:{solution_count}"),
            solution_count,
            entries,
        )
        .expect("native document artifact")
    }

    #[test]
    fn compact_v1_streaming_bytes_match_the_accepted_golden_envelope() {
        const GOLDEN_HEX: &str = concat!(
            "434c524153534100010001000000000018000000736f6c7574696f6e2d7365742d",
            "61727469666163742e763111000000746573742d736f6c7574696f6e2d73657406",
            "0000006b65792d763107000000686173682d763106000000686173683a31010000",
            "0000000000000000000a000000736f6c7574696f6e2d61009b477530"
        );
        let encoded = CompactSolutionSetEncoder
            .encode(&golden_artifact())
            .expect("compact golden");
        assert_eq!(hex_bytes(encoded.bytes()), GOLDEN_HEX);
        assert_eq!(encoded.encoded_bytes(), 127);
        assert_eq!(encoded.checksum(), "crc32:3075479b");
    }

    #[test]
    fn json_v1_streaming_bytes_match_the_accepted_golden_envelope() {
        const GOLDEN: &str = concat!(
            "{\"schema\":\"solution-set-artifact.v1\",\"encoding\":\"json-v1\",",
            "\"compression\":\"none\",\"completeness\":\"complete\",",
            "\"comment_authority\":\"annotation-only\",",
            "\"source_solution_set_contract\":\"test-solution-set\",",
            "\"normalized_key_algorithm\":\"key-v1\",",
            "\"normalized_set_hash_algorithm\":\"hash-v1\",",
            "\"normalized_set_hash\":\"hash:1\",\"solution_count\":1,",
            "\"solutions\":[{\"key\":\"solution-a\",\"annotation\":{",
            "\"pc_probability\":null,\"average_score\":null},\"comment\":null}]}"
        );
        let encoded = JsonSolutionSetEncoder
            .encode(&golden_artifact())
            .expect("JSON golden");
        assert_eq!(encoded.bytes(), GOLDEN.as_bytes());
        assert_eq!(encoded.encoded_bytes(), 430);
        assert_eq!(encoded.checksum(), "crc32:9020be7b");
        assert_eq!(encoded.uncompressed_bytes(), encoded.encoded_bytes() as u64);
    }

    #[test]
    fn json_none_compression_plan_and_receipt_report_actual_byte_count() {
        let artifact = golden_artifact();
        let plan = JsonSolutionSetEncoder
            .measure_checked(&artifact, 4_096, &NeverCancelled)
            .expect("JSON plan");
        assert_eq!(plan.uncompressed_bytes(), plan.byte_count());

        let mut bytes = Vec::new();
        let receipt = JsonSolutionSetEncoder
            .encode_into(&artifact, &plan, &mut bytes, &NeverCancelled)
            .expect("JSON receipt");
        assert_eq!(receipt.uncompressed_bytes(), receipt.byte_count());
        assert_eq!(receipt.byte_count(), bytes.len() as u64);
    }

    #[test]
    fn checked_plan_matches_streaming_receipt_and_limit_fails_before_writing() {
        let artifact = golden_artifact();
        let plan = CompactSolutionSetEncoder
            .measure_checked(&artifact, 127, &NeverCancelled)
            .expect("exact plan");
        assert_eq!(plan.byte_count(), 127);
        let mut bytes = Vec::new();
        let receipt = CompactSolutionSetEncoder
            .encode_into(&artifact, &plan, &mut bytes, &NeverCancelled)
            .expect("streaming receipt");
        assert_eq!(receipt.as_plan(), plan);
        assert_eq!(bytes.len(), 127);

        assert_eq!(
            CompactSolutionSetEncoder.measure_checked(&artifact, 126, &NeverCancelled),
            Err(SolutionArtifactEncodingError::CapacityExceeded)
        );
    }

    #[test]
    fn checked_in_memory_encoding_honors_its_explicit_caller_limit() {
        let artifact = golden_artifact();
        let encoded = CompactSolutionSetEncoder
            .encode_checked(&artifact, 127, &NeverCancelled)
            .expect("exact checked in-memory encoding");
        assert_eq!(encoded.encoded_bytes(), 127);
        assert_eq!(
            CompactSolutionSetEncoder.encode_checked(&artifact, 126, &NeverCancelled),
            Err(SolutionArtifactEncodingError::CapacityExceeded)
        );
    }

    #[test]
    fn ctk3_large_solution_set_streams_as_an_ordered_two_segment_bundle() {
        let artifact = native_document_artifact(CTK3_MAX_SEGMENT_PAGES + 1);
        let plan = Ctk3SolutionSetEncoder
            .measure_checked(&artifact, DEFAULT_MAX_ARTIFACT_BYTES, &NeverCancelled)
            .expect("bounded CTK3 plan");
        let mut bytes = Vec::new();
        let receipt = Ctk3SolutionSetEncoder
            .encode_into(&artifact, &plan, &mut bytes, &NeverCancelled)
            .expect("segmented CTK3 stream");
        let text = str::from_utf8(&bytes).expect("CTK3 is ASCII");
        let info = clearra_ctk3::inspect_ctk3_exact(text).expect("exact native CTK3 bundle");

        assert!(text.starts_with(CTK3_BUNDLE_PREFIX));
        assert_eq!(info.segment_count, 2);
        assert_eq!(info.page_count, CTK3_MAX_SEGMENT_PAGES + 1);
        assert_eq!(receipt.as_plan(), plan);
    }

    #[test]
    fn ctk3_cancellation_between_segments_never_returns_a_complete_receipt() {
        let artifact = native_document_artifact(CTK3_MAX_SEGMENT_PAGES + 1);
        let plan = Ctk3SolutionSetEncoder
            .measure_checked(&artifact, DEFAULT_MAX_ARTIFACT_BYTES, &NeverCancelled)
            .expect("bounded CTK3 plan");
        let mut partial = Vec::new();
        let result = Ctk3SolutionSetEncoder.encode_into(
            &artifact,
            &plan,
            &mut partial,
            &CancelAt(PublicationCheckpoint::EncodingProgress {
                completed_units: CTK3_MAX_SEGMENT_PAGES as u64,
            }),
        );

        assert_eq!(result, Err(SolutionArtifactEncodingError::Cancelled));
        assert!(partial.starts_with(CTK3_BUNDLE_PREFIX.as_bytes()));
        assert!(!partial.is_empty());
        assert!((partial.len() as u64) < plan.byte_count());
    }

    #[test]
    fn native_document_rejections_keep_empty_key_and_fumen_limit_causes_distinct() {
        let empty = SolutionSetArtifact::try_new(
            "test-native-document-set",
            "test-native-key-v1",
            "test-native-hash-v1",
            "hash:empty",
            0,
            Vec::new(),
        )
        .expect("complete empty artifact");
        assert_eq!(
            Ctk3SolutionSetEncoder.measure_checked(
                &empty,
                DEFAULT_MAX_ARTIFACT_BYTES,
                &NeverCancelled,
            ),
            Err(SolutionArtifactEncodingError::EmptyDocument)
        );
        assert_eq!(
            Ctk3SolutionSetEncoder.measure_checked(
                &golden_artifact(),
                DEFAULT_MAX_ARTIFACT_BYTES,
                &NeverCancelled,
            ),
            Err(SolutionArtifactEncodingError::InvalidDocumentSolutionKey)
        );
        assert_eq!(
            FumenSolutionSetEncoder.measure_checked(
                &native_document_artifact(FUMEN_MAX_PAGES + 1),
                DEFAULT_MAX_ARTIFACT_BYTES,
                &NeverCancelled,
            ),
            Err(SolutionArtifactEncodingError::FumenPageLimitExceeded)
        );
    }

    #[test]
    fn adversarial_encoder_can_forge_same_length_bytes_under_an_honest_receipt() {
        let artifact = golden_artifact();
        let honest = CompactSolutionSetEncoder.encode(&artifact).expect("honest");
        let plan = CompactSolutionSetEncoder
            .measure_checked(&artifact, 127, &NeverCancelled)
            .expect("plan");
        let mut forged = Vec::new();
        let receipt = SameLengthForgedCompactEncoder
            .encode_into(&artifact, &plan, &mut forged, &NeverCancelled)
            .expect("forged encoder returns the honest receipt");

        assert_eq!(forged.len(), honest.bytes().len());
        assert_ne!(forged, honest.bytes());
        assert_eq!(receipt.byte_count(), honest.encoded_bytes() as u64);
        assert_eq!(receipt.checksum(), honest.checksum());
    }

    #[test]
    fn limit_ignoring_encoder_cannot_bypass_default_memory_bound() {
        let repeated = "x".repeat(MAX_ARTIFACT_KEY_BYTES - 3);
        let entries = (0..9)
            .map(|index| {
                SolutionArtifactEntry::try_new(
                    format!("{index:02}-{repeated}"),
                    SolutionArtifactAnnotation::new(),
                )
                .expect("maximum-length entry")
            })
            .collect::<Vec<_>>();
        let oversized = SolutionSetArtifact::try_new(
            "test-solution-set",
            "test-key-v1",
            "test-set-hash-v1",
            "hash:oversized",
            entries.len(),
            entries,
        )
        .expect("oversized JSON artifact model");

        let plan = LimitIgnoringEncoder
            .measure_checked(&oversized, 1, &NeverCancelled)
            .expect("malicious measurement ignores its caller limit");
        assert!(plan.byte_count() > MAX_IN_MEMORY_ARTIFACT_BYTES);
        assert_eq!(
            LimitIgnoringEncoder.encode(&oversized),
            Err(SolutionArtifactEncodingError::CapacityExceeded)
        );
    }

    #[test]
    fn sink_stream_verifier_accepts_exact_compact_and_json_golden_bytes() {
        let artifact = golden_artifact();
        for encoder in [
            &CompactSolutionSetEncoder as &dyn SolutionArtifactEncoder,
            &JsonSolutionSetEncoder as &dyn SolutionArtifactEncoder,
        ] {
            let plan = encoder
                .measure_checked(&artifact, 4_096, &NeverCancelled)
                .expect("plan");
            let mut bytes = Vec::new();
            let receipt = {
                let mut verifier =
                    ArtifactStreamVerifier::new(&mut bytes, encoder.encoding(), plan.byte_count());
                let receipt = encoder
                    .encode_into(&artifact, &plan, &mut verifier, &NeverCancelled)
                    .expect("encoded");
                verifier
                    .verify(&plan, &receipt)
                    .expect("independent byte verification");
                receipt
            };
            assert_eq!(receipt.byte_count(), bytes.len() as u64);
        }
    }

    #[derive(Default)]
    struct RecordingControl {
        checkpoints: RefCell<Vec<PublicationCheckpoint>>,
    }

    impl PublicationControl for RecordingControl {
        fn cancelled_at(&self, checkpoint: PublicationCheckpoint) -> bool {
            self.checkpoints.borrow_mut().push(checkpoint);
            false
        }
    }

    #[test]
    fn measurement_and_write_have_distinct_deterministic_cancellation_checkpoints() {
        let artifact = golden_artifact();
        let measurement = RecordingControl::default();
        let plan = CompactSolutionSetEncoder
            .measure_checked(&artifact, 127, &measurement)
            .expect("measurement");
        assert_eq!(
            measurement.checkpoints.into_inner(),
            vec![
                PublicationCheckpoint::BeforeMeasure,
                PublicationCheckpoint::MeasuringProgress { completed_units: 0 },
            ]
        );

        let writing = RecordingControl::default();
        let mut bytes = Vec::new();
        CompactSolutionSetEncoder
            .encode_into(&artifact, &plan, &mut bytes, &writing)
            .expect("write");
        assert_eq!(
            writing.checkpoints.into_inner(),
            vec![
                PublicationCheckpoint::BeforeEncoding,
                PublicationCheckpoint::EncodingProgress { completed_units: 0 },
            ]
        );
    }

    struct CancelAt(PublicationCheckpoint);

    impl PublicationControl for CancelAt {
        fn cancelled_at(&self, checkpoint: PublicationCheckpoint) -> bool {
            checkpoint == self.0
        }
    }

    #[test]
    fn cancellation_during_measurement_fails_before_any_output_plan_exists() {
        assert_eq!(
            CompactSolutionSetEncoder.measure_checked(
                &golden_artifact(),
                127,
                &CancelAt(PublicationCheckpoint::MeasuringProgress { completed_units: 0 }),
            ),
            Err(SolutionArtifactEncodingError::Cancelled)
        );
    }

    fn hex_bytes(bytes: &[u8]) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}
