use core::cmp::Ordering;

use crate::{
    manifest::{
        ActivatedSnapshot, Pc4ArtifactRole, Pc4ProfileManifest, Pc4RuleProfile, SnapshotIdentity,
        FIELD_HASH_INDEX_MAGIC, FIELD_HASH_RECORD_BYTES, GRAPH_OFFSETS_MAGIC, GRAPH_OFFSET_BYTES,
        INDEX_HEADER_BYTES, RANGE_INDEX_VERSION,
    },
    protocol::{RangeRequest, RangeResponse, RangeResponseKind, RangeTransportFailure},
    GraphTargetEncoding,
};

const MAX_FIELD_HASH: u64 = (1_u64 << 40) - 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LookupHit {
    pub snapshot: SnapshotIdentity,
    pub profile: Pc4RuleProfile,
    pub field_id: u32,
    pub graph_target_encoding: GraphTargetEncoding,
    /// Opaque graph-record bytes. A separately qualified materializer owns the
    /// upstream record layout and placement semantics.
    pub graph_record: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupFailure {
    Offline,
    RateLimited { retry_after_seconds: Option<u64> },
    DatasetUnavailable,
    FormatMismatch(FormatMismatch),
}

impl LookupFailure {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Offline => "pc4_online_offline",
            Self::RateLimited { .. } => "pc4_online_rate_limited",
            Self::DatasetUnavailable => "pc4_online_dataset_unavailable",
            Self::FormatMismatch(mismatch) => mismatch.reason(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatMismatch {
    HeaderMagic {
        artifact: Pc4ArtifactRole,
    },
    HeaderVersion {
        artifact: Pc4ArtifactRole,
        actual: u32,
    },
    HeaderFieldCount {
        artifact: Pc4ArtifactRole,
        actual: u32,
    },
    FieldIdOutsideDomain {
        field_id: u32,
        field_count: u32,
    },
    GraphOffsetsDescending {
        start: u32,
        end: u32,
    },
    GraphOffsetOutsideArtifact {
        offset: u32,
        graph_bytes: u64,
    },
    GraphRecordTooLarge {
        bytes: u64,
        maximum: u32,
    },
}

impl FormatMismatch {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::HeaderMagic { .. } => "pc4_online_index_magic_mismatch",
            Self::HeaderVersion { .. } => "pc4_online_index_version_mismatch",
            Self::HeaderFieldCount { .. } => "pc4_online_index_field_count_mismatch",
            Self::FieldIdOutsideDomain { .. } => "pc4_online_field_id_outside_domain",
            Self::GraphOffsetsDescending { .. } => "pc4_online_graph_offsets_descending",
            Self::GraphOffsetOutsideArtifact { .. } => "pc4_online_graph_offset_outside_artifact",
            Self::GraphRecordTooLarge { .. } => "pc4_online_graph_record_too_large",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupStartError {
    FieldHashOutsidePc4Domain { field_hash: u64 },
}

impl LookupStartError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::FieldHashOutsidePc4Domain { .. } => "pc4_online_field_hash_outside_pc4_domain",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupplyError {
    Terminal,
    StaleRequest { expected: u64, actual: u64 },
    SnapshotMismatch,
    ProfileMismatch,
    ArtifactMismatch,
    WholeContentRejected,
    ContentRangeMismatch,
    CompleteLengthMismatch { expected: u64, actual: u64 },
    BodyLengthMismatch { expected: u32, actual: usize },
}

impl SupplyError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Terminal => "pc4_online_lookup_terminal",
            Self::StaleRequest { .. } => "pc4_online_stale_range_response",
            Self::SnapshotMismatch => "pc4_online_snapshot_mismatch",
            Self::ProfileMismatch => "pc4_online_profile_mismatch",
            Self::ArtifactMismatch => "pc4_online_artifact_mismatch",
            Self::WholeContentRejected => "pc4_online_whole_content_rejected",
            Self::ContentRangeMismatch => "pc4_online_content_range_mismatch",
            Self::CompleteLengthMismatch { .. } => "pc4_online_complete_length_mismatch",
            Self::BodyLengthMismatch { .. } => "pc4_online_body_length_mismatch",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LookupStep {
    NeedRange(RangeRequest),
    Hit(LookupHit),
    Miss,
    Failed(LookupFailure),
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct LookupMachine {
    snapshot: SnapshotIdentity,
    profile: Pc4ProfileManifest,
    field_hash: u64,
    next_request_id: u64,
    phase: Phase,
    pending: Option<RangeRequest>,
    terminal: Option<Terminal>,
}

#[derive(Clone, Debug)]
enum Phase {
    FieldIndexHeader,
    FieldIndexRecord { low: u32, high: u32, index: u32 },
    OffsetIndexHeader { field_id: u32 },
    OffsetPair { field_id: u32 },
    GraphRecord { field_id: u32 },
}

#[derive(Clone, Debug)]
enum Terminal {
    Hit(LookupHit),
    Miss,
    Failed(LookupFailure),
    Cancelled,
}

impl LookupMachine {
    pub fn start(
        snapshot: &ActivatedSnapshot,
        profile: Pc4RuleProfile,
        field_hash: u64,
    ) -> Result<Self, LookupStartError> {
        if field_hash > MAX_FIELD_HASH {
            return Err(LookupStartError::FieldHashOutsidePc4Domain { field_hash });
        }
        let mut machine = Self {
            snapshot: snapshot.identity().clone(),
            profile: snapshot.profile(profile).clone(),
            field_hash,
            next_request_id: 1,
            phase: Phase::FieldIndexHeader,
            pending: None,
            terminal: None,
        };
        machine.request(
            Pc4ArtifactRole::FieldHashIndex,
            0,
            INDEX_HEADER_BYTES as u32,
        );
        Ok(machine)
    }

    pub fn step(&self) -> LookupStep {
        if let Some(terminal) = &self.terminal {
            return match terminal {
                Terminal::Hit(hit) => LookupStep::Hit(hit.clone()),
                Terminal::Miss => LookupStep::Miss,
                Terminal::Failed(failure) => LookupStep::Failed(failure.clone()),
                Terminal::Cancelled => LookupStep::Cancelled,
            };
        }
        LookupStep::NeedRange(
            self.pending
                .as_ref()
                .expect("non-terminal lookup always owns one pending Range request")
                .clone(),
        )
    }

    pub fn supply(&mut self, response: RangeResponse) -> Result<(), SupplyError> {
        let request = self.pending.as_ref().ok_or(SupplyError::Terminal)?;
        validate_response(request, &response)?;
        let bytes = response.bytes;
        self.pending = None;
        match self.phase.clone() {
            Phase::FieldIndexHeader => self.consume_field_index_header(&bytes),
            Phase::FieldIndexRecord { low, high, index } => {
                self.consume_field_index_record(&bytes, low, high, index)
            }
            Phase::OffsetIndexHeader { field_id } => {
                self.consume_offset_index_header(&bytes, field_id)
            }
            Phase::OffsetPair { field_id } => self.consume_offset_pair(&bytes, field_id),
            Phase::GraphRecord { field_id } => {
                self.terminal = Some(Terminal::Hit(LookupHit {
                    snapshot: self.snapshot.clone(),
                    profile: self.profile.profile(),
                    field_id,
                    graph_target_encoding: self.profile.graph_target_encoding(),
                    graph_record: bytes,
                }));
            }
        }
        Ok(())
    }

    pub fn reject_range(
        &mut self,
        request_id: u64,
        failure: RangeTransportFailure,
    ) -> Result<(), SupplyError> {
        let request = self.pending.as_ref().ok_or(SupplyError::Terminal)?;
        if request.request_id() != request_id {
            return Err(SupplyError::StaleRequest {
                expected: request.request_id(),
                actual: request_id,
            });
        }
        self.pending = None;
        self.terminal = Some(Terminal::Failed(match failure {
            RangeTransportFailure::Offline => LookupFailure::Offline,
            RangeTransportFailure::RateLimited {
                retry_after_seconds,
            } => LookupFailure::RateLimited {
                retry_after_seconds,
            },
            RangeTransportFailure::Timeout | RangeTransportFailure::Unavailable => {
                LookupFailure::DatasetUnavailable
            }
        }));
        Ok(())
    }

    pub fn cancel(&mut self) {
        if self.terminal.is_none() {
            self.pending = None;
            self.terminal = Some(Terminal::Cancelled);
        }
    }

    fn consume_field_index_header(&mut self, bytes: &[u8]) {
        if let Err(error) = validate_index_header(
            bytes,
            FIELD_HASH_INDEX_MAGIC,
            self.profile.field_count(),
            Pc4ArtifactRole::FieldHashIndex,
        ) {
            self.fail_format(error);
            return;
        }
        self.request_field_record(0, self.profile.field_count());
    }

    fn request_field_record(&mut self, low: u32, high: u32) {
        if low >= high {
            self.terminal = Some(Terminal::Miss);
            return;
        }
        let index = low + (high - low) / 2;
        self.phase = Phase::FieldIndexRecord { low, high, index };
        self.request(
            Pc4ArtifactRole::FieldHashIndex,
            INDEX_HEADER_BYTES + u64::from(index) * FIELD_HASH_RECORD_BYTES,
            FIELD_HASH_RECORD_BYTES as u32,
        );
    }

    fn consume_field_index_record(&mut self, bytes: &[u8], low: u32, high: u32, index: u32) {
        let candidate_hash = read_u40(bytes, 0);
        let field_id = read_u24(bytes, 5);
        if field_id >= self.profile.field_count() {
            self.fail_format(FormatMismatch::FieldIdOutsideDomain {
                field_id,
                field_count: self.profile.field_count(),
            });
            return;
        }
        match candidate_hash.cmp(&self.field_hash) {
            Ordering::Equal => {
                self.phase = Phase::OffsetIndexHeader { field_id };
                self.request(Pc4ArtifactRole::GraphOffsets, 0, INDEX_HEADER_BYTES as u32);
            }
            Ordering::Less => self.request_field_record(index + 1, high),
            Ordering::Greater => self.request_field_record(low, index),
        }
    }

    fn consume_offset_index_header(&mut self, bytes: &[u8], field_id: u32) {
        if let Err(error) = validate_index_header(
            bytes,
            GRAPH_OFFSETS_MAGIC,
            self.profile.field_count(),
            Pc4ArtifactRole::GraphOffsets,
        ) {
            self.fail_format(error);
            return;
        }
        self.phase = Phase::OffsetPair { field_id };
        self.request(
            Pc4ArtifactRole::GraphOffsets,
            INDEX_HEADER_BYTES + u64::from(field_id) * GRAPH_OFFSET_BYTES,
            (2 * GRAPH_OFFSET_BYTES) as u32,
        );
    }

    fn consume_offset_pair(&mut self, bytes: &[u8], field_id: u32) {
        let start = read_u32(bytes, 0);
        let end = read_u32(bytes, 4);
        if end < start {
            self.fail_format(FormatMismatch::GraphOffsetsDescending { start, end });
            return;
        }
        let graph_bytes = self.profile.graph().byte_len();
        if u64::from(end) > graph_bytes {
            self.fail_format(FormatMismatch::GraphOffsetOutsideArtifact {
                offset: end,
                graph_bytes,
            });
            return;
        }
        let record_bytes = u64::from(end - start);
        if record_bytes > u64::from(self.profile.maximum_graph_record_bytes()) {
            self.fail_format(FormatMismatch::GraphRecordTooLarge {
                bytes: record_bytes,
                maximum: self.profile.maximum_graph_record_bytes(),
            });
            return;
        }
        if record_bytes == 0 {
            self.terminal = Some(Terminal::Hit(LookupHit {
                snapshot: self.snapshot.clone(),
                profile: self.profile.profile(),
                field_id,
                graph_target_encoding: self.profile.graph_target_encoding(),
                graph_record: Vec::new(),
            }));
            return;
        }
        self.phase = Phase::GraphRecord { field_id };
        self.request(
            Pc4ArtifactRole::Graph,
            u64::from(start),
            record_bytes as u32,
        );
    }

    fn request(&mut self, artifact: Pc4ArtifactRole, offset: u64, length: u32) {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        self.pending = Some(RangeRequest::new(
            request_id,
            self.snapshot.clone(),
            self.profile.profile(),
            self.profile.artifact(artifact).clone(),
            offset,
            length,
        ));
    }

    fn fail_format(&mut self, mismatch: FormatMismatch) {
        self.pending = None;
        self.terminal = Some(Terminal::Failed(LookupFailure::FormatMismatch(mismatch)));
    }
}

fn validate_response(request: &RangeRequest, response: &RangeResponse) -> Result<(), SupplyError> {
    if response.request_id != request.request_id() {
        return Err(SupplyError::StaleRequest {
            expected: request.request_id(),
            actual: response.request_id,
        });
    }
    if response.snapshot != *request.snapshot() {
        return Err(SupplyError::SnapshotMismatch);
    }
    if response.profile != request.profile() {
        return Err(SupplyError::ProfileMismatch);
    }
    if response.artifact != request.artifact() {
        return Err(SupplyError::ArtifactMismatch);
    }
    if response.artifact_content_identity != request.artifact_descriptor().content_identity() {
        return Err(SupplyError::ArtifactMismatch);
    }
    if response.kind != RangeResponseKind::PartialContent {
        return Err(SupplyError::WholeContentRejected);
    }
    if response.offset != request.offset() {
        return Err(SupplyError::ContentRangeMismatch);
    }
    if response.complete_length != request.artifact_descriptor().byte_len() {
        return Err(SupplyError::CompleteLengthMismatch {
            expected: request.artifact_descriptor().byte_len(),
            actual: response.complete_length,
        });
    }
    if response.bytes.len() != request.length() as usize {
        return Err(SupplyError::BodyLengthMismatch {
            expected: request.length(),
            actual: response.bytes.len(),
        });
    }
    if request.end_exclusive() > response.complete_length {
        return Err(SupplyError::ContentRangeMismatch);
    }
    Ok(())
}

fn validate_index_header(
    bytes: &[u8],
    expected_magic: [u8; 8],
    expected_count: u32,
    artifact: Pc4ArtifactRole,
) -> Result<(), FormatMismatch> {
    if bytes[..8] != expected_magic {
        return Err(FormatMismatch::HeaderMagic { artifact });
    }
    let version = read_u32(bytes, 8);
    if version != RANGE_INDEX_VERSION {
        return Err(FormatMismatch::HeaderVersion {
            artifact,
            actual: version,
        });
    }
    let count = read_u32(bytes, 12);
    if count != expected_count {
        return Err(FormatMismatch::HeaderFieldCount {
            artifact,
            actual: count,
        });
    }
    Ok(())
}

fn read_u24(bytes: &[u8], offset: usize) -> u32 {
    u32::from(bytes[offset])
        | (u32::from(bytes[offset + 1]) << 8)
        | (u32::from(bytes[offset + 2]) << 16)
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u40(bytes: &[u8], offset: usize) -> u64 {
    u64::from(bytes[offset])
        | (u64::from(bytes[offset + 1]) << 8)
        | (u64::from(bytes[offset + 2]) << 16)
        | (u64::from(bytes[offset + 3]) << 24)
        | (u64::from(bytes[offset + 4]) << 32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::activated_snapshot;

    const HASHES: [u64; 3] = [0, 15, 30];
    const FIELD_IDS: [u32; 3] = [2, 0, 1];
    const GRAPH: [u8; 9] = [10, 11, 12, 20, 21, 30, 31, 32, 33];

    fn field_index() -> Vec<u8> {
        let mut bytes = index_header(FIELD_HASH_INDEX_MAGIC, HASHES.len() as u32);
        for (hash, field_id) in HASHES.into_iter().zip(FIELD_IDS) {
            bytes.extend_from_slice(&hash.to_le_bytes()[..5]);
            bytes.extend_from_slice(&field_id.to_le_bytes()[..3]);
        }
        bytes
    }

    fn graph_offsets() -> Vec<u8> {
        let mut bytes = index_header(GRAPH_OFFSETS_MAGIC, HASHES.len() as u32);
        for offset in [0_u32, 3, 5, 9] {
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes
    }

    fn index_header(magic: [u8; 8], count: u32) -> Vec<u8> {
        let mut bytes = magic.to_vec();
        bytes.extend_from_slice(&RANGE_INDEX_VERSION.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes
    }

    fn respond(machine: &mut LookupMachine, request: RangeRequest, source: &[u8]) {
        let begin = request.offset() as usize;
        let end = request.end_exclusive() as usize;
        machine
            .supply(RangeResponse {
                request_id: request.request_id(),
                snapshot: request.snapshot().clone(),
                profile: request.profile(),
                artifact: request.artifact(),
                artifact_content_identity: request
                    .artifact_descriptor()
                    .content_identity()
                    .to_owned(),
                kind: RangeResponseKind::PartialContent,
                offset: request.offset(),
                complete_length: source.len() as u64,
                bytes: source[begin..end].to_vec(),
            })
            .expect("synthetic Range response");
    }

    fn drive(field_hash: u64, profile: Pc4RuleProfile) -> LookupStep {
        let snapshot = activated_snapshot(HASHES.len() as u32, GRAPH.len() as u64);
        let mut machine = LookupMachine::start(&snapshot, profile, field_hash).expect("lookup");
        let field_index = field_index();
        let graph_offsets = graph_offsets();
        loop {
            match machine.step() {
                LookupStep::NeedRange(request) => {
                    let source: &[u8] = match request.artifact() {
                        Pc4ArtifactRole::FieldHashIndex => &field_index,
                        Pc4ArtifactRole::GraphOffsets => &graph_offsets,
                        Pc4ArtifactRole::Graph => &GRAPH,
                    };
                    respond(&mut machine, request, source);
                }
                terminal => return terminal,
            }
        }
    }

    #[test]
    fn known_answer_lookup_uses_raw_byte_offsets_for_u24_and_u32_profiles() {
        assert_eq!(
            drive(15, Pc4RuleProfile::Srs),
            LookupStep::Hit(LookupHit {
                snapshot: SnapshotIdentity::new(
                    "synthetic/repository",
                    "immutable-revision-a",
                    "generation-a"
                )
                .expect("identity"),
                profile: Pc4RuleProfile::Srs,
                field_id: 0,
                graph_target_encoding: GraphTargetEncoding::U24LittleEndian,
                graph_record: vec![10, 11, 12],
            })
        );
        assert_eq!(
            drive(30, Pc4RuleProfile::SrsX),
            LookupStep::Hit(LookupHit {
                snapshot: SnapshotIdentity::new(
                    "synthetic/repository",
                    "immutable-revision-a",
                    "generation-a"
                )
                .expect("identity"),
                profile: Pc4RuleProfile::SrsX,
                field_id: 1,
                graph_target_encoding: GraphTargetEncoding::U32LittleEndian,
                graph_record: vec![20, 21],
            })
        );
    }

    #[test]
    fn absent_field_is_a_miss_not_a_dataset_failure() {
        assert_eq!(drive(16, Pc4RuleProfile::NoKick), LookupStep::Miss);
    }

    #[test]
    fn stale_snapshot_short_body_and_whole_body_do_not_advance_machine() {
        let snapshot = activated_snapshot(HASHES.len() as u32, GRAPH.len() as u64);
        let mut machine = LookupMachine::start(&snapshot, Pc4RuleProfile::Srs, 15).expect("lookup");
        let LookupStep::NeedRange(request) = machine.step() else {
            panic!("initial Range request")
        };
        let source = field_index();
        let base = RangeResponse {
            request_id: request.request_id(),
            snapshot: request.snapshot().clone(),
            profile: request.profile(),
            artifact: request.artifact(),
            artifact_content_identity: request.artifact_descriptor().content_identity().to_owned(),
            kind: RangeResponseKind::PartialContent,
            offset: request.offset(),
            complete_length: source.len() as u64,
            bytes: source[..request.length() as usize].to_vec(),
        };

        let mut stale = base.clone();
        stale.snapshot = SnapshotIdentity::new("repository", "other-revision", "other-generation")
            .expect("stale identity");
        assert_eq!(machine.supply(stale), Err(SupplyError::SnapshotMismatch));

        let mut whole = base.clone();
        whole.kind = RangeResponseKind::WholeContent;
        assert_eq!(
            machine.supply(whole),
            Err(SupplyError::WholeContentRejected)
        );

        let mut wrong_artifact_identity = base.clone();
        wrong_artifact_identity.artifact_content_identity = "different-oid".to_owned();
        assert_eq!(
            machine.supply(wrong_artifact_identity),
            Err(SupplyError::ArtifactMismatch)
        );

        let mut short = base.clone();
        short.bytes.pop();
        assert_eq!(
            machine.supply(short),
            Err(SupplyError::BodyLengthMismatch {
                expected: request.length(),
                actual: request.length() as usize - 1,
            })
        );
        assert_eq!(machine.step(), LookupStep::NeedRange(request));
    }

    #[test]
    fn transport_failures_remain_distinct_and_terminal() {
        let snapshot = activated_snapshot(HASHES.len() as u32, GRAPH.len() as u64);
        let mut offline = LookupMachine::start(&snapshot, Pc4RuleProfile::Srs, 15).expect("lookup");
        let LookupStep::NeedRange(request) = offline.step() else {
            panic!("request")
        };
        offline
            .reject_range(request.request_id(), RangeTransportFailure::Offline)
            .expect("offline rejection");
        assert_eq!(offline.step(), LookupStep::Failed(LookupFailure::Offline));

        let mut limited = LookupMachine::start(&snapshot, Pc4RuleProfile::Srs, 15).expect("lookup");
        let LookupStep::NeedRange(request) = limited.step() else {
            panic!("request")
        };
        limited
            .reject_range(
                request.request_id(),
                RangeTransportFailure::RateLimited {
                    retry_after_seconds: Some(30),
                },
            )
            .expect("rate-limit rejection");
        assert_eq!(
            limited.step(),
            LookupStep::Failed(LookupFailure::RateLimited {
                retry_after_seconds: Some(30),
            })
        );
    }

    #[test]
    fn malformed_index_header_fails_closed() {
        let snapshot = activated_snapshot(HASHES.len() as u32, GRAPH.len() as u64);
        let mut machine = LookupMachine::start(&snapshot, Pc4RuleProfile::Srs, 15).expect("lookup");
        let LookupStep::NeedRange(request) = machine.step() else {
            panic!("request")
        };
        let mut invalid = field_index();
        invalid[0] ^= 1;
        respond(&mut machine, request, &invalid);
        assert_eq!(
            machine.step(),
            LookupStep::Failed(LookupFailure::FormatMismatch(FormatMismatch::HeaderMagic {
                artifact: Pc4ArtifactRole::FieldHashIndex,
            }))
        );
    }

    #[test]
    fn cancellation_is_terminal_and_rejects_late_bytes() {
        let snapshot = activated_snapshot(HASHES.len() as u32, GRAPH.len() as u64);
        let mut machine = LookupMachine::start(&snapshot, Pc4RuleProfile::Srs, 15).expect("lookup");
        let LookupStep::NeedRange(request) = machine.step() else {
            panic!("request")
        };
        machine.cancel();
        assert_eq!(machine.step(), LookupStep::Cancelled);
        let bytes = field_index();
        assert_eq!(
            machine.supply(RangeResponse {
                request_id: request.request_id(),
                snapshot: request.snapshot().clone(),
                profile: request.profile(),
                artifact: request.artifact(),
                artifact_content_identity: request
                    .artifact_descriptor()
                    .content_identity()
                    .to_owned(),
                kind: RangeResponseKind::PartialContent,
                offset: request.offset(),
                complete_length: bytes.len() as u64,
                bytes: bytes[..request.length() as usize].to_vec(),
            }),
            Err(SupplyError::Terminal)
        );
    }
}
