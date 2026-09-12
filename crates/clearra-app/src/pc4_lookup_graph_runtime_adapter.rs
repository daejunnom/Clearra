// SRP rationale: this module owns only the generation-bound in-memory bridge
// from qualified lookup records to complete adjacency and exact one-edge
// placement materialization. Range transport, lookup paging, traversal,
// terminal decisions, reducers, fallback, and product activation stay outside.

use core::{fmt, mem::size_of, num::NonZeroUsize};

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_core_executor::{
    materialize_pc4_ilc_transition, Pc4IlcMaterializationError, Pc4IlcPlacement,
};
use clearra_pc4_tablebase::{
    decode_hydra_graph_record_v1, hydra_field_hash_v1_to_clearra_board64_mask, ActivatedSnapshot,
    ClearraPlacementIdentity, DecodedHydraGraphRecordV1, FixedQueueAdjacencyQuery,
    GraphTargetEncoding, HydraFieldHashOutsideDomain, HydraGraphRecordDecodeError, LookupHit,
    MaterializationOutput, Pc4GraphPiece, Pc4PlacementMaterializer, Pc4RuleProfile,
    PlacementIdentityError, PlacementRotation, QualifiedCompleteAdjacency,
    QualifiedCompleteAdjacencyProvider, QualifiedPc4GraphEdge, QualifiedPc4TargetIdentity,
};
use clearra_rules::kicks::KickTableProfileId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4LookupGraphCacheLimits {
    max_records: NonZeroUsize,
    max_encoded_graph_bytes: NonZeroUsize,
    max_decoded_target_bytes: NonZeroUsize,
}

impl Pc4LookupGraphCacheLimits {
    pub const fn new(
        max_records: NonZeroUsize,
        max_encoded_graph_bytes: NonZeroUsize,
        max_decoded_target_bytes: NonZeroUsize,
    ) -> Self {
        Self {
            max_records,
            max_encoded_graph_bytes,
            max_decoded_target_bytes,
        }
    }

    pub const fn max_records(self) -> usize {
        self.max_records.get()
    }

    pub const fn max_encoded_graph_bytes(self) -> usize {
        self.max_encoded_graph_bytes.get()
    }

    pub const fn max_decoded_target_bytes(self) -> usize {
        self.max_decoded_target_bytes.get()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Pc4LookupGraphCacheUsage {
    record_count: usize,
    encoded_graph_bytes: usize,
    decoded_target_count: usize,
    decoded_target_bytes: usize,
}

impl Pc4LookupGraphCacheUsage {
    pub const fn record_count(self) -> usize {
        self.record_count
    }

    pub const fn encoded_graph_bytes(self) -> usize {
        self.encoded_graph_bytes
    }

    pub const fn decoded_target_count(self) -> usize {
        self.decoded_target_count
    }

    pub const fn decoded_target_bytes(self) -> usize {
        self.decoded_target_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4LookupGraphCacheBudgetKind {
    Records,
    EncodedGraphBytes,
    DecodedTargetBytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4LookupGraphCacheStartError {
    TargetSnapshotMismatch,
}

impl Pc4LookupGraphCacheStartError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::TargetSnapshotMismatch => "pc4_lookup_graph_cache_target_snapshot_mismatch",
        }
    }
}

impl fmt::Display for Pc4LookupGraphCacheStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4LookupGraphCacheStartError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4LookupGraphCacheError {
    TargetMismatch,
    LookupSnapshotMismatch,
    LookupProfileMismatch {
        expected: Pc4RuleProfile,
        actual: Pc4RuleProfile,
    },
    LookupTargetEncodingMismatch {
        expected: GraphTargetEncoding,
        actual: GraphTargetEncoding,
    },
    FieldIdOutsideDomain {
        field_id: u32,
        field_count: u32,
    },
    GraphRecordTooLarge {
        maximum: u32,
        actual: usize,
    },
    GraphRecordDecode(HydraGraphRecordDecodeError),
    ConflictingRecord {
        field_id: u32,
    },
    FieldHashMappedToDifferentId {
        field_hash: u64,
        existing_field_id: u32,
        actual_field_id: u32,
    },
    BudgetExceeded {
        kind: Pc4LookupGraphCacheBudgetKind,
        limit: usize,
        attempted: usize,
    },
    AccountingOverflow,
    AllocationFailed,
}

impl Pc4LookupGraphCacheError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::TargetMismatch => "pc4_lookup_graph_cache_target_mismatch",
            Self::LookupSnapshotMismatch => "pc4_lookup_graph_cache_snapshot_mismatch",
            Self::LookupProfileMismatch { .. } => "pc4_lookup_graph_cache_profile_mismatch",
            Self::LookupTargetEncodingMismatch { .. } => {
                "pc4_lookup_graph_cache_target_encoding_mismatch"
            }
            Self::FieldIdOutsideDomain { .. } => "pc4_lookup_graph_cache_field_id_outside_domain",
            Self::GraphRecordTooLarge { .. } => "pc4_lookup_graph_cache_record_too_large",
            Self::GraphRecordDecode(error) => error.reason(),
            Self::ConflictingRecord { .. } => "pc4_lookup_graph_cache_record_drift",
            Self::FieldHashMappedToDifferentId { .. } => {
                "pc4_lookup_graph_cache_field_identity_drift"
            }
            Self::BudgetExceeded { .. } => "pc4_lookup_graph_cache_budget_exceeded",
            Self::AccountingOverflow => "pc4_lookup_graph_cache_accounting_overflow",
            Self::AllocationFailed => "pc4_lookup_graph_cache_allocation_failed",
        }
    }
}

