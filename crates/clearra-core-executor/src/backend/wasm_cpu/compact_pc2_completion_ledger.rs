//! Test-only issued-to-applied evidence for the compact CPU PC2 coordinator.
//!
//! The existing worker transport does not return scoped candidate identities.
//! This ledger proves exact work delivery and local application only; it is
//! not a caller authentication mechanism, a verifier of worker answers, or a
//! replacement for the separately checked physical replay and request scope.

use std::{
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;

use super::distributed::WasmCandidatePacket;

const LOCKS: usize = 5;
// The first slice only covers small compact families. Exceeding this bound
// is a no-evidence result; ordinary distributed PC can finish independently.
const MAX_CANDIDATES: usize = 64;
static NEXT_COORDINATOR_EPOCH: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WorkScope {
    coordinator_epoch: NonZeroU64,
    branch: u32,
    work: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExactCandidate {
    ordinal: u64,
    pass: u8,
    target: u32,
    rows: [u32; LOCKS],
}

impl ExactCandidate {
    fn from_packet(packet: &WasmCandidatePacket) -> Option<Self> {
        let rows: [u32; LOCKS] = packet.row_ids().try_into().ok()?;
        if packet.is_extended()
            || packet.pass_index() != 0
            || rows
                .iter()
                .enumerate()
                .any(|(index, row)| rows[..index].contains(row))
        {
            return None;
        }
        Some(Self {
            ordinal: packet.ordinal(),
            pass: packet.pass_index(),
            target: packet.target_index(),
            rows,
        })
    }
}

#[derive(Clone, Copy)]
struct IssuedWork {
    branch: u32,
    start: usize,
    end: usize,
    applied: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LedgerError {
    Cancelled,
    ResourceIncomplete,
    GeometryStillOpen,
    GeometryAlreadyClosed,
    WrongProducerCount,
    InvalidIssuedWork,
    WrongReceiptScope,
    WrongReceiptCandidates,
    DuplicateReceipt,
    MissingReceipt,
    ApplyRejected,
}

struct CompactPc2CompletionLedger {
    cancellation: ExecutionCancellationToken,
    coordinator_epoch: NonZeroU64,
    candidates: [Option<ExactCandidate>; MAX_CANDIDATES],
    work: [Option<IssuedWork>; MAX_CANDIDATES],
    candidate_count: usize,
    work_count: usize,
    geometry_closed: bool,
    failure: Option<LedgerError>,
}

/// An opaque, non-cloneable record of exact work delivery. This is not a
/// continuation proof. In particular, an empty seal is not a positive witness.
struct CompactPc2CompletionSeal {
    cancellation: ExecutionCancellationToken,
    coordinator_epoch: NonZeroU64,
    candidates: [Option<ExactCandidate>; MAX_CANDIDATES],
    work: [Option<IssuedWork>; MAX_CANDIDATES],
}

impl CompactPc2CompletionLedger {
    fn new(cancellation: &ExecutionCancellationToken) -> Result<Self, LedgerError> {
        let coordinator_epoch = NEXT_COORDINATOR_EPOCH
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |epoch| {
                epoch.checked_add(1)
            })
            .ok()
            .and_then(NonZeroU64::new)
            .ok_or(LedgerError::ResourceIncomplete)?;
        Ok(Self {
            cancellation: cancellation.clone(),
            coordinator_epoch,
            candidates: [None; MAX_CANDIDATES],
            work: [None; MAX_CANDIDATES],
            candidate_count: 0,
            work_count: 0,
            geometry_closed: false,
            failure: None,
        })
    }

    /// Called only by the coordinator, on compact CPU producer packets before
    /// they are dispatched. Record exact row order, never a digest or key.
    fn issue_work(
        &mut self,
        branch: u32,
        packets: &[WasmCandidatePacket],
    ) -> Result<WorkScope, LedgerError> {
        self.active()?;
        if self.geometry_closed {
            return self.reject(LedgerError::GeometryAlreadyClosed);
        }
        if packets.is_empty() {
            return self.reject(LedgerError::InvalidIssuedWork);
        }
        let start = self.candidate_count;
        if packets.len() > MAX_CANDIDATES - start {
            return self.reject(LedgerError::ResourceIncomplete);
        }
        let end = start + packets.len();
        for (index, packet) in (start..end).zip(packets) {
            let Some(candidate) = ExactCandidate::from_packet(packet) else {
                return self.reject(LedgerError::InvalidIssuedWork);
            };
            if candidate.ordinal != index as u64 {
                return self.reject(LedgerError::InvalidIssuedWork);
            }
            self.candidates[index] = Some(candidate);
        }

        // Work is nonempty, so the work table cannot have more entries than
        // the candidate table that was checked above.
        let work = self.work_count;
        self.work[work] = Some(IssuedWork {
            branch,
            start,
            end,
            applied: false,
        });
        self.work_count += 1;
        self.candidate_count = end;
        Ok(WorkScope {
            coordinator_epoch: self.coordinator_epoch,
            branch,
            work,
        })
    }

    /// The producer's local exhaustive `Completed` branch supplies this count.
    /// No result/summary received from the transport can close Geometry. A
    /// truncated producer must instead call `producer_resource_incomplete`.
    fn close_geometry(&mut self, produced_candidate_count: usize) -> Result<(), LedgerError> {
        self.active()?;
        if self.geometry_closed {
            return self.reject(LedgerError::GeometryAlreadyClosed);
        }
        if produced_candidate_count != self.candidate_count {
            return self.reject(LedgerError::WrongProducerCount);
        }
        self.geometry_closed = true;
        Ok(())
    }

    fn producer_resource_incomplete(&mut self) -> Result<(), LedgerError> {
        self.active()?;
        self.reject(LedgerError::ResourceIncomplete)
    }

    /// Validate one exact issued receipt before invoking a local merger hook.
    /// The hook must apply the validated verifier output as a single work unit.
    /// Raw packets or `CoreExecutionResult` alone cannot mark it applied.
    /// Work units can complete out of order, but the order within one cannot.
    fn apply_work(
        &mut self,
        scope: WorkScope,
        packets: &[WasmCandidatePacket],
        apply_locally: impl FnOnce() -> Result<(), LedgerError>,
    ) -> Result<(), LedgerError> {
        self.active()?;
        let Some(work) = scoped_work(self.coordinator_epoch, &self.work, scope) else {
            return self.reject(LedgerError::WrongReceiptScope);
        };
        if work.applied {
            return self.reject(LedgerError::DuplicateReceipt);
        }
        let expected = &self.candidates[work.start..work.end];
        if packets.len() != expected.len()
            || !packets.iter().zip(expected).all(|(actual, expected)| {
                ExactCandidate::from_packet(actual).as_ref() == expected.as_ref()
            })
        {
            return self.reject(LedgerError::WrongReceiptCandidates);
        }
        if let Err(error) = apply_locally() {
            return self.reject(error);
        }
        self.active()?;
        self.work[scope.work] = Some(IssuedWork {
            applied: true,
            ..work
        });
        Ok(())
    }

    fn seal(self) -> Result<CompactPc2CompletionSeal, LedgerError> {
        self.active()?;
        if !self.geometry_closed {
            return Err(LedgerError::GeometryStillOpen);
        }
        if self.work[..self.work_count]
            .iter()
            .any(|work| !work.is_some_and(|work| work.applied))
        {
            return Err(LedgerError::MissingReceipt);
        }
        Ok(CompactPc2CompletionSeal {
            cancellation: self.cancellation,
            coordinator_epoch: self.coordinator_epoch,
            candidates: self.candidates,
            work: self.work,
        })
    }

    fn active(&self) -> Result<(), LedgerError> {
        if self.cancellation.is_cancelled() {
            return Err(LedgerError::Cancelled);
        }
        self.failure.map_or(Ok(()), Err)
    }

    fn reject<T>(&mut self, error: LedgerError) -> Result<T, LedgerError> {
        self.failure = Some(error);
        Err(error)
    }
}

impl CompactPc2CompletionSeal {
    /// A replay claimant must carry the same applied work scope as the exact
    /// row packet. Looking up an ordinal using a different receipt fails.
    fn applied_identity(&self, scope: WorkScope, ordinal: u64) -> Option<ExactCandidate> {
        if self.cancellation.is_cancelled() {
            return None;
        }
        let index = usize::try_from(ordinal).ok()?;
        let work = scoped_work(self.coordinator_epoch, &self.work, scope)?;
        if !work.applied || !(work.start..work.end).contains(&index) {
            return None;
        }
        self.candidates[index]
    }
}

fn scoped_work(
    epoch: NonZeroU64,
    work: &[Option<IssuedWork>; MAX_CANDIDATES],
    scope: WorkScope,
) -> Option<IssuedWork> {
    if scope.coordinator_epoch != epoch {
        return None;
    }
    let entry = work.get(scope.work).copied().flatten()?;
    (entry.branch == scope.branch).then_some(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(ordinal: u64) -> WasmCandidatePacket {
        let start = 10 * u32::try_from(ordinal).unwrap();
        WasmCandidatePacket::new(ordinal, 0, (start..start + LOCKS as u32).collect())
    }

    fn ledger() -> CompactPc2CompletionLedger {
        CompactPc2CompletionLedger::new(&ExecutionCancellationToken::new()).unwrap()
    }

    #[test]
    fn compact_pc2_seal_keeps_exact_scope_and_accepts_cross_work_reordering() {
        let mut ledger = ledger();
        let first = [packet(0), packet(1)];
        let second = [packet(2)];
        let first_scope = ledger.issue_work(4, &first).unwrap();
        let second_scope = ledger.issue_work(9, &second).unwrap();
        ledger.apply_work(second_scope, &second, || Ok(())).unwrap();
        ledger.close_geometry(3).unwrap();
        ledger.apply_work(first_scope, &first, || Ok(())).unwrap();
        let seal = ledger.seal().unwrap();
        assert_eq!(
            seal.applied_identity(first_scope, 1),
            Some(ExactCandidate {
                ordinal: 1,
                pass: 0,
                target: 0,
                rows: [10, 11, 12, 13, 14],
            })
        );
        assert!(seal.applied_identity(second_scope, 2).is_some());
        assert!(seal.applied_identity(second_scope, 1).is_none());
        assert!(seal.applied_identity(first_scope, 2).is_none());
    }

    #[test]
    fn compact_pc2_receipt_rejects_duplicate_and_missing_work() {
        let mut duplicate = ledger();
        let packets = [packet(0)];
        let scope = duplicate.issue_work(0, &packets).unwrap();
        duplicate.apply_work(scope, &packets, || Ok(())).unwrap();
        assert_eq!(
            duplicate.apply_work(scope, &packets, || panic!("duplicate cannot reach merger")),
            Err(LedgerError::DuplicateReceipt)
        );
        assert_eq!(duplicate.seal().err(), Some(LedgerError::DuplicateReceipt));

        let mut missing = ledger();
        let first = [packet(0)];
        let second = [packet(1)];
        missing.issue_work(1, &first).unwrap();
        let second_scope = missing.issue_work(2, &second).unwrap();
        missing
            .apply_work(second_scope, &second, || Ok(()))
            .unwrap();
        missing.close_geometry(2).unwrap();
        assert_eq!(missing.seal().err(), Some(LedgerError::MissingReceipt));
    }

    #[test]
    fn compact_pc2_receipt_rejects_same_count_with_reordered_or_altered_candidates() {
        let original = [packet(0), packet(1)];
        let variants = [
            [packet(1), packet(0)],
            [packet(0), packet(0)],
            [
                WasmCandidatePacket::new(0, 1, vec![0, 1, 2, 3, 4]),
                packet(1),
            ],
            [
                WasmCandidatePacket::new(0, 0, vec![1, 0, 2, 3, 4]),
                packet(1),
            ],
            [
                WasmCandidatePacket::new(0, 0, vec![0, 1, 2, 3, 5]),
                packet(1),
            ],
            [
                WasmCandidatePacket::for_pass(0, 1, 0, vec![0, 1, 2, 3, 4]),
                packet(1),
            ],
        ];
        for invalid in variants
            .iter()
            .map(|variant| variant.as_slice())
            .chain([&original[..1]])
        {
            let mut ledger = ledger();
            let scope = ledger.issue_work(1, &original).unwrap();
            ledger.close_geometry(2).unwrap();
            assert_eq!(
                ledger.apply_work(scope, invalid, || panic!("invalid receipt reached merger")),
                Err(LedgerError::WrongReceiptCandidates)
            );
            assert_eq!(
                ledger.seal().err(),
                Some(LedgerError::WrongReceiptCandidates)
            );
        }
    }

    #[test]
    fn compact_pc2_receipt_rejects_foreign_branch_work_and_coordinator_epoch() {
        let packets = [packet(0)];
        let mut foreign = ledger();
        let foreign_scope = foreign.issue_work(4, &packets).unwrap();
        for modify_scope in [
            Some(0), // different branch
            Some(1), // nonexistent work
            None,    // another coordinator's first work, with identical packets
        ] {
            let mut ledger = ledger();
            let scope = ledger.issue_work(4, &packets).unwrap();
            let invalid = match modify_scope {
                Some(0) => WorkScope { branch: 5, ..scope },
                Some(_) => WorkScope { work: 1, ..scope },
                None => foreign_scope,
            };
            assert_eq!(
                ledger.apply_work(invalid, &packets, || panic!("wrong scope reached merger")),
                Err(LedgerError::WrongReceiptScope)
            );
            assert_eq!(ledger.seal().err(), Some(LedgerError::WrongReceiptScope));
        }
    }

    #[test]
    fn compact_pc2_seal_requires_the_producers_exhaustive_exact_count() {
        let open = ledger();
        assert_eq!(open.seal().err(), Some(LedgerError::GeometryStillOpen));

        let mut wrong_count = ledger();
        wrong_count.issue_work(0, &[packet(0)]).unwrap();
        assert_eq!(
            wrong_count.close_geometry(2),
            Err(LedgerError::WrongProducerCount)
        );
        assert_eq!(
            wrong_count.seal().err(),
            Some(LedgerError::WrongProducerCount)
        );

        let mut gap = ledger();
        assert_eq!(
            gap.issue_work(0, &[packet(1)]),
            Err(LedgerError::InvalidIssuedWork)
        );

        let mut duplicate_close = ledger();
        duplicate_close.close_geometry(0).unwrap();
        assert_eq!(
            duplicate_close.issue_work(0, &[packet(0)]),
            Err(LedgerError::GeometryAlreadyClosed)
        );

        let mut empty = ledger();
        empty.close_geometry(0).unwrap();
        let seal = empty.seal().unwrap();
        let scope = WorkScope {
            coordinator_epoch: seal.coordinator_epoch,
            branch: 0,
            work: 0,
        };
        assert!(seal.applied_identity(scope, 0).is_none());
    }

    #[test]
    fn compact_pc2_seal_cannot_cross_cancellation_before_during_or_after_application() {
        let packets = [packet(0)];
        for when in 0..3 {
            let token = ExecutionCancellationToken::new();
            let mut ledger = CompactPc2CompletionLedger::new(&token).unwrap();
            let scope = ledger.issue_work(0, &packets).unwrap();
            ledger.close_geometry(1).unwrap();
            if when == 0 {
                token.handle().cancel();
            }
            let applied = ledger.apply_work(scope, &packets, || {
                assert_ne!(when, 0, "pre-cancelled work reached merger");
                if when == 1 {
                    token.handle().cancel();
                }
                Ok(())
            });
            if when == 2 {
                assert_eq!(applied, Ok(()));
                let seal = ledger.seal().unwrap();
                assert!(seal.applied_identity(scope, 0).is_some());
                token.handle().cancel();
                assert!(seal.applied_identity(scope, 0).is_none());
            } else {
                assert_eq!(applied, Err(LedgerError::Cancelled));
                assert_eq!(ledger.seal().err(), Some(LedgerError::Cancelled));
            }
        }
    }

    #[test]
    fn compact_pc2_seal_cannot_cross_resource_or_local_apply_failure() {
        let mut bounded = ledger();
        let packets = (0..MAX_CANDIDATES as u64).map(packet).collect::<Vec<_>>();
        bounded.issue_work(0, &packets).unwrap();
        assert_eq!(
            bounded.issue_work(0, &[packet(MAX_CANDIDATES as u64)]),
            Err(LedgerError::ResourceIncomplete)
        );
        assert_eq!(bounded.seal().err(), Some(LedgerError::ResourceIncomplete));

        let mut truncated_producer = ledger();
        assert_eq!(
            truncated_producer.producer_resource_incomplete(),
            Err(LedgerError::ResourceIncomplete)
        );
        assert_eq!(
            truncated_producer.seal().err(),
            Some(LedgerError::ResourceIncomplete)
        );

        for reason in [LedgerError::ResourceIncomplete, LedgerError::ApplyRejected] {
            let mut ledger = ledger();
            let packets = [packet(0)];
            let scope = ledger.issue_work(0, &packets).unwrap();
            ledger.close_geometry(1).unwrap();
            assert_eq!(
                ledger.apply_work(scope, &packets, || Err(reason)),
                Err(reason)
            );
            assert_eq!(ledger.seal().err(), Some(reason));
        }
    }
}
