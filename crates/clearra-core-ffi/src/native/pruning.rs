use clearra_core_domain::pruning::{
    BatchId, ClearStateKey, EvidenceDigest, FallbackAction, ProofLevel, PruneReason,
    PruningEvidencePolicy,
};

pub const C_NATIVE_PRUNING_LEDGER_MAX_ENTRIES: usize = 64;
pub const C_NATIVE_PRUNING_MINIMAL_RECORD_MAX_ENTRIES: usize = 32;
pub const C_NATIVE_PRUNING_REASON_COUNT: usize = 26;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativePruningProofLedgerEntry {
    pub batch_id: u64,
    pub producer_id: u64,
    pub catalog_identity_digest: u64,
    pub state_layer: u8,
    pub prune_reason: u8,
    pub proof_level: u8,
    pub fallback_if_invalid: u8,
    pub affected_candidate_count: u32,
    pub has_clear_state_key: u8,
    pub reserved: [u8; 7],
    pub clear_state_key: u64,
    pub evidence_digest: u64,
}

impl CNativePruningProofLedgerEntry {
    fn decode_evidence(&self) -> Result<NativePruningEvidence, NativePruningLedgerError> {
        if self.batch_id == 0 {
            return Err(NativePruningLedgerError::MissingBatchId);
        }
        if self.evidence_digest == 0 {
            return Err(NativePruningLedgerError::MissingEvidenceDigest);
        }
        if self.producer_id == 0 {
            return Err(NativePruningLedgerError::MissingProducerId);
        }
        if self.catalog_identity_digest == 0 {
            return Err(NativePruningLedgerError::MissingCatalogIdentityDigest);
        }
        let has_clear_state_key = bool_from_native(
            self.has_clear_state_key,
            NativePruningLedgerError::InvalidClearStateFlag {
                value: self.has_clear_state_key,
            },
        )?;

        Ok(NativePruningEvidence {
            batch_id: BatchId(self.batch_id),
            producer_id: self.producer_id,
            catalog_identity_digest: EvidenceDigest(self.catalog_identity_digest),
            state_layer: self.state_layer,
            prune_reason: prune_reason(self.prune_reason)?,
            proof_level: proof_level(self.proof_level)?,
            fallback_if_invalid: fallback_action(self.fallback_if_invalid)?,
            affected_candidate_count: self.affected_candidate_count as usize,
            clear_state_key: has_clear_state_key.then_some(ClearStateKey(self.clear_state_key)),
            evidence_digest: EvidenceDigest(self.evidence_digest),
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativePruningMinimalRecord {
    pub batch_id: u64,
    pub producer_id: u64,
    pub catalog_identity_digest: u64,
    pub aggregate_evidence_digest: u64,
    pub affected_candidate_count: u64,
    pub prune_reason: u8,
    pub reserved: [u8; 7],
}

impl CNativePruningMinimalRecord {
    fn decode_record(&self) -> Result<NativePruningMinimalRecord, NativePruningLedgerError> {
        if self.batch_id == 0 {
            return Err(NativePruningLedgerError::MissingBatchId);
        }
        if self.producer_id == 0 {
            return Err(NativePruningLedgerError::MissingProducerId);
        }
        if self.catalog_identity_digest == 0 {
            return Err(NativePruningLedgerError::MissingCatalogIdentityDigest);
        }
        if self.aggregate_evidence_digest == 0 {
            return Err(NativePruningLedgerError::MissingEvidenceDigest);
        }
        if self.affected_candidate_count == 0 {
            return Err(NativePruningLedgerError::MissingAffectedCandidateCount);
        }
        Ok(NativePruningMinimalRecord {
            batch_id: BatchId(self.batch_id),
            producer_id: self.producer_id,
            catalog_identity_digest: EvidenceDigest(self.catalog_identity_digest),
            aggregate_evidence_digest: EvidenceDigest(self.aggregate_evidence_digest),
            affected_candidate_count: self.affected_candidate_count,
            prune_reason: prune_reason(self.prune_reason)?,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CNativePruningProofLedger {
    pub count: u16,
    pub capacity: u16,
    pub minimal_record_count: u16,
    pub minimal_record_capacity: u16,
    pub evidence_truncated: u8,
    pub evidence_policy: u8,
    pub complete_required_capacity_hit: u8,
    pub minimal_record_capacity_hit: u8,
    pub dropped_evidence_count: u32,
    pub candidates_kept_due_to_evidence_capacity: u32,
    pub prune_reason_counts: [u32; C_NATIVE_PRUNING_REASON_COUNT],
    pub entries: [CNativePruningProofLedgerEntry; C_NATIVE_PRUNING_LEDGER_MAX_ENTRIES],
    pub minimal_records: [CNativePruningMinimalRecord; C_NATIVE_PRUNING_MINIMAL_RECORD_MAX_ENTRIES],
}

impl Default for CNativePruningProofLedger {
    fn default() -> Self {
        Self {
            count: 0,
            capacity: 0,
            minimal_record_count: 0,
            minimal_record_capacity: 0,
            evidence_truncated: 0,
            evidence_policy: 0,
            complete_required_capacity_hit: 0,
            minimal_record_capacity_hit: 0,
            dropped_evidence_count: 0,
            candidates_kept_due_to_evidence_capacity: 0,
            prune_reason_counts: [0; C_NATIVE_PRUNING_REASON_COUNT],
            entries: [CNativePruningProofLedgerEntry::default();
                C_NATIVE_PRUNING_LEDGER_MAX_ENTRIES],
            minimal_records: [CNativePruningMinimalRecord::default();
                C_NATIVE_PRUNING_MINIMAL_RECORD_MAX_ENTRIES],
        }
    }
}

impl CNativePruningProofLedger {
    pub fn to_owned_report(&self) -> Result<NativePruningLedger, NativePruningLedgerError> {
        if usize::from(self.capacity) != C_NATIVE_PRUNING_LEDGER_MAX_ENTRIES {
            return Err(NativePruningLedgerError::InvalidCapacity {
                value: self.capacity,
            });
        }
        if usize::from(self.count) > C_NATIVE_PRUNING_LEDGER_MAX_ENTRIES {
            return Err(NativePruningLedgerError::CountExceeded { value: self.count });
        }
        if usize::from(self.minimal_record_capacity) != C_NATIVE_PRUNING_MINIMAL_RECORD_MAX_ENTRIES
        {
            return Err(NativePruningLedgerError::InvalidMinimalRecordCapacity {
                value: self.minimal_record_capacity,
            });
        }
        if usize::from(self.minimal_record_count) > C_NATIVE_PRUNING_MINIMAL_RECORD_MAX_ENTRIES {
            return Err(NativePruningLedgerError::MinimalRecordCountExceeded {
                value: self.minimal_record_count,
            });
        }
        let evidence_truncated = bool_from_native(
            self.evidence_truncated,
            NativePruningLedgerError::InvalidEvidenceTruncated {
                value: self.evidence_truncated,
            },
        )?;
        let evidence_policy = evidence_policy(self.evidence_policy)?;
        let complete_required_capacity_hit = bool_from_native(
            self.complete_required_capacity_hit,
            NativePruningLedgerError::InvalidCompleteRequiredCapacityHit {
                value: self.complete_required_capacity_hit,
            },
        )?;
        let minimal_record_capacity_hit = bool_from_native(
            self.minimal_record_capacity_hit,
            NativePruningLedgerError::InvalidMinimalRecordCapacityHit {
                value: self.minimal_record_capacity_hit,
            },
        )?;
        if evidence_policy == PruningEvidencePolicy::CompleteRequired && evidence_truncated {
            return Err(NativePruningLedgerError::CompleteRequiredEvidenceTruncated);
        }
        if evidence_policy == PruningEvidencePolicy::BestEffort && complete_required_capacity_hit {
            return Err(NativePruningLedgerError::BestEffortReportedCompleteRequiredCapacityHit);
        }
        if complete_required_capacity_hit && self.candidates_kept_due_to_evidence_capacity == 0 {
            return Err(NativePruningLedgerError::CompleteRequiredCapacityHitWithoutKeptCandidate);
        }
        if minimal_record_capacity_hit && self.candidates_kept_due_to_evidence_capacity == 0 {
            return Err(NativePruningLedgerError::MinimalRecordCapacityHitWithoutKeptCandidate);
        }
        let entries = self.entries[..usize::from(self.count)]
            .iter()
            .map(CNativePruningProofLedgerEntry::decode_evidence)
            .collect::<Result<Vec<_>, _>>()?;
        let minimal_records = self.minimal_records[..usize::from(self.minimal_record_count)]
            .iter()
            .map(CNativePruningMinimalRecord::decode_record)
            .collect::<Result<Vec<_>, _>>()?;
        for entry in &entries {
            if !minimal_records.iter().any(|record| {
                record.batch_id == entry.batch_id
                    && record.producer_id == entry.producer_id
                    && record.catalog_identity_digest == entry.catalog_identity_digest
                    && record.prune_reason == entry.prune_reason
                    && record.affected_candidate_count
                        >= u64::try_from(entry.affected_candidate_count).unwrap_or(u64::MAX)
            }) {
                return Err(NativePruningLedgerError::DetailedEvidenceMissingMinimalRecord);
            }
        }

        Ok(NativePruningLedger {
            entries,
            minimal_records,
            evidence_policy,
            evidence_truncated,
            dropped_evidence_count: self.dropped_evidence_count as usize,
            complete_required_capacity_hit,
            minimal_record_capacity_hit,
            candidates_kept_due_to_evidence_capacity: self.candidates_kept_due_to_evidence_capacity
                as usize,
            prune_reason_counts: self.prune_reason_counts,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativePruningEvidence {
    pub batch_id: BatchId,
    pub producer_id: u64,
    pub catalog_identity_digest: EvidenceDigest,
    pub state_layer: u8,
    pub prune_reason: PruneReason,
    pub proof_level: ProofLevel,
    pub fallback_if_invalid: FallbackAction,
    pub affected_candidate_count: usize,
    pub clear_state_key: Option<ClearStateKey>,
    pub evidence_digest: EvidenceDigest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativePruningMinimalRecord {
    pub batch_id: BatchId,
    pub producer_id: u64,
    pub catalog_identity_digest: EvidenceDigest,
    pub aggregate_evidence_digest: EvidenceDigest,
    pub affected_candidate_count: u64,
    pub prune_reason: PruneReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativePruningLedger {
    pub entries: Vec<NativePruningEvidence>,
    pub minimal_records: Vec<NativePruningMinimalRecord>,
    pub evidence_policy: PruningEvidencePolicy,
    pub evidence_truncated: bool,
    pub dropped_evidence_count: usize,
    pub complete_required_capacity_hit: bool,
    pub minimal_record_capacity_hit: bool,
    pub candidates_kept_due_to_evidence_capacity: usize,
    pub prune_reason_counts: [u32; C_NATIVE_PRUNING_REASON_COUNT],
}

impl NativePruningLedger {
    pub fn merge_partition_reports<I>(reports: I) -> Result<Self, NativePruningLedgerError>
    where
        I: IntoIterator<Item = Self>,
    {
        let mut reports = reports.into_iter();
        let Some(mut merged) = reports.next() else {
            return Err(NativePruningLedgerError::EmptyPartitionReportSet);
        };
        for report in reports {
            merged.merge_partition_report(report)?;
        }
        Ok(merged)
    }

    pub fn merge_partition_report(&mut self, report: Self) -> Result<(), NativePruningLedgerError> {
        if report.evidence_policy != self.evidence_policy {
            return Err(NativePruningLedgerError::InconsistentPartitionEvidencePolicy);
        }
        self.entries.extend(report.entries);
        self.minimal_records.extend(report.minimal_records);
        self.evidence_truncated |= report.evidence_truncated;
        self.dropped_evidence_count = self
            .dropped_evidence_count
            .saturating_add(report.dropped_evidence_count);
        self.complete_required_capacity_hit |= report.complete_required_capacity_hit;
        self.minimal_record_capacity_hit |= report.minimal_record_capacity_hit;
        self.candidates_kept_due_to_evidence_capacity = self
            .candidates_kept_due_to_evidence_capacity
            .saturating_add(report.candidates_kept_due_to_evidence_capacity);
        for (target, source) in self
            .prune_reason_counts
            .iter_mut()
            .zip(report.prune_reason_counts)
        {
            *target = target.saturating_add(source);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePruningLedgerError {
    InvalidCapacity { value: u16 },
    InvalidMinimalRecordCapacity { value: u16 },
    CountExceeded { value: u16 },
    MinimalRecordCountExceeded { value: u16 },
    InvalidEvidenceTruncated { value: u8 },
    InvalidEvidencePolicy { value: u8 },
    InvalidCompleteRequiredCapacityHit { value: u8 },
    InvalidMinimalRecordCapacityHit { value: u8 },
    CompleteRequiredEvidenceTruncated,
    BestEffortReportedCompleteRequiredCapacityHit,
    CompleteRequiredCapacityHitWithoutKeptCandidate,
    MinimalRecordCapacityHitWithoutKeptCandidate,
    DetailedEvidenceMissingMinimalRecord,
    InvalidPruneReason { value: u8 },
    InvalidProofLevel { value: u8 },
    InvalidFallbackAction { value: u8 },
    InvalidClearStateFlag { value: u8 },
    MissingBatchId,
    MissingProducerId,
    MissingCatalogIdentityDigest,
    MissingAffectedCandidateCount,
    MissingEvidenceDigest,
    EmptyPartitionReportSet,
    InconsistentPartitionEvidencePolicy,
}

fn bool_from_native(
    value: u8,
    error: NativePruningLedgerError,
) -> Result<bool, NativePruningLedgerError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(error),
    }
}

fn prune_reason(value: u8) -> Result<PruneReason, NativePruningLedgerError> {
    match value {
        1 => Ok(PruneReason::AreaOverflow),
        2 => Ok(PruneReason::PieceCountOverflow),
        3 => Ok(PruneReason::PlacementCollision),
        4 => Ok(PruneReason::TargetMaskOverflow),
        5 => Ok(PruneReason::RowCapacityOverflow),
        6 => Ok(PruneReason::CellDomainEmptyUnderClearState),
        7 => Ok(PruneReason::CellDomainEmptyForAllReachableClearStates),
        8 => Ok(PruneReason::ForcedPieceFamilyUnderClearState),
        9 => Ok(PruneReason::ForcedPieceFamilyForAllReachableClearStates),
        10 => Ok(PruneReason::ComponentExactCoverImpossible),
        11 => Ok(PruneReason::HoldAutomatonImpossible),
        12 => Ok(PruneReason::ReachabilityImpossible),
        13 => Ok(PruneReason::BuildOrdersHoldReachableIntersectionEmpty),
        14 => Ok(PruneReason::ResourceBudgetExceeded),
        15 => Ok(PruneReason::LineClearOrderImpossible),
        16 => Ok(PruneReason::ColumnDemandOverflow),
        17 => Ok(PruneReason::FullParentDomainEmpty),
        18 => Ok(PruneReason::SameTileParentDomainEmpty),
        19 => Ok(PruneReason::AdditiveInvariantMismatch),
        20 => Ok(PruneReason::SeparatorComponentInfeasible),
        21 => Ok(PruneReason::ParentDomainHallViolation),
        22 => Ok(PruneReason::ColumnDemandUnreachable),
        23 => Ok(PruneReason::BumperDomainEmpty),
        24 => Ok(PruneReason::BumperBridgeIncompatible),
        25 => Ok(PruneReason::RealizationDomainEmpty),
        _ => Err(NativePruningLedgerError::InvalidPruneReason { value }),
    }
}

fn proof_level(value: u8) -> Result<ProofLevel, NativePruningLedgerError> {
    match value {
        1 => Ok(ProofLevel::LocalOnly),
        2 => Ok(ProofLevel::ClearStateConditional),
        3 => Ok(ProofLevel::AllReachableClearStates),
        4 => Ok(ProofLevel::GlobalSafe),
        _ => Err(NativePruningLedgerError::InvalidProofLevel { value }),
    }
}

fn fallback_action(value: u8) -> Result<FallbackAction, NativePruningLedgerError> {
    match value {
        1 => Ok(FallbackAction::KeepCandidate),
        2 => Ok(FallbackAction::RunBuildUp),
        3 => Ok(FallbackAction::DisableDomainPruning),
        _ => Err(NativePruningLedgerError::InvalidFallbackAction { value }),
    }
}

fn evidence_policy(value: u8) -> Result<PruningEvidencePolicy, NativePruningLedgerError> {
    match value {
        1 => Ok(PruningEvidencePolicy::BestEffort),
        2 => Ok(PruningEvidencePolicy::CompleteRequired),
        _ => Err(NativePruningLedgerError::InvalidEvidencePolicy { value }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pruning_ledger_rejects_uninitialized_capacity() {
        let ledger = CNativePruningProofLedger::default();

        assert_eq!(
            ledger.to_owned_report(),
            Err(NativePruningLedgerError::InvalidCapacity { value: 0 })
        );
    }
}