impl fmt::Display for Pc4LookupGraphCacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4LookupGraphCacheError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4LookupGraphCacheAdmission {
    AlreadyPresent,
    Inserted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Pc4LookupGraphCacheEntry {
    field_id: u32,
    field_hash: u64,
    encoded_record: Vec<u8>,
    decoded_record: DecodedHydraGraphRecordV1,
}

/// Fixed-budget record cache bound to one activated generation and one
/// independently qualified product target.
///
/// New records are admitted atomically. Exact repeated records are
/// idempotent, while a changed hash or byte body under an existing field
/// identity fails closed. Records are retained until the owner drops the
/// cache; exhausting any budget never evicts a record needed by an in-flight
/// transactional traversal page.
#[derive(Clone, Debug)]
pub struct Pc4LookupGraphCache {
    snapshot: ActivatedSnapshot,
    target: QualifiedPc4TargetIdentity,
    target_encoding: GraphTargetEncoding,
    field_count: u32,
    maximum_graph_record_bytes: u32,
    limits: Pc4LookupGraphCacheLimits,
    usage: Pc4LookupGraphCacheUsage,
    entries: Vec<Pc4LookupGraphCacheEntry>,
}

impl Pc4LookupGraphCache {
    pub fn new(
        snapshot: &ActivatedSnapshot,
        target: QualifiedPc4TargetIdentity,
        limits: Pc4LookupGraphCacheLimits,
    ) -> Result<Self, Pc4LookupGraphCacheStartError> {
        if target.snapshot() != snapshot.qualified_identity() {
            return Err(Pc4LookupGraphCacheStartError::TargetSnapshotMismatch);
        }
        let profile = snapshot.profile(target.profile());
        Ok(Self {
            snapshot: snapshot.clone(),
            target,
            target_encoding: profile.graph_target_encoding(),
            field_count: profile.field_count(),
            maximum_graph_record_bytes: profile.maximum_graph_record_bytes(),
            limits,
            usage: Pc4LookupGraphCacheUsage::default(),
            entries: Vec::new(),
        })
    }

    pub const fn activated_snapshot(&self) -> &ActivatedSnapshot {
        &self.snapshot
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.target.profile()
    }

    pub const fn target_encoding(&self) -> GraphTargetEncoding {
        self.target_encoding
    }

    pub const fn field_count(&self) -> u32 {
        self.field_count
    }

    pub const fn limits(&self) -> Pc4LookupGraphCacheLimits {
        self.limits
    }

    pub const fn usage(&self) -> Pc4LookupGraphCacheUsage {
        self.usage
    }

    pub fn contains_field_id(&self, field_id: u32) -> bool {
        self.entry(field_id).is_some()
    }

    /// Admits one lookup hit under the exact target that authorized its
    /// lookup session. The session identity is intentionally not cached, so
    /// an identical immutable record fetched by a later session remains an
    /// idempotent hit.
    pub fn admit(
        &mut self,
        lookup_target: &QualifiedPc4TargetIdentity,
        hit: LookupHit,
    ) -> Result<Pc4LookupGraphCacheAdmission, Pc4LookupGraphCacheError> {
        self.validate_lookup_binding(lookup_target, &hit)?;

        if let Some(existing) = self.entry(hit.field_id) {
            return if existing.field_hash == hit.field_hash
                && existing.encoded_record == hit.graph_record
            {
                Ok(Pc4LookupGraphCacheAdmission::AlreadyPresent)
            } else {
                Err(Pc4LookupGraphCacheError::ConflictingRecord {
                    field_id: hit.field_id,
                })
            };
        }
        if let Some(existing) = self
            .entries
            .iter()
            .find(|entry| entry.field_hash == hit.field_hash)
        {
            return Err(Pc4LookupGraphCacheError::FieldHashMappedToDifferentId {
                field_hash: hit.field_hash,
                existing_field_id: existing.field_id,
                actual_field_id: hit.field_id,
            });
        }

        let decoded_record = decode_hydra_graph_record_v1(
            &hit.graph_record,
            hit.field_hash,
            self.target_encoding,
            self.field_count,
        )
        .map_err(Pc4LookupGraphCacheError::GraphRecordDecode)?;
        let encoded_graph_bytes = hit.graph_record.len();
        let decoded_target_count = decoded_record.total_target_count();
        let decoded_target_bytes = decoded_target_count
            .checked_mul(size_of::<u32>())
            .ok_or(Pc4LookupGraphCacheError::AccountingOverflow)?;
        let next_usage = Pc4LookupGraphCacheUsage {
            record_count: self
                .usage
                .record_count
                .checked_add(1)
                .ok_or(Pc4LookupGraphCacheError::AccountingOverflow)?,
            encoded_graph_bytes: self
                .usage
                .encoded_graph_bytes
                .checked_add(encoded_graph_bytes)
                .ok_or(Pc4LookupGraphCacheError::AccountingOverflow)?,
            decoded_target_count: self
                .usage
                .decoded_target_count
                .checked_add(decoded_target_count)
                .ok_or(Pc4LookupGraphCacheError::AccountingOverflow)?,
            decoded_target_bytes: self
                .usage
                .decoded_target_bytes
                .checked_add(decoded_target_bytes)
                .ok_or(Pc4LookupGraphCacheError::AccountingOverflow)?,
        };
        check_cache_budget(
            Pc4LookupGraphCacheBudgetKind::Records,
            self.limits.max_records(),
            next_usage.record_count,
        )?;
        check_cache_budget(
            Pc4LookupGraphCacheBudgetKind::EncodedGraphBytes,
            self.limits.max_encoded_graph_bytes(),
            next_usage.encoded_graph_bytes,
        )?;
        check_cache_budget(
            Pc4LookupGraphCacheBudgetKind::DecodedTargetBytes,
            self.limits.max_decoded_target_bytes(),
            next_usage.decoded_target_bytes,
        )?;

        self.entries
            .try_reserve(1)
            .map_err(|_| Pc4LookupGraphCacheError::AllocationFailed)?;
        self.entries.push(Pc4LookupGraphCacheEntry {
            field_id: hit.field_id,
            field_hash: hit.field_hash,
            encoded_record: hit.graph_record,
            decoded_record,
        });
        self.usage = next_usage;
        Ok(Pc4LookupGraphCacheAdmission::Inserted)
    }

