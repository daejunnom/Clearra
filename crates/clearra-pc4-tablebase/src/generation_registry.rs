// SRP rationale: this module has one behavior-level change reason: retaining
// and atomically handing off already-qualified immutable PC4 generations.

use crate::{ActivatedSnapshot, QualifiedSnapshotIdentity, SnapshotIdentity};
use core::fmt;
use std::{collections::VecDeque, sync::Arc};

/// Maximum number of rollback generations retained by the pure registry.
///
/// The limit is deliberately small and explicit. It bounds registry metadata;
/// an in-flight [`PinnedPc4Generation`] may keep an older generation alive
/// after the registry itself evicts that generation.
pub const MAX_RETAINED_PC4_GENERATIONS: u8 = 16;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4GenerationRetentionLimit(u8);

impl Pc4GenerationRetentionLimit {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = MAX_RETAINED_PC4_GENERATIONS;

    pub fn new(value: u8) -> Result<Self, Pc4GenerationRetentionLimitError> {
        if !(Self::MIN..=Self::MAX).contains(&value) {
            return Err(Pc4GenerationRetentionLimitError { actual: value });
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4GenerationRetentionLimitError {
    pub actual: u8,
}

impl Pc4GenerationRetentionLimitError {
    pub const fn reason(self) -> &'static str {
        "pc4_generation_retention_limit_invalid"
    }
}

impl fmt::Display for Pc4GenerationRetentionLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4GenerationRetentionLimitError {}

/// Monotonic optimistic-write version for one registry instance.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4GenerationRegistryVersion(u64);

impl Pc4GenerationRegistryVersion {
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinnedPc4Generation {
    snapshot: Arc<ActivatedSnapshot>,
}

impl PinnedPc4Generation {
    fn new(snapshot: Arc<ActivatedSnapshot>) -> Self {
        Self { snapshot }
    }

    /// Returns the immutable verified snapshot and all of its profile manifests.
    pub fn activated_snapshot(&self) -> &ActivatedSnapshot {
        &self.snapshot
    }

    pub fn qualified_identity(&self) -> &QualifiedSnapshotIdentity {
        self.snapshot.qualified_identity()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4CurrentGeneration {
    NoCurrent,
    Current(PinnedPc4Generation),
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Pc4GenerationStageToken {
    stage_id: u64,
    registry_version: Pc4GenerationRegistryVersion,
}

impl Pc4GenerationStageToken {
    pub const fn registry_version(&self) -> Pc4GenerationRegistryVersion {
        self.registry_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4GenerationStageOutcome {
    /// The candidate is already the current exact qualified generation.
    Current(PinnedPc4Generation),
    /// The candidate is already staged by another overlapping caller.
    AlreadyStaged {
        token: Pc4GenerationStageToken,
        generation: PinnedPc4Generation,
    },
    /// The candidate is retained and can be selected only through rollback.
    Retained(PinnedPc4Generation),
    /// A new exact qualified generation was staged without changing current.
    Staged {
        token: Pc4GenerationStageToken,
        generation: PinnedPc4Generation,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4GenerationRetentionChange {
    NoPreviousCurrent,
    Retained {
        previous_current: PinnedPc4Generation,
    },
    RetainedAndEvicted {
        previous_current: PinnedPc4Generation,
        evicted: PinnedPc4Generation,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4GenerationPromotionOutcome {
    pub version: Pc4GenerationRegistryVersion,
    pub current: PinnedPc4Generation,
    pub retention: Pc4GenerationRetentionChange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4GenerationCancellationOutcome {
    pub version: Pc4GenerationRegistryVersion,
    pub cancelled: PinnedPc4Generation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4GenerationRollbackOutcome {
    AlreadyCurrent(PinnedPc4Generation),
    RolledBack {
        version: Pc4GenerationRegistryVersion,
        current: PinnedPc4Generation,
        retained_previous_current: PinnedPc4Generation,
    },
}

/// Host-observed preparation failure. Recording it never starts I/O or an
/// offline fallback and never mutates registry state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4GenerationPreparationFailure {
    FetchFailed,
    QualificationRejected,
    QualificationProviderFailed,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4GenerationFailureOutcome {
    NoCurrent {
        failure: Pc4GenerationPreparationFailure,
    },
    CurrentPreserved {
        failure: Pc4GenerationPreparationFailure,
        current: PinnedPc4Generation,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4GenerationRegistrySlot {
    Current,
    Staged,
    Retained,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4GenerationIdentityDrift {
    /// The same repository + generation label was rebound to another revision.
    RevisionForGenerationLabel,
    /// The same immutable snapshot identity was rebound to different verified
    /// manifest content or verification evidence.
    QualificationForSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ClosedStageDisposition {
    Cancelled,
    Promoted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4GenerationRegistryError {
    StaleWrite {
        expected: Pc4GenerationRegistryVersion,
        actual: Pc4GenerationRegistryVersion,
    },
    StageSlotOccupied,
    StageTokenMismatch,
    StageClosed {
        disposition: Pc4ClosedStageDisposition,
    },
    NoStagedGeneration,
    IdentityDrift {
        slot: Pc4GenerationRegistrySlot,
        drift: Pc4GenerationIdentityDrift,
    },
    NoCurrentGeneration,
    RollbackBlockedByStagedGeneration,
    RollbackTargetIsStaged,
    RetainedGenerationNotFound,
    VersionExhausted,
    StageIdExhausted,
}

impl Pc4GenerationRegistryError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::StaleWrite { .. } => "pc4_generation_registry_stale_write",
            Self::StageSlotOccupied => "pc4_generation_registry_stage_slot_occupied",
            Self::StageTokenMismatch => "pc4_generation_registry_stage_token_mismatch",
            Self::StageClosed { .. } => "pc4_generation_registry_stage_closed",
            Self::NoStagedGeneration => "pc4_generation_registry_no_staged_generation",
            Self::IdentityDrift { .. } => "pc4_generation_registry_identity_drift",
            Self::NoCurrentGeneration => "pc4_generation_registry_no_current_generation",
            Self::RollbackBlockedByStagedGeneration => {
                "pc4_generation_registry_rollback_blocked_by_stage"
            }
            Self::RollbackTargetIsStaged => "pc4_generation_registry_rollback_target_is_staged",
            Self::RetainedGenerationNotFound => {
                "pc4_generation_registry_retained_generation_not_found"
            }
            Self::VersionExhausted => "pc4_generation_registry_version_exhausted",
            Self::StageIdExhausted => "pc4_generation_registry_stage_id_exhausted",
        }
    }
}

impl fmt::Display for Pc4GenerationRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for Pc4GenerationRegistryError {}

#[derive(Clone, Debug)]
struct StagedGeneration {
    token: Pc4GenerationStageToken,
    snapshot: Arc<ActivatedSnapshot>,
}

#[derive(Clone, Debug)]
struct ClosedStage {
    token: Pc4GenerationStageToken,
    disposition: Pc4ClosedStageDisposition,
}

/// Pure, feature-off N-generation registry for verified PC4 snapshots.
///
/// The registry neither discovers nor fetches a dataset. It accepts only an
/// [`ActivatedSnapshot`], which means qualification has already bound the
/// dynamic upstream revision and manifest. A request pins current before doing
/// work; promotion never changes that request's immutable `Arc`.
#[derive(Debug)]
pub struct Pc4GenerationRegistry {
    retention_limit: Pc4GenerationRetentionLimit,
    version: Pc4GenerationRegistryVersion,
    next_stage_id: u64,
    current: Option<Arc<ActivatedSnapshot>>,
    staged: Option<StagedGeneration>,
    retained: VecDeque<Arc<ActivatedSnapshot>>,
    last_closed_stage: Option<ClosedStage>,
}

impl Pc4GenerationRegistry {
    pub fn new(retention_limit: Pc4GenerationRetentionLimit) -> Self {
        Self {
            retention_limit,
            version: Pc4GenerationRegistryVersion(0),
            next_stage_id: 0,
            current: None,
            staged: None,
            retained: VecDeque::with_capacity(usize::from(retention_limit.get())),
            last_closed_stage: None,
        }
    }

    pub const fn retention_limit(&self) -> Pc4GenerationRetentionLimit {
        self.retention_limit
    }

    pub const fn version(&self) -> Pc4GenerationRegistryVersion {
        self.version
    }

    pub fn pin_current(&self) -> Pc4CurrentGeneration {
        match &self.current {
            Some(current) => {
                Pc4CurrentGeneration::Current(PinnedPc4Generation::new(Arc::clone(current)))
            }
            None => Pc4CurrentGeneration::NoCurrent,
        }
    }

    pub fn staged_generation(&self) -> Option<PinnedPc4Generation> {
        self.staged
            .as_ref()
            .map(|staged| PinnedPc4Generation::new(Arc::clone(&staged.snapshot)))
    }

    pub fn retained_identities(&self) -> impl ExactSizeIterator<Item = &QualifiedSnapshotIdentity> {
        self.retained
            .iter()
            .map(|snapshot| snapshot.qualified_identity())
    }

    /// Returns a typed, non-mutating failure result for host-side fetch or
    /// qualification. It does not imply or start offline execution.
    pub fn preserve_after_preparation_failure(
        &self,
        failure: Pc4GenerationPreparationFailure,
    ) -> Pc4GenerationFailureOutcome {
        match self.pin_current() {
            Pc4CurrentGeneration::NoCurrent => Pc4GenerationFailureOutcome::NoCurrent { failure },
            Pc4CurrentGeneration::Current(current) => {
                Pc4GenerationFailureOutcome::CurrentPreserved { failure, current }
            }
        }
    }

    /// Stages one already-qualified generation without replacing current.
    pub fn stage(
        &mut self,
        expected: Pc4GenerationRegistryVersion,
        candidate: Arc<ActivatedSnapshot>,
    ) -> Result<Pc4GenerationStageOutcome, Pc4GenerationRegistryError> {
        self.require_version(expected)?;

        if let Some(current) = &self.current {
            if current.qualified_identity() == candidate.qualified_identity() {
                return Ok(Pc4GenerationStageOutcome::Current(
                    PinnedPc4Generation::new(Arc::clone(current)),
                ));
            }
        }
        if let Some(staged) = &self.staged {
            if staged.snapshot.qualified_identity() == candidate.qualified_identity() {
                return Ok(Pc4GenerationStageOutcome::AlreadyStaged {
                    token: staged.token,
                    generation: PinnedPc4Generation::new(Arc::clone(&staged.snapshot)),
                });
            }
        }
        if let Some(retained) = self
            .retained
            .iter()
            .find(|snapshot| snapshot.qualified_identity() == candidate.qualified_identity())
        {
            return Ok(Pc4GenerationStageOutcome::Retained(
                PinnedPc4Generation::new(Arc::clone(retained)),
            ));
        }

        self.reject_identity_drift(candidate.qualified_identity())?;
        if self.staged.is_some() {
            return Err(Pc4GenerationRegistryError::StageSlotOccupied);
        }

        let next_version = self.next_version()?;
        let next_stage_id = self
            .next_stage_id
            .checked_add(1)
            .ok_or(Pc4GenerationRegistryError::StageIdExhausted)?;
        let token = Pc4GenerationStageToken {
            stage_id: self.next_stage_id,
            registry_version: next_version,
        };
        self.next_stage_id = next_stage_id;
        self.version = next_version;
        self.staged = Some(StagedGeneration {
            token,
            snapshot: Arc::clone(&candidate),
        });
        Ok(Pc4GenerationStageOutcome::Staged {
            token,
            generation: PinnedPc4Generation::new(candidate),
        })
    }

    /// Atomically replaces current with the exact staged generation.
    pub fn promote(
        &mut self,
        token: &Pc4GenerationStageToken,
    ) -> Result<Pc4GenerationPromotionOutcome, Pc4GenerationRegistryError> {
        self.require_open_stage_token(token)?;
        let next_version = self.next_version()?;
        let staged = self
            .staged
            .take()
            .expect("validated stage token requires staged generation");
        let promoted = staged.snapshot;
        let retention = match self.current.take() {
            None => Pc4GenerationRetentionChange::NoPreviousCurrent,
            Some(previous) => {
                let evicted = if self.retained.len() == usize::from(self.retention_limit.get()) {
                    self.retained.pop_back()
                } else {
                    None
                };
                self.retained.push_front(Arc::clone(&previous));
                match evicted {
                    Some(evicted) => Pc4GenerationRetentionChange::RetainedAndEvicted {
                        previous_current: PinnedPc4Generation::new(previous),
                        evicted: PinnedPc4Generation::new(evicted),
                    },
                    None => Pc4GenerationRetentionChange::Retained {
                        previous_current: PinnedPc4Generation::new(previous),
                    },
                }
            }
        };
        self.current = Some(Arc::clone(&promoted));
        self.version = next_version;
        self.last_closed_stage = Some(ClosedStage {
            token: staged.token,
            disposition: Pc4ClosedStageDisposition::Promoted,
        });
        Ok(Pc4GenerationPromotionOutcome {
            version: next_version,
            current: PinnedPc4Generation::new(promoted),
            retention,
        })
    }

    pub fn cancel_staged(
        &mut self,
        token: &Pc4GenerationStageToken,
    ) -> Result<Pc4GenerationCancellationOutcome, Pc4GenerationRegistryError> {
        self.require_open_stage_token(token)?;
        let next_version = self.next_version()?;
        let staged = self
            .staged
            .take()
            .expect("validated stage token requires staged generation");
        let cancelled = Arc::clone(&staged.snapshot);
        self.version = next_version;
        self.last_closed_stage = Some(ClosedStage {
            token: staged.token,
            disposition: Pc4ClosedStageDisposition::Cancelled,
        });
        Ok(Pc4GenerationCancellationOutcome {
            version: next_version,
            cancelled: PinnedPc4Generation::new(cancelled),
        })
    }

    /// Atomically selects an exact retained generation as current.
    pub fn rollback(
        &mut self,
        expected: Pc4GenerationRegistryVersion,
        target: &QualifiedSnapshotIdentity,
    ) -> Result<Pc4GenerationRollbackOutcome, Pc4GenerationRegistryError> {
        self.require_version(expected)?;
        if self.staged.is_some() {
            if self
                .staged
                .as_ref()
                .is_some_and(|staged| staged.snapshot.qualified_identity() == target)
            {
                return Err(Pc4GenerationRegistryError::RollbackTargetIsStaged);
            }
            return Err(Pc4GenerationRegistryError::RollbackBlockedByStagedGeneration);
        }
        let current = self
            .current
            .as_ref()
            .ok_or(Pc4GenerationRegistryError::NoCurrentGeneration)?;
        if current.qualified_identity() == target {
            return Ok(Pc4GenerationRollbackOutcome::AlreadyCurrent(
                PinnedPc4Generation::new(Arc::clone(current)),
            ));
        }
        self.reject_identity_drift(target)?;
        let retained_index = self
            .retained
            .iter()
            .position(|snapshot| snapshot.qualified_identity() == target)
            .ok_or(Pc4GenerationRegistryError::RetainedGenerationNotFound)?;
        let next_version = self.next_version()?;
        let restored = self
            .retained
            .remove(retained_index)
            .expect("retained index came from the same deque");
        let previous = self
            .current
            .replace(Arc::clone(&restored))
            .expect("rollback requires a current generation");
        // Removing the rollback target created capacity before this insertion.
        self.retained.push_front(Arc::clone(&previous));
        self.version = next_version;
        Ok(Pc4GenerationRollbackOutcome::RolledBack {
            version: next_version,
            current: PinnedPc4Generation::new(restored),
            retained_previous_current: PinnedPc4Generation::new(previous),
        })
    }

    fn require_version(
        &self,
        expected: Pc4GenerationRegistryVersion,
    ) -> Result<(), Pc4GenerationRegistryError> {
        if expected == self.version {
            Ok(())
        } else {
            Err(Pc4GenerationRegistryError::StaleWrite {
                expected,
                actual: self.version,
            })
        }
    }

    fn require_open_stage_token(
        &self,
        token: &Pc4GenerationStageToken,
    ) -> Result<(), Pc4GenerationRegistryError> {
        if let Some(closed) = &self.last_closed_stage {
            if closed.token.stage_id == token.stage_id {
                return Err(Pc4GenerationRegistryError::StageClosed {
                    disposition: closed.disposition,
                });
            }
        }
        self.require_version(token.registry_version)?;
        match &self.staged {
            Some(staged) if staged.token.stage_id == token.stage_id => Ok(()),
            Some(_) => Err(Pc4GenerationRegistryError::StageTokenMismatch),
            None => Err(Pc4GenerationRegistryError::NoStagedGeneration),
        }
    }

    fn next_version(&self) -> Result<Pc4GenerationRegistryVersion, Pc4GenerationRegistryError> {
        self.version
            .0
            .checked_add(1)
            .map(Pc4GenerationRegistryVersion)
            .ok_or(Pc4GenerationRegistryError::VersionExhausted)
    }

    fn reject_identity_drift(
        &self,
        candidate: &QualifiedSnapshotIdentity,
    ) -> Result<(), Pc4GenerationRegistryError> {
        if let Some(error) = self.identity_drift_in_slot(
            candidate,
            self.current.iter(),
            Pc4GenerationRegistrySlot::Current,
        ) {
            return Err(error);
        }
        if let Some(error) = self.identity_drift_in_slot(
            candidate,
            self.staged.iter().map(|staged| &staged.snapshot),
            Pc4GenerationRegistrySlot::Staged,
        ) {
            return Err(error);
        }
        if let Some(error) = self.identity_drift_in_slot(
            candidate,
            self.retained.iter(),
            Pc4GenerationRegistrySlot::Retained,
        ) {
            return Err(error);
        }
        Ok(())
    }

    fn identity_drift_in_slot<'a, I>(
        &self,
        candidate: &QualifiedSnapshotIdentity,
        existing: I,
        slot: Pc4GenerationRegistrySlot,
    ) -> Option<Pc4GenerationRegistryError>
    where
        I: IntoIterator<Item = &'a Arc<ActivatedSnapshot>>,
    {
        existing.into_iter().find_map(|snapshot| {
            let existing = snapshot.qualified_identity();
            let drift = if existing == candidate {
                None
            } else if existing.snapshot_identity() == candidate.snapshot_identity() {
                Some(Pc4GenerationIdentityDrift::QualificationForSnapshot)
            } else if same_generation_label(
                existing.snapshot_identity(),
                candidate.snapshot_identity(),
            ) {
                Some(Pc4GenerationIdentityDrift::RevisionForGenerationLabel)
            } else {
                None
            };
            drift.map(|drift| Pc4GenerationRegistryError::IdentityDrift { slot, drift })
        })
    }
}

fn same_generation_label(left: &SnapshotIdentity, right: &SnapshotIdentity) -> bool {
    left.repository() == right.repository() && left.generation() == right.generation()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::{
        activated_snapshot_for_generation, activated_snapshot_for_revision_generation,
        SYNTHETIC_REVISION_B,
    };

    fn retention(value: u8) -> Pc4GenerationRetentionLimit {
        Pc4GenerationRetentionLimit::new(value).expect("valid synthetic retention")
    }

    fn activated(generation: &str, manifest: &str) -> Arc<ActivatedSnapshot> {
        Arc::new(activated_snapshot_for_generation(generation, manifest))
    }

    fn activated_at_revision(
        revision: &str,
        generation: &str,
        manifest: &str,
    ) -> Arc<ActivatedSnapshot> {
        Arc::new(activated_snapshot_for_revision_generation(
            revision, generation, manifest,
        ))
    }

    fn stage_and_promote(
        registry: &mut Pc4GenerationRegistry,
        generation: Arc<ActivatedSnapshot>,
    ) -> Pc4GenerationPromotionOutcome {
        let token = match registry
            .stage(registry.version(), generation)
            .expect("stage qualified generation")
        {
            Pc4GenerationStageOutcome::Staged { token, .. } => token,
            unexpected => panic!("expected a new staged generation, got {unexpected:?}"),
        };
        registry.promote(&token).expect("promote staged generation")
    }

    #[test]
    fn retention_limit_is_explicitly_bounded() {
        for invalid in [0, MAX_RETAINED_PC4_GENERATIONS + 1, u8::MAX] {
            assert_eq!(
                Pc4GenerationRetentionLimit::new(invalid),
                Err(Pc4GenerationRetentionLimitError { actual: invalid })
            );
        }
        assert_eq!(retention(1).get(), 1);
        assert_eq!(
            retention(MAX_RETAINED_PC4_GENERATIONS).get(),
            MAX_RETAINED_PC4_GENERATIONS
        );
    }

    #[test]
    fn no_current_and_preparation_failure_do_not_invent_fallback() {
        let registry = Pc4GenerationRegistry::new(retention(2));
        assert_eq!(registry.pin_current(), Pc4CurrentGeneration::NoCurrent);
        assert_eq!(
            registry
                .preserve_after_preparation_failure(Pc4GenerationPreparationFailure::FetchFailed),
            Pc4GenerationFailureOutcome::NoCurrent {
                failure: Pc4GenerationPreparationFailure::FetchFailed
            }
        );
        assert_eq!(registry.version().get(), 0);
    }

    #[test]
    fn stage_then_promote_is_atomic_and_failed_preparation_preserves_current() {
        let mut registry = Pc4GenerationRegistry::new(retention(2));
        let a = activated("generation-a", "manifest-a");
        let b = activated("generation-b", "manifest-b");
        let first = stage_and_promote(&mut registry, Arc::clone(&a));
        assert!(matches!(
            first.retention,
            Pc4GenerationRetentionChange::NoPreviousCurrent
        ));

        let token = match registry
            .stage(registry.version(), Arc::clone(&b))
            .expect("stage b")
        {
            Pc4GenerationStageOutcome::Staged { token, .. } => token,
            unexpected => panic!("expected staged b, got {unexpected:?}"),
        };
        let pinned_a = match registry.pin_current() {
            Pc4CurrentGeneration::Current(current) => current,
            Pc4CurrentGeneration::NoCurrent => panic!("a must remain current while b is staged"),
        };
        assert_eq!(pinned_a.qualified_identity(), a.qualified_identity());
        assert!(matches!(
            registry.preserve_after_preparation_failure(
                Pc4GenerationPreparationFailure::QualificationRejected
            ),
            Pc4GenerationFailureOutcome::CurrentPreserved { .. }
        ));
        assert_eq!(
            registry.pin_current(),
            Pc4CurrentGeneration::Current(pinned_a)
        );

        let promoted = registry.promote(&token).expect("promote b");
        assert_eq!(
            promoted.current.qualified_identity(),
            b.qualified_identity()
        );
        assert!(matches!(
            promoted.retention,
            Pc4GenerationRetentionChange::Retained { .. }
        ));
    }

    #[test]
    fn overlapping_request_pin_survives_promotions_and_registry_eviction() {
        let mut registry = Pc4GenerationRegistry::new(retention(1));
        let a = activated("generation-a", "manifest-a");
        let b = activated("generation-b", "manifest-b");
        let c = activated("generation-c", "manifest-c");
        stage_and_promote(&mut registry, Arc::clone(&a));
        let request_a = match registry.pin_current() {
            Pc4CurrentGeneration::Current(current) => current,
            Pc4CurrentGeneration::NoCurrent => panic!("a is current"),
        };
        stage_and_promote(&mut registry, Arc::clone(&b));
        let promotion_c = stage_and_promote(&mut registry, Arc::clone(&c));
        let evicted = match promotion_c.retention {
            Pc4GenerationRetentionChange::RetainedAndEvicted { evicted, .. } => evicted,
            unexpected => panic!("retention-one promotion must evict a, got {unexpected:?}"),
        };
        assert_eq!(evicted.qualified_identity(), a.qualified_identity());
        assert_eq!(request_a.qualified_identity(), a.qualified_identity());
        assert_eq!(
            request_a.activated_snapshot().manifest_content_identity(),
            a.manifest_content_identity()
        );
        assert_eq!(
            registry.retained_identities().collect::<Vec<_>>(),
            vec![b.qualified_identity()]
        );
    }

    #[test]
    fn overlapping_stagers_deduplicate_exact_identity_and_reject_replacement() {
        let mut registry = Pc4GenerationRegistry::new(retention(2));
        let a = activated("generation-a", "manifest-a");
        let b = activated("generation-b", "manifest-b");
        let first = registry
            .stage(registry.version(), Arc::clone(&a))
            .expect("first stage");
        let first_token = match first {
            Pc4GenerationStageOutcome::Staged { token, .. } => token,
            unexpected => panic!("expected stage, got {unexpected:?}"),
        };
        assert!(matches!(
            registry
                .stage(registry.version(), Arc::clone(&a))
                .expect("exact overlapping stage"),
            Pc4GenerationStageOutcome::AlreadyStaged { token, .. }
                if token == first_token
        ));
        assert_eq!(
            registry.stage(registry.version(), b),
            Err(Pc4GenerationRegistryError::StageSlotOccupied)
        );
        assert_eq!(
            registry
                .staged_generation()
                .expect("a remains staged")
                .qualified_identity(),
            a.qualified_identity()
        );
    }

    #[test]
    fn cancelled_and_stale_promotions_are_transactional() {
        let mut registry = Pc4GenerationRegistry::new(retention(2));
        let a = activated("generation-a", "manifest-a");
        let b = activated("generation-b", "manifest-b");
        stage_and_promote(&mut registry, Arc::clone(&a));
        let token = match registry.stage(registry.version(), b).expect("stage b") {
            Pc4GenerationStageOutcome::Staged { token, .. } => token,
            unexpected => panic!("expected staged b, got {unexpected:?}"),
        };
        let cancelled_token = token;
        registry.cancel_staged(&token).expect("cancel b");
        let version_after_cancel = registry.version();
        assert_eq!(
            registry.promote(&cancelled_token),
            Err(Pc4GenerationRegistryError::StageClosed {
                disposition: Pc4ClosedStageDisposition::Cancelled
            })
        );
        assert_eq!(registry.version(), version_after_cancel);
        assert_eq!(
            registry
                .pin_current()
                .current_identity()
                .expect("a remains current"),
            a.qualified_identity()
        );

        let stale = Pc4GenerationRegistryVersion(0);
        assert!(matches!(
            registry.stage(stale, activated("generation-c", "manifest-c")),
            Err(Pc4GenerationRegistryError::StaleWrite { .. })
        ));
        assert_eq!(registry.version(), version_after_cancel);
    }

    #[test]
    fn retention_and_rollback_preserve_exact_generations() {
        let mut registry = Pc4GenerationRegistry::new(retention(2));
        let a = activated("generation-a", "manifest-a");
        let b = activated("generation-b", "manifest-b");
        let c = activated("generation-c", "manifest-c");
        stage_and_promote(&mut registry, Arc::clone(&a));
        stage_and_promote(&mut registry, Arc::clone(&b));
        stage_and_promote(&mut registry, Arc::clone(&c));
        assert_eq!(
            registry.retained_identities().collect::<Vec<_>>(),
            vec![b.qualified_identity(), a.qualified_identity()]
        );

        let rolled_back = registry
            .rollback(registry.version(), a.qualified_identity())
            .expect("rollback to a");
        assert!(matches!(
            rolled_back,
            Pc4GenerationRollbackOutcome::RolledBack { ref current, .. }
                if current.qualified_identity() == a.qualified_identity()
        ));
        assert_eq!(
            registry.retained_identities().collect::<Vec<_>>(),
            vec![c.qualified_identity(), b.qualified_identity()]
        );
        assert!(matches!(
            registry
                .stage(registry.version(), Arc::clone(&c))
                .expect("exact retained generation is reported"),
            Pc4GenerationStageOutcome::Retained(ref retained)
                if retained.qualified_identity() == c.qualified_identity()
        ));
    }

    #[test]
    fn failed_rollback_paths_leave_current_and_retention_unchanged() {
        let mut registry = Pc4GenerationRegistry::new(retention(2));
        let a = activated("generation-a", "manifest-a");
        let b = activated("generation-b", "manifest-b");
        let missing = activated("generation-missing", "manifest-missing");
        stage_and_promote(&mut registry, Arc::clone(&a));
        stage_and_promote(&mut registry, Arc::clone(&b));
        let version = registry.version();
        let retained_before = registry.retained_identities().cloned().collect::<Vec<_>>();
        assert_eq!(
            registry.rollback(registry.version(), missing.qualified_identity()),
            Err(Pc4GenerationRegistryError::RetainedGenerationNotFound)
        );
        assert_eq!(registry.version(), version);
        assert_eq!(
            registry.retained_identities().collect::<Vec<_>>(),
            retained_before.iter().collect::<Vec<_>>()
        );
        assert_eq!(
            registry
                .pin_current()
                .current_identity()
                .expect("b remains current"),
            b.qualified_identity()
        );

        let staged = activated("generation-c", "manifest-c");
        let _token = match registry
            .stage(registry.version(), Arc::clone(&staged))
            .expect("stage c")
        {
            Pc4GenerationStageOutcome::Staged { token, .. } => token,
            unexpected => panic!("expected staged c, got {unexpected:?}"),
        };
        let staged_version = registry.version();
        assert_eq!(
            registry.rollback(registry.version(), staged.qualified_identity()),
            Err(Pc4GenerationRegistryError::RollbackTargetIsStaged)
        );
        assert_eq!(registry.version(), staged_version);
        assert_eq!(
            registry
                .pin_current()
                .current_identity()
                .expect("b remains current while c is staged"),
            b.qualified_identity()
        );
    }

    #[test]
    fn exact_collisions_are_observed_and_identity_drift_fails_closed() {
        let mut registry = Pc4GenerationRegistry::new(retention(2));
        let a = activated("generation-a", "manifest-a");
        stage_and_promote(&mut registry, Arc::clone(&a));
        assert!(matches!(
            registry
                .stage(registry.version(), Arc::clone(&a))
                .expect("exact current is idempotent"),
            Pc4GenerationStageOutcome::Current(_)
        ));
        let drift = activated("generation-a", "manifest-a-drift");
        assert_eq!(
            registry.stage(registry.version(), drift),
            Err(Pc4GenerationRegistryError::IdentityDrift {
                slot: Pc4GenerationRegistrySlot::Current,
                drift: Pc4GenerationIdentityDrift::QualificationForSnapshot,
            })
        );
        assert_eq!(
            registry
                .pin_current()
                .current_identity()
                .expect("current survives drift"),
            a.qualified_identity()
        );

        let revision_drift = activated_at_revision(
            SYNTHETIC_REVISION_B,
            "generation-a",
            "manifest-other-revision",
        );
        assert_eq!(
            registry.stage(registry.version(), revision_drift),
            Err(Pc4GenerationRegistryError::IdentityDrift {
                slot: Pc4GenerationRegistrySlot::Current,
                drift: Pc4GenerationIdentityDrift::RevisionForGenerationLabel,
            })
        );
        assert_eq!(
            registry
                .pin_current()
                .current_identity()
                .expect("revision drift cannot replace current"),
            a.qualified_identity()
        );
    }

    trait CurrentIdentityExt {
        fn current_identity(&self) -> Option<&QualifiedSnapshotIdentity>;
    }

    impl CurrentIdentityExt for Pc4CurrentGeneration {
        fn current_identity(&self) -> Option<&QualifiedSnapshotIdentity> {
            match self {
                Pc4CurrentGeneration::NoCurrent => None,
                Pc4CurrentGeneration::Current(current) => Some(current.qualified_identity()),
            }
        }
    }
}