    pub const fn adjacency_provider(&self) -> Pc4LookupCompleteAdjacencyProvider<'_> {
        Pc4LookupCompleteAdjacencyProvider { cache: self }
    }

    pub const fn placement_materializer(&self) -> Pc4LookupPlacementMaterializer<'_> {
        Pc4LookupPlacementMaterializer { cache: self }
    }

    fn validate_lookup_binding(
        &self,
        lookup_target: &QualifiedPc4TargetIdentity,
        hit: &LookupHit,
    ) -> Result<(), Pc4LookupGraphCacheError> {
        if lookup_target != &self.target {
            return Err(Pc4LookupGraphCacheError::TargetMismatch);
        }
        if hit.snapshot != *self.target.snapshot() {
            return Err(Pc4LookupGraphCacheError::LookupSnapshotMismatch);
        }
        if hit.profile != self.profile() {
            return Err(Pc4LookupGraphCacheError::LookupProfileMismatch {
                expected: self.profile(),
                actual: hit.profile,
            });
        }
        if hit.graph_target_encoding != self.target_encoding {
            return Err(Pc4LookupGraphCacheError::LookupTargetEncodingMismatch {
                expected: self.target_encoding,
                actual: hit.graph_target_encoding,
            });
        }
        if hit.field_id >= self.field_count {
            return Err(Pc4LookupGraphCacheError::FieldIdOutsideDomain {
                field_id: hit.field_id,
                field_count: self.field_count,
            });
        }
        if hit.graph_record.len() > self.maximum_graph_record_bytes as usize {
            return Err(Pc4LookupGraphCacheError::GraphRecordTooLarge {
                maximum: self.maximum_graph_record_bytes,
                actual: hit.graph_record.len(),
            });
        }
        Ok(())
    }

    fn entry(&self, field_id: u32) -> Option<&Pc4LookupGraphCacheEntry> {
        self.entries.iter().find(|entry| entry.field_id == field_id)
    }
}

fn check_cache_budget(
    kind: Pc4LookupGraphCacheBudgetKind,
    limit: usize,
    attempted: usize,
) -> Result<(), Pc4LookupGraphCacheError> {
    if attempted > limit {
        Err(Pc4LookupGraphCacheError::BudgetExceeded {
            kind,
            limit,
            attempted,
        })
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4LookupAdjacencyError {
    QueryTargetMismatch,
    FieldIdOutsideDomain { field_id: u32, field_count: u32 },
    RecordRequired { field_id: u32 },
    AllocationFailed,
}

impl Pc4LookupAdjacencyError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::QueryTargetMismatch => "pc4_lookup_adjacency_target_mismatch",
            Self::FieldIdOutsideDomain { .. } => "pc4_lookup_adjacency_field_id_outside_domain",
            Self::RecordRequired { .. } => "pc4_lookup_adjacency_record_required",
            Self::AllocationFailed => "pc4_lookup_adjacency_allocation_failed",
        }
    }
}

impl fmt::Display for Pc4LookupAdjacencyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4LookupAdjacencyError {}

/// Read-only complete-adjacency view over a shared qualified record cache.
pub struct Pc4LookupCompleteAdjacencyProvider<'a> {
    cache: &'a Pc4LookupGraphCache,
}

impl Pc4LookupCompleteAdjacencyProvider<'_> {
    fn complete_outgoing_edges_from_parts(
        &self,
        target: &QualifiedPc4TargetIdentity,
        source_field_id: u32,
        piece: Pc4GraphPiece,
        queue_index: usize,
    ) -> Result<QualifiedCompleteAdjacency, Pc4LookupAdjacencyError> {
        if target != self.cache.target() {
            return Err(Pc4LookupAdjacencyError::QueryTargetMismatch);
        }
        if source_field_id >= self.cache.field_count() {
            return Err(Pc4LookupAdjacencyError::FieldIdOutsideDomain {
                field_id: source_field_id,
                field_count: self.cache.field_count(),
            });
        }
        let entry =
            self.cache
                .entry(source_field_id)
                .ok_or(Pc4LookupAdjacencyError::RecordRequired {
                    field_id: source_field_id,
                })?;
        let targets = entry.decoded_record.targets(piece);
        let mut edges = Vec::new();
        edges
            .try_reserve_exact(targets.len())
            .map_err(|_| Pc4LookupAdjacencyError::AllocationFailed)?;
        for &target_field_id in targets {
            edges.push(QualifiedPc4GraphEdge::from_qualified_record(
                self.cache.target(),
                source_field_id,
                piece,
                target_field_id,
            ));
        }
        Ok(QualifiedCompleteAdjacency::from_qualified_provider(
            self.cache.target(),
            source_field_id,
            piece,
            queue_index,
            edges,
        ))
    }
}

impl QualifiedCompleteAdjacencyProvider for Pc4LookupCompleteAdjacencyProvider<'_> {
    type Error = Pc4LookupAdjacencyError;

    fn target(&self) -> &QualifiedPc4TargetIdentity {
        self.cache.target()
    }

    fn complete_outgoing_edges(
        &mut self,
        query: &FixedQueueAdjacencyQuery<'_>,
    ) -> Result<QualifiedCompleteAdjacency, Self::Error> {
        self.complete_outgoing_edges_from_parts(
            query.target(),
            query.source_field_id(),
            query.piece(),
            query.queue_index(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4LookupMaterializationField {
    Source,
    Target,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4LookupMaterializationError {
    EdgeTargetMismatch,
    FieldIdOutsideDomain {
        field: Pc4LookupMaterializationField,
        field_id: u32,
        field_count: u32,
    },
    SourceRecordRequired {
        field_id: u32,
    },
    TargetRecordRequired {
        field_id: u32,
    },
    SourceHashOutsideDomain(HydraFieldHashOutsideDomain),
    TargetHashOutsideDomain(HydraFieldHashOutsideDomain),
    Core(Pc4IlcMaterializationError),
    CorePieceMismatch,
    PlacementCoordinateOutsideIdentityDomain {
        x: i8,
        y: i8,
    },
    PlacementIdentity(PlacementIdentityError),
    AllocationFailed,
}

impl Pc4LookupMaterializationError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::EdgeTargetMismatch => "pc4_lookup_materializer_target_mismatch",
            Self::FieldIdOutsideDomain { .. } => "pc4_lookup_materializer_field_id_outside_domain",
            Self::SourceRecordRequired { .. } => "pc4_lookup_materializer_source_record_required",
            Self::TargetRecordRequired { .. } => "pc4_lookup_materializer_target_record_required",
            Self::SourceHashOutsideDomain(error) | Self::TargetHashOutsideDomain(error) => {
                error.reason()
            }
            Self::Core(error) => error.reason(),
            Self::CorePieceMismatch => "pc4_lookup_materializer_core_piece_mismatch",
            Self::PlacementCoordinateOutsideIdentityDomain { .. } => {
                "pc4_lookup_materializer_coordinate_outside_identity_domain"
            }
            Self::PlacementIdentity(error) => error.reason(),
            Self::AllocationFailed => "pc4_lookup_materializer_allocation_failed",
        }
    }
}

impl fmt::Display for Pc4LookupMaterializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4LookupMaterializationError {}

/// Read-only exact placement view over the same qualified cache used by the
/// adjacency provider.
pub struct Pc4LookupPlacementMaterializer<'a> {
    cache: &'a Pc4LookupGraphCache,
}

impl Pc4PlacementMaterializer for Pc4LookupPlacementMaterializer<'_> {
    type Error = Pc4LookupMaterializationError;

    fn profile(&self) -> Pc4RuleProfile {
        self.cache.profile()
    }

    fn enumerate(
        &mut self,
        edge: &QualifiedPc4GraphEdge,
    ) -> Result<MaterializationOutput, Self::Error> {
        if edge.target() != self.cache.target() {
            return Err(Pc4LookupMaterializationError::EdgeTargetMismatch);
        }
        validate_materialization_field_id(
            self.cache,
            Pc4LookupMaterializationField::Source,
            edge.source_field_id(),
        )?;
        validate_materialization_field_id(
            self.cache,
            Pc4LookupMaterializationField::Target,
            edge.target_field_id(),
        )?;
        let source = self.cache.entry(edge.source_field_id()).ok_or(
            Pc4LookupMaterializationError::SourceRecordRequired {
                field_id: edge.source_field_id(),
            },
        )?;
        let target = self.cache.entry(edge.target_field_id()).ok_or(
            Pc4LookupMaterializationError::TargetRecordRequired {
                field_id: edge.target_field_id(),
            },
        )?;
        let source_cells = hydra_field_hash_v1_to_clearra_board64_mask(source.field_hash)
            .map_err(Pc4LookupMaterializationError::SourceHashOutsideDomain)?;
        let target_cells = hydra_field_hash_v1_to_clearra_board64_mask(target.field_hash)
            .map_err(Pc4LookupMaterializationError::TargetHashOutsideDomain)?;
        let core_piece = graph_piece_to_core(edge.piece());
        let core_placements = materialize_pc4_ilc_transition(
            source_cells,
            target_cells,
            core_piece,
            profile_to_kick_table(self.cache.profile()),
        )
        .map_err(Pc4LookupMaterializationError::Core)?;
        let mut placements = Vec::new();
        placements
            .try_reserve_exact(core_placements.len())
            .map_err(|_| Pc4LookupMaterializationError::AllocationFailed)?;
        for placement in core_placements {
            placements.push(map_core_placement(placement, edge.piece())?);
        }
        Ok(MaterializationOutput {
            snapshot: self.cache.target().snapshot().clone(),
            profile: self.cache.profile(),
            source_field_id: edge.source_field_id(),
            piece: edge.piece(),
            target_field_id: edge.target_field_id(),
            placements,
        })
    }
}

fn validate_materialization_field_id(
    cache: &Pc4LookupGraphCache,
    field: Pc4LookupMaterializationField,
    field_id: u32,
) -> Result<(), Pc4LookupMaterializationError> {
    if field_id >= cache.field_count() {
        Err(Pc4LookupMaterializationError::FieldIdOutsideDomain {
            field,
            field_id,
            field_count: cache.field_count(),
        })
    } else {
        Ok(())
    }
}

fn map_core_placement(
    placement: Pc4IlcPlacement,
    expected_piece: Pc4GraphPiece,
) -> Result<ClearraPlacementIdentity, Pc4LookupMaterializationError> {
    if core_piece_to_graph(placement.piece()) != expected_piece {
        return Err(Pc4LookupMaterializationError::CorePieceMismatch);
    }
    let x = u16::try_from(placement.x()).map_err(|_| {
        Pc4LookupMaterializationError::PlacementCoordinateOutsideIdentityDomain {
            x: placement.x(),
            y: placement.y(),
        }
    })?;
    let y = u16::try_from(placement.y()).map_err(|_| {
        Pc4LookupMaterializationError::PlacementCoordinateOutsideIdentityDomain {
            x: placement.x(),
            y: placement.y(),
        }
    })?;
    ClearraPlacementIdentity::new(
        expected_piece,
        core_rotation_to_graph(placement.rotation()),
        x,
        y,
        placement.occupied_cells(),
    )
    .map_err(Pc4LookupMaterializationError::PlacementIdentity)
}

const fn graph_piece_to_core(piece: Pc4GraphPiece) -> PieceKind {
    match piece {
        Pc4GraphPiece::I => PieceKind::I,
        Pc4GraphPiece::O => PieceKind::O,
        Pc4GraphPiece::T => PieceKind::T,
        Pc4GraphPiece::S => PieceKind::S,
        Pc4GraphPiece::Z => PieceKind::Z,
        Pc4GraphPiece::J => PieceKind::J,
        Pc4GraphPiece::L => PieceKind::L,
    }
}

const fn core_piece_to_graph(piece: PieceKind) -> Pc4GraphPiece {
    match piece {
        PieceKind::I => Pc4GraphPiece::I,
        PieceKind::O => Pc4GraphPiece::O,
        PieceKind::T => Pc4GraphPiece::T,
        PieceKind::S => Pc4GraphPiece::S,
        PieceKind::Z => Pc4GraphPiece::Z,
        PieceKind::J => Pc4GraphPiece::J,
        PieceKind::L => Pc4GraphPiece::L,
    }
}

const fn core_rotation_to_graph(rotation: RotationState) -> PlacementRotation {
    match rotation {
        RotationState::Zero => PlacementRotation::Zero,
        RotationState::Right => PlacementRotation::Right,
        RotationState::Two => PlacementRotation::Two,
        RotationState::Left => PlacementRotation::Left,
    }
}

const fn profile_to_kick_table(profile: Pc4RuleProfile) -> KickTableProfileId {
    match profile {
        Pc4RuleProfile::Srs => KickTableProfileId::Srs90,
        Pc4RuleProfile::SrsPlus => KickTableProfileId::SrsPlus,
        Pc4RuleProfile::SrsX => KickTableProfileId::SrsX,
        Pc4RuleProfile::Jstris180 => KickTableProfileId::Jstris180,
        Pc4RuleProfile::NoKick => KickTableProfileId::NoKick,
    }
}

#[cfg(test)]
mod tests {
    use clearra_pc4_tablebase::{
        clearra_board64_mask_to_hydra_field_hash_v1, ArtifactDescriptor, DatasetSnapshotManifest,
        DatasetSnapshotVerifier, FieldIdIndexRelation, LookupSessionId, ManifestContentIdentity,
        Pc4ArtifactRole, Pc4ProfileManifest, Pc4TargetLines, Pc4TerminalUseCase,
        ProfileAvailability, ProfileQualification, ProfileTargetCompletenessQualification,
        SnapshotIdentity, SnapshotVerificationAttestation, SnapshotVerificationFailure,
        SnapshotVerificationRequest,
    };

    use super::*;

    const EMPTY_TARGETS: &[u32] = &[];

    struct Verifier;

    impl DatasetSnapshotVerifier for Verifier {
        fn verify(
            &mut self,
            request: SnapshotVerificationRequest<'_>,
        ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
            SnapshotVerificationAttestation::new(
                request.snapshot_identity().clone(),
                request.manifest_content_identity().clone(),
                "synthetic-lookup-graph-runtime-verification",
            )
            .map_err(|_| SnapshotVerificationFailure::Rejected)
        }
    }

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("non-zero test budget")
    }

    fn limits() -> Pc4LookupGraphCacheLimits {
        Pc4LookupGraphCacheLimits::new(nonzero(16), nonzero(16 * 1024), nonzero(16 * 1024))
    }

    fn activated_snapshot(generation: &str, field_count: u32) -> ActivatedSnapshot {
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                let prefix = profile.as_str();
                let descriptor = |role, suffix: &str, byte_len| {
                    ArtifactDescriptor::new(
                        role,
                        format!("{prefix}/{suffix}"),
                        byte_len,
                        format!("{generation}-{prefix}-{suffix}"),
                    )
                    .expect("synthetic artifact")
                };
                let manifest = Pc4ProfileManifest::new(
                    profile,
                    field_count,
                    GraphTargetEncoding::U24LittleEndian,
                    FieldIdIndexRelation::RecordOrdinal,
                    4_096,
                    descriptor(
                        Pc4ArtifactRole::FieldHashIndex,
                        "field.idx",
                        16 + u64::from(field_count) * 8,
                    ),
                    descriptor(
                        Pc4ArtifactRole::GraphOffsets,
                        "offsets.idx",
                        16 + (u64::from(field_count) + 1) * 4,
                    ),
                    descriptor(Pc4ArtifactRole::Graph, "graph.bin", 1_048_576),
                    ProfileQualification::new(
                        format!("{prefix}-index"),
                        format!("{prefix}-record"),
                        format!("{prefix}-provenance"),
                        format!("{prefix}-known-answer"),
                    )
                    .expect("synthetic profile qualification"),
                )
                .expect("synthetic profile manifest")
                .with_target_qualifications(
                    [
                        Pc4TerminalUseCase::PcSearch,
                        Pc4TerminalUseCase::SetupSearch,
                    ]
                    .into_iter()
                    .map(|use_case| {
                        ProfileTargetCompletenessQualification::new(
                            use_case,
                            Pc4TargetLines::new(4).expect("4L target"),
                            format!("{prefix}-{use_case:?}-terminal"),
                            format!("{prefix}-{use_case:?}-outgoing"),
                            format!("{prefix}-{use_case:?}-known-answer"),
                            format!("{prefix}-{use_case:?}-offline-parity"),
                        )
                        .expect("synthetic target qualification")
                    })
                    .collect(),
                )
                .expect("unique target qualifications");
                ProfileAvailability::qualified(manifest)
            })
            .collect();
        DatasetSnapshotManifest::new(
            SnapshotIdentity::new(
                "synthetic/repository",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                generation,
            )
            .expect("synthetic snapshot identity"),
            ManifestContentIdentity::new(format!("synthetic-manifest-{generation}"))
                .expect("synthetic manifest identity"),
            profiles,
        )
        .expect("synthetic manifest")
        .activate(&mut Verifier)
        .expect("activated synthetic snapshot")
    }

    fn target(
        snapshot: &ActivatedSnapshot,
        profile: Pc4RuleProfile,
        use_case: Pc4TerminalUseCase,
    ) -> QualifiedPc4TargetIdentity {
        snapshot
            .qualified_target(
                profile,
                use_case,
                Pc4TargetLines::new(4).expect("4L target"),
            )
            .expect("qualified target")
    }

    fn hydra_record(
        source_hash: u64,
        per_piece_targets: [&[u32]; 7],
        encoding: GraphTargetEncoding,
    ) -> Vec<u8> {
        let mut bytes = source_hash.to_be_bytes()[3..].to_vec();
        for targets in per_piece_targets {
            bytes.push(u8::try_from(targets.len()).expect("small test degree"));
            for target in targets {
                match encoding {
                    GraphTargetEncoding::U24LittleEndian => {
                        bytes.extend_from_slice(&target.to_le_bytes()[..3]);
                    }
                    GraphTargetEncoding::U32LittleEndian => {
                        bytes.extend_from_slice(&target.to_le_bytes());
                    }
                }
            }
        }
        bytes
    }

    fn hit(
        target: &QualifiedPc4TargetIdentity,
        session: u64,
        field_id: u32,
        field_hash: u64,
        per_piece_targets: [&[u32]; 7],
    ) -> LookupHit {
        LookupHit {
            lookup_session: LookupSessionId::new(session).expect("lookup session"),
            snapshot: target.snapshot().clone(),
            profile: target.profile(),
            field_id,
            field_hash,
            graph_target_encoding: GraphTargetEncoding::U24LittleEndian,
            graph_record: hydra_record(
                field_hash,
                per_piece_targets,
                GraphTargetEncoding::U24LittleEndian,
            ),
        }
    }

    fn empty_hit(
        target: &QualifiedPc4TargetIdentity,
        session: u64,
        field_id: u32,
        field_hash: u64,
    ) -> LookupHit {
        hit(target, session, field_id, field_hash, [EMPTY_TARGETS; 7])
    }

    #[test]
    fn cache_admission_is_atomic_idempotent_and_counts_both_representations() {
        let snapshot = activated_snapshot("generation-a", 8);
        let qualified_target = target(&snapshot, Pc4RuleProfile::Srs, Pc4TerminalUseCase::PcSearch);
        let mut cache = Pc4LookupGraphCache::new(&snapshot, qualified_target.clone(), limits())
            .expect("bound cache");
        let targets = [1, 3, 1];
        let first = hit(
            &qualified_target,
            1,
            0,
            0,
            [
                &targets,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
            ],
        );
        let encoded_bytes = first.graph_record.len();

        assert_eq!(
            cache.admit(&qualified_target, first.clone()),
            Ok(Pc4LookupGraphCacheAdmission::Inserted)
        );
        assert_eq!(cache.usage().record_count(), 1);
        assert_eq!(cache.usage().encoded_graph_bytes(), encoded_bytes);
        assert_eq!(cache.usage().decoded_target_count(), 3);
        assert_eq!(cache.usage().decoded_target_bytes(), 3 * size_of::<u32>());

        let mut repeated = first.clone();
        repeated.lookup_session = LookupSessionId::new(2).expect("second session");
        assert_eq!(
            cache.admit(&qualified_target, repeated),
            Ok(Pc4LookupGraphCacheAdmission::AlreadyPresent)
        );
        assert_eq!(cache.usage().record_count(), 1);

        let mut drifted = first;
        drifted.graph_record.push(0);
        assert_eq!(
            cache.admit(&qualified_target, drifted),
            Err(Pc4LookupGraphCacheError::ConflictingRecord { field_id: 0 })
        );
        assert_eq!(cache.usage().record_count(), 1);
        assert_eq!(cache.usage().encoded_graph_bytes(), encoded_bytes);

        assert_eq!(
            cache.admit(&qualified_target, empty_hit(&qualified_target, 3, 2, 0)),
            Err(Pc4LookupGraphCacheError::FieldHashMappedToDifferentId {
                field_hash: 0,
                existing_field_id: 0,
                actual_field_id: 2,
            })
        );
        assert_eq!(cache.usage().record_count(), 1);
    }

    #[test]
    fn cache_rejects_snapshot_profile_target_encoding_domain_and_decode_drift() {
        let snapshot = activated_snapshot("generation-a", 4);
        let other_snapshot = activated_snapshot("generation-b", 4);
        let qualified_target = target(&snapshot, Pc4RuleProfile::Srs, Pc4TerminalUseCase::PcSearch);
        let other_generation_target = target(
            &other_snapshot,
            Pc4RuleProfile::Srs,
            Pc4TerminalUseCase::PcSearch,
        );
        assert_eq!(
            Pc4LookupGraphCache::new(&snapshot, other_generation_target.clone(), limits())
                .expect_err("cross-generation target"),
            Pc4LookupGraphCacheStartError::TargetSnapshotMismatch
        );

        let mut cache = Pc4LookupGraphCache::new(&snapshot, qualified_target.clone(), limits())
            .expect("bound cache");
        let setup_target = target(
            &snapshot,
            Pc4RuleProfile::Srs,
            Pc4TerminalUseCase::SetupSearch,
        );
        assert_eq!(
            cache.admit(&setup_target, empty_hit(&setup_target, 1, 0, 0)),
            Err(Pc4LookupGraphCacheError::TargetMismatch)
        );

        let mut wrong_snapshot = empty_hit(&qualified_target, 2, 0, 0);
        wrong_snapshot.snapshot = other_generation_target.snapshot().clone();
        assert_eq!(
            cache.admit(&qualified_target, wrong_snapshot),
            Err(Pc4LookupGraphCacheError::LookupSnapshotMismatch)
        );
        let mut wrong_profile = empty_hit(&qualified_target, 3, 0, 0);
        wrong_profile.profile = Pc4RuleProfile::SrsPlus;
        assert_eq!(
            cache.admit(&qualified_target, wrong_profile),
            Err(Pc4LookupGraphCacheError::LookupProfileMismatch {
                expected: Pc4RuleProfile::Srs,
                actual: Pc4RuleProfile::SrsPlus,
            })
        );
        let mut wrong_encoding = empty_hit(&qualified_target, 4, 0, 0);
        wrong_encoding.graph_target_encoding = GraphTargetEncoding::U32LittleEndian;
        assert_eq!(
            cache.admit(&qualified_target, wrong_encoding),
            Err(Pc4LookupGraphCacheError::LookupTargetEncodingMismatch {
                expected: GraphTargetEncoding::U24LittleEndian,
                actual: GraphTargetEncoding::U32LittleEndian,
            })
        );
        assert_eq!(
            cache.admit(&qualified_target, empty_hit(&qualified_target, 5, 4, 1)),
            Err(Pc4LookupGraphCacheError::FieldIdOutsideDomain {
                field_id: 4,
                field_count: 4,
            })
        );
        let mut malformed = empty_hit(&qualified_target, 6, 0, 0);
        malformed.graph_record.pop();
        assert!(matches!(
            cache.admit(&qualified_target, malformed),
            Err(Pc4LookupGraphCacheError::GraphRecordDecode(_))
        ));
        assert_eq!(cache.usage(), Pc4LookupGraphCacheUsage::default());
    }

    #[test]
    fn every_cache_budget_rejects_without_mutating_records_or_counters() {
        let snapshot = activated_snapshot("generation-budget", 4);
        let qualified_target = target(&snapshot, Pc4RuleProfile::Srs, Pc4TerminalUseCase::PcSearch);

        let mut record_limited = Pc4LookupGraphCache::new(
            &snapshot,
            qualified_target.clone(),
            Pc4LookupGraphCacheLimits::new(nonzero(1), nonzero(1_024), nonzero(1_024)),
        )
        .expect("record-limited cache");
        record_limited
            .admit(&qualified_target, empty_hit(&qualified_target, 1, 0, 0))
            .expect("first record");
        let committed = record_limited.usage();
        assert_eq!(
            record_limited.admit(&qualified_target, empty_hit(&qualified_target, 2, 1, 1)),
            Err(Pc4LookupGraphCacheError::BudgetExceeded {
                kind: Pc4LookupGraphCacheBudgetKind::Records,
                limit: 1,
                attempted: 2,
            })
        );
        assert_eq!(record_limited.usage(), committed);
        assert!(!record_limited.contains_field_id(1));

        let minimum_record_bytes = empty_hit(&qualified_target, 3, 0, 0).graph_record.len();
        let mut encoded_limited = Pc4LookupGraphCache::new(
            &snapshot,
            qualified_target.clone(),
            Pc4LookupGraphCacheLimits::new(
                nonzero(2),
                nonzero(minimum_record_bytes - 1),
                nonzero(1_024),
            ),
        )
        .expect("encoded-limited cache");
        assert!(matches!(
            encoded_limited.admit(&qualified_target, empty_hit(&qualified_target, 3, 0, 0)),
            Err(Pc4LookupGraphCacheError::BudgetExceeded {
                kind: Pc4LookupGraphCacheBudgetKind::EncodedGraphBytes,
                ..
            })
        ));
        assert_eq!(encoded_limited.usage(), Pc4LookupGraphCacheUsage::default());

        let decoded_targets = [1, 2];
        let decoded_hit = hit(
            &qualified_target,
            4,
            0,
            0,
            [
                &decoded_targets,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
                EMPTY_TARGETS,
            ],
        );
        let mut decoded_limited = Pc4LookupGraphCache::new(
            &snapshot,
            qualified_target.clone(),
            Pc4LookupGraphCacheLimits::new(nonzero(2), nonzero(1_024), nonzero(4)),
        )
        .expect("decoded-limited cache");
        assert_eq!(
            decoded_limited.admit(&qualified_target, decoded_hit),
            Err(Pc4LookupGraphCacheError::BudgetExceeded {
                kind: Pc4LookupGraphCacheBudgetKind::DecodedTargetBytes,
                limit: 4,
                attempted: 8,
            })
        );
        assert_eq!(decoded_limited.usage(), Pc4LookupGraphCacheUsage::default());
    }

    #[test]
    fn provider_requires_a_record_and_returns_every_decoded_target_in_order() {
        let snapshot = activated_snapshot("generation-provider", 8);
        let qualified_target = target(&snapshot, Pc4RuleProfile::Srs, Pc4TerminalUseCase::PcSearch);
        let setup_target = target(
            &snapshot,
            Pc4RuleProfile::Srs,
            Pc4TerminalUseCase::SetupSearch,
        );
        let mut cache = Pc4LookupGraphCache::new(&snapshot, qualified_target.clone(), limits())
            .expect("bound cache");
        let provider = cache.adjacency_provider();
        assert_eq!(
            provider.complete_outgoing_edges_from_parts(&qualified_target, 0, Pc4GraphPiece::I, 7,),
            Err(Pc4LookupAdjacencyError::RecordRequired { field_id: 0 })
        );
        assert_eq!(
            provider.complete_outgoing_edges_from_parts(&setup_target, 0, Pc4GraphPiece::I, 7,),
            Err(Pc4LookupAdjacencyError::QueryTargetMismatch)
        );
        drop(provider);

        let outgoing = [3, 1, 3, 2];
        cache
            .admit(
                &qualified_target,
                hit(
                    &qualified_target,
                    1,
                    0,
                    0,
                    [
                        &outgoing,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                    ],
                ),
            )
            .expect("source record");
        let adjacency = cache
            .adjacency_provider()
            .complete_outgoing_edges_from_parts(&qualified_target, 0, Pc4GraphPiece::I, 7)
            .expect("complete adjacency");
        assert_eq!(adjacency.queue_index(), 7);
        assert_eq!(
            adjacency
                .edges()
                .iter()
                .map(QualifiedPc4GraphEdge::target_field_id)
                .collect::<Vec<_>>(),
            outgoing
        );
    }

    #[test]
    fn materializer_misses_are_typed_and_all_profile_rotation_maps_are_exact() {
        let snapshot = activated_snapshot("generation-misses", 4);
        let qualified_target = target(&snapshot, Pc4RuleProfile::Srs, Pc4TerminalUseCase::PcSearch);
        let setup_target = target(
            &snapshot,
            Pc4RuleProfile::Srs,
            Pc4TerminalUseCase::SetupSearch,
        );
        let mut cache = Pc4LookupGraphCache::new(&snapshot, qualified_target.clone(), limits())
            .expect("bound cache");
        let edge =
            QualifiedPc4GraphEdge::from_qualified_record(&qualified_target, 0, Pc4GraphPiece::I, 1);
        assert_eq!(
            cache.placement_materializer().enumerate(&edge),
            Err(Pc4LookupMaterializationError::SourceRecordRequired { field_id: 0 })
        );
        cache
            .admit(&qualified_target, empty_hit(&qualified_target, 1, 0, 0))
            .expect("source record");
        assert_eq!(
            cache.placement_materializer().enumerate(&edge),
            Err(Pc4LookupMaterializationError::TargetRecordRequired { field_id: 1 })
        );
        let wrong_edge =
            QualifiedPc4GraphEdge::from_qualified_record(&setup_target, 0, Pc4GraphPiece::I, 1);
        assert_eq!(
            cache.placement_materializer().enumerate(&wrong_edge),
            Err(Pc4LookupMaterializationError::EdgeTargetMismatch)
        );

        assert_eq!(
            profile_to_kick_table(Pc4RuleProfile::Srs),
            KickTableProfileId::Srs90
        );
        assert_eq!(
            profile_to_kick_table(Pc4RuleProfile::SrsPlus),
            KickTableProfileId::SrsPlus
        );
        assert_eq!(
            profile_to_kick_table(Pc4RuleProfile::SrsX),
            KickTableProfileId::SrsX
        );
        assert_eq!(
            profile_to_kick_table(Pc4RuleProfile::Jstris180),
            KickTableProfileId::Jstris180
        );
        assert_eq!(
            profile_to_kick_table(Pc4RuleProfile::NoKick),
            KickTableProfileId::NoKick
        );
        assert_eq!(
            core_rotation_to_graph(RotationState::Zero),
            PlacementRotation::Zero
        );
        assert_eq!(
            core_rotation_to_graph(RotationState::Right),
            PlacementRotation::Right
        );
        assert_eq!(
            core_rotation_to_graph(RotationState::Two),
            PlacementRotation::Two
        );
        assert_eq!(
            core_rotation_to_graph(RotationState::Left),
            PlacementRotation::Left
        );
    }

    #[test]
    fn root_i_edge_uses_hydra_row_orientation_and_preserves_every_core_placement() {
        let snapshot = activated_snapshot("generation-root-i", 4);
        let qualified_target = target(
            &snapshot,
            Pc4RuleProfile::SrsPlus,
            Pc4TerminalUseCase::PcSearch,
        );
        let mut cache = Pc4LookupGraphCache::new(&snapshot, qualified_target.clone(), limits())
            .expect("bound cache");
        let clearra_target = 0b1111_u64;
        let hydra_target =
            clearra_board64_mask_to_hydra_field_hash_v1(clearra_target).expect("Hydra target hash");
        assert_eq!(hydra_target, 0b1111_u64 << 6);
        let outgoing = [1];
        cache
            .admit(
                &qualified_target,
                hit(
                    &qualified_target,
                    1,
                    0,
                    0,
                    [
                        &outgoing,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                    ],
                ),
            )
            .expect("source record");
        cache
            .admit(
                &qualified_target,
                empty_hit(&qualified_target, 2, 1, hydra_target),
            )
            .expect("target record");
        let edge =
            QualifiedPc4GraphEdge::from_qualified_record(&qualified_target, 0, Pc4GraphPiece::I, 1);
        let output = cache
            .placement_materializer()
            .enumerate(&edge)
            .expect("root I materialization");
        let expected = materialize_pc4_ilc_transition(
            0,
            clearra_target,
            PieceKind::I,
            KickTableProfileId::SrsPlus,
        )
        .expect("direct core materialization")
        .into_iter()
        .map(|placement| map_core_placement(placement, Pc4GraphPiece::I))
        .collect::<Result<Vec<_>, _>>()
        .expect("mapped core placements");

        assert!(!output.placements.is_empty());
        assert_eq!(output.placements, expected);
        assert!(output.placements.iter().all(|placement| {
            placement.piece() == Pc4GraphPiece::I && placement.occupied_cells() == clearra_target
        }));
        assert!(output
            .placements
            .iter()
            .any(|placement| placement.x() == 0 && placement.y() == 0));
    }

    #[test]
    fn post_clear_logical_frame_materializes_in_the_clearra_physical_frame() {
        const WIDTH: usize = 10;
        const ROW_MASK: u64 = (1_u64 << WIDTH) - 1;

        let snapshot = activated_snapshot("generation-post-clear", 4);
        let qualified_target = target(&snapshot, Pc4RuleProfile::Srs, Pc4TerminalUseCase::PcSearch);
        let mut cache = Pc4LookupGraphCache::new(&snapshot, qualified_target.clone(), limits())
            .expect("bound cache");
        let source_cells = ROW_MASK | (0b11_1111_u64 << (WIDTH + 4));
        let target_cells = ROW_MASK | (ROW_MASK << WIDTH);
        let projected_i = 0b1111_u64 << WIDTH;
        let source_hash =
            clearra_board64_mask_to_hydra_field_hash_v1(source_cells).expect("source Hydra hash");
        let target_hash =
            clearra_board64_mask_to_hydra_field_hash_v1(target_cells).expect("target Hydra hash");
        let outgoing = [3];
        cache
            .admit(
                &qualified_target,
                hit(
                    &qualified_target,
                    1,
                    2,
                    source_hash,
                    [
                        &outgoing,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                        EMPTY_TARGETS,
                    ],
                ),
            )
            .expect("source record");
        cache
            .admit(
                &qualified_target,
                empty_hit(&qualified_target, 2, 3, target_hash),
            )
            .expect("target record");
        let output = cache
            .placement_materializer()
            .enumerate(&QualifiedPc4GraphEdge::from_qualified_record(
                &qualified_target,
                2,
                Pc4GraphPiece::I,
                3,
            ))
            .expect("post-clear materialization");

        assert!(!output.placements.is_empty());
        assert!(output
            .placements
            .iter()
            .all(|placement| { placement.occupied_cells() == projected_i && placement.y() == 0 }));
    }
}
