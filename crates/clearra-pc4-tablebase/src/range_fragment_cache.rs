// SRP rationale: this module owns only bounded in-memory reuse of one exact,
// already-qualified Range fragment. Transport, dataset discovery, parsing,
// activation, and product fallback remain in their respective host layers.
use core::{fmt, num::NonZeroUsize};
use std::collections::VecDeque;

use crate::{
    ArtifactDescriptor, Pc4RuleProfile, QualifiedSnapshotIdentity, RangeRequest, RangeResponse,
    RangeResponseKind,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RangeFragmentCacheLimits {
    max_entries: NonZeroUsize,
    max_total_bytes: NonZeroUsize,
    max_fragment_bytes: NonZeroUsize,
}

impl RangeFragmentCacheLimits {
    pub const fn new(
        max_entries: NonZeroUsize,
        max_total_bytes: NonZeroUsize,
        max_fragment_bytes: NonZeroUsize,
    ) -> Self {
        Self {
            max_entries,
            max_total_bytes,
            max_fragment_bytes,
        }
    }

    pub const fn max_entries(self) -> usize {
        self.max_entries.get()
    }

    pub const fn max_total_bytes(self) -> usize {
        self.max_total_bytes.get()
    }

    pub const fn max_fragment_bytes(self) -> usize {
        self.max_fragment_bytes.get()
    }
}

/// Host-owned cancellation observation for one cache transaction.
///
/// Implementations should remain monotonic for the duration of a call.
pub trait RangeFragmentCacheGuard {
    fn is_cancelled(&self) -> bool;
}

impl<F> RangeFragmentCacheGuard for F
where
    F: Fn() -> bool,
{
    fn is_cancelled(&self) -> bool {
        self()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeFragmentResponseBinding {
    LookupSession,
    RequestId,
    Snapshot,
    Profile,
    ArtifactRole,
    ArtifactContentIdentity,
    ResponseKind,
    Offset,
    CompleteLength,
    BodyLength,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeFragmentCacheBudgetKind {
    FragmentBytes,
    TotalBytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RangeFragmentCacheError {
    Cancelled,
    ZeroLengthRange,
    RangeEndOverflow,
    RangeOutsideArtifact {
        end_exclusive: u64,
        artifact_length: u64,
    },
    ResponseBindingDrift {
        binding: RangeFragmentResponseBinding,
    },
    BudgetExceeded {
        kind: RangeFragmentCacheBudgetKind,
        limit: usize,
        actual: usize,
    },
    ConflictingFragment,
    AccountingOverflow,
    AllocationFailed,
}

impl RangeFragmentCacheError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_range_cache_cancelled",
            Self::ZeroLengthRange => "pc4_range_cache_zero_length_range",
            Self::RangeEndOverflow => "pc4_range_cache_range_end_overflow",
            Self::RangeOutsideArtifact { .. } => "pc4_range_cache_range_outside_artifact",
            Self::ResponseBindingDrift { .. } => "pc4_range_cache_response_binding_drift",
            Self::BudgetExceeded { .. } => "pc4_range_cache_budget_exceeded",
            Self::ConflictingFragment => "pc4_range_cache_conflicting_fragment",
            Self::AccountingOverflow => "pc4_range_cache_accounting_overflow",
            Self::AllocationFailed => "pc4_range_cache_allocation_failed",
        }
    }
}

impl fmt::Display for RangeFragmentCacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for RangeFragmentCacheError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeFragmentCacheInsert {
    AlreadyPresent,
    Inserted {
        evicted_entries: usize,
        evicted_bytes: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RangeFragmentKey {
    snapshot: QualifiedSnapshotIdentity,
    profile: Pc4RuleProfile,
    artifact: ArtifactDescriptor,
    offset: u64,
    length: u32,
}

impl RangeFragmentKey {
    fn from_request(request: &RangeRequest) -> Self {
        Self {
            snapshot: request.snapshot().clone(),
            profile: request.profile(),
            artifact: request.artifact_descriptor().clone(),
            offset: request.offset(),
            length: request.length(),
        }
    }

    fn matches(&self, request: &RangeRequest) -> bool {
        self.snapshot == *request.snapshot()
            && self.profile == request.profile()
            && self.artifact == *request.artifact_descriptor()
            && self.offset == request.offset()
            && self.length == request.length()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RangeFragmentEntry {
    key: RangeFragmentKey,
    bytes: Vec<u8>,
}

/// Bounded FIFO cache for exact immutable Range fragments.
///
/// FIFO order is insertion order and an identical re-insert does not refresh
/// it. Entries never satisfy an overlapping or containing range. Lookup
/// session and request IDs are deliberately not cached: a hit is rebound to
/// the exact current request, and the regular lookup boundary still rejects a
/// response if a caller mixes that rebound response into another request.
#[derive(Clone, Debug)]
pub struct RangeFragmentCache {
    limits: RangeFragmentCacheLimits,
    entries: VecDeque<RangeFragmentEntry>,
    total_bytes: usize,
}

impl RangeFragmentCache {
    pub fn new(limits: RangeFragmentCacheLimits) -> Self {
        Self {
            limits,
            entries: VecDeque::new(),
            total_bytes: 0,
        }
    }

    pub const fn limits(&self) -> RangeFragmentCacheLimits {
        self.limits
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub const fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Returns an exact hit rebound to `request`'s live session/request IDs.
    /// A miss is not permission to widen or slice a different cached range.
    pub fn lookup<G>(
        &self,
        request: &RangeRequest,
        guard: &G,
    ) -> Result<Option<RangeResponse>, RangeFragmentCacheError>
    where
        G: RangeFragmentCacheGuard + ?Sized,
    {
        ensure_not_cancelled(guard)?;
        validate_request(request)?;

        let Some(entry) = self.entries.iter().find(|entry| entry.key.matches(request)) else {
            ensure_not_cancelled(guard)?;
            return Ok(None);
        };

        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(entry.bytes.len())
            .map_err(|_| RangeFragmentCacheError::AllocationFailed)?;
        bytes.extend_from_slice(&entry.bytes);
        let response = RangeResponse {
            lookup_session: request.lookup_session(),
            request_id: request.request_id(),
            snapshot: request.snapshot().clone(),
            profile: request.profile(),
            artifact: request.artifact(),
            artifact_content_identity: request.artifact_descriptor().content_identity().to_owned(),
            kind: RangeResponseKind::PartialContent,
            offset: request.offset(),
            complete_length: request.artifact_descriptor().byte_len(),
            bytes,
        };
        ensure_not_cancelled(guard)?;
        Ok(Some(response))
    }

    /// Validates and stores one successful exact partial response.
    ///
    /// Validation, allocation, eviction planning, and the final cancellation
    /// check complete before any cache state changes. A conflicting byte body
    /// under the same qualified identity is rejected rather than overwritten.
    pub fn insert<G>(
        &mut self,
        request: &RangeRequest,
        response: &RangeResponse,
        guard: &G,
    ) -> Result<RangeFragmentCacheInsert, RangeFragmentCacheError>
    where
        G: RangeFragmentCacheGuard + ?Sized,
    {
        ensure_not_cancelled(guard)?;
        validate_request(request)?;
        validate_response(request, response)?;

        if let Some(existing) = self.entries.iter().find(|entry| entry.key.matches(request)) {
            ensure_not_cancelled(guard)?;
            return if existing.bytes == response.bytes {
                Ok(RangeFragmentCacheInsert::AlreadyPresent)
            } else {
                Err(RangeFragmentCacheError::ConflictingFragment)
            };
        }

        let fragment_bytes = response.bytes.len();
        check_budget(
            RangeFragmentCacheBudgetKind::FragmentBytes,
            self.limits.max_fragment_bytes(),
            fragment_bytes,
        )?;
        check_budget(
            RangeFragmentCacheBudgetKind::TotalBytes,
            self.limits.max_total_bytes(),
            fragment_bytes,
        )?;

        let mut staged_bytes = Vec::new();
        staged_bytes
            .try_reserve_exact(fragment_bytes)
            .map_err(|_| RangeFragmentCacheError::AllocationFailed)?;
        staged_bytes.extend_from_slice(&response.bytes);
        let staged_key = RangeFragmentKey::from_request(request);

        let mut evicted_entries = 0_usize;
        let mut evicted_bytes = 0_usize;
        while self
            .entries
            .len()
            .checked_sub(evicted_entries)
            .and_then(|remaining| remaining.checked_add(1))
            .ok_or(RangeFragmentCacheError::AccountingOverflow)?
            > self.limits.max_entries()
            || self
                .total_bytes
                .checked_sub(evicted_bytes)
                .and_then(|remaining| remaining.checked_add(fragment_bytes))
                .ok_or(RangeFragmentCacheError::AccountingOverflow)?
                > self.limits.max_total_bytes()
        {
            let entry = self
                .entries
                .get(evicted_entries)
                .ok_or(RangeFragmentCacheError::AccountingOverflow)?;
            evicted_bytes = evicted_bytes
                .checked_add(entry.bytes.len())
                .ok_or(RangeFragmentCacheError::AccountingOverflow)?;
            evicted_entries = evicted_entries
                .checked_add(1)
                .ok_or(RangeFragmentCacheError::AccountingOverflow)?;
        }

        let next_total_bytes = self
            .total_bytes
            .checked_sub(evicted_bytes)
            .and_then(|remaining| remaining.checked_add(fragment_bytes))
            .ok_or(RangeFragmentCacheError::AccountingOverflow)?;
        if evicted_entries == 0 {
            self.entries
                .try_reserve(1)
                .map_err(|_| RangeFragmentCacheError::AllocationFailed)?;
        }
        ensure_not_cancelled(guard)?;

        for _ in 0..evicted_entries {
            self.entries
                .pop_front()
                .expect("prevalidated FIFO eviction count");
        }
        self.entries.push_back(RangeFragmentEntry {
            key: staged_key,
            bytes: staged_bytes,
        });
        self.total_bytes = next_total_bytes;

        Ok(RangeFragmentCacheInsert::Inserted {
            evicted_entries,
            evicted_bytes,
        })
    }
}

fn ensure_not_cancelled<G>(guard: &G) -> Result<(), RangeFragmentCacheError>
where
    G: RangeFragmentCacheGuard + ?Sized,
{
    if guard.is_cancelled() {
        Err(RangeFragmentCacheError::Cancelled)
    } else {
        Ok(())
    }
}

fn validate_request(request: &RangeRequest) -> Result<(), RangeFragmentCacheError> {
    if request.length() == 0 {
        return Err(RangeFragmentCacheError::ZeroLengthRange);
    }
    let end_exclusive = request
        .offset()
        .checked_add(u64::from(request.length()))
        .ok_or(RangeFragmentCacheError::RangeEndOverflow)?;
    let artifact_length = request.artifact_descriptor().byte_len();
    if end_exclusive > artifact_length {
        return Err(RangeFragmentCacheError::RangeOutsideArtifact {
            end_exclusive,
            artifact_length,
        });
    }
    Ok(())
}

fn validate_response(
    request: &RangeRequest,
    response: &RangeResponse,
) -> Result<(), RangeFragmentCacheError> {
    let binding = if response.lookup_session != request.lookup_session() {
        Some(RangeFragmentResponseBinding::LookupSession)
    } else if response.request_id != request.request_id() {
        Some(RangeFragmentResponseBinding::RequestId)
    } else if response.snapshot != *request.snapshot() {
        Some(RangeFragmentResponseBinding::Snapshot)
    } else if response.profile != request.profile() {
        Some(RangeFragmentResponseBinding::Profile)
    } else if response.artifact != request.artifact() {
        Some(RangeFragmentResponseBinding::ArtifactRole)
    } else if response.artifact_content_identity != request.artifact_descriptor().content_identity()
    {
        Some(RangeFragmentResponseBinding::ArtifactContentIdentity)
    } else if response.kind != RangeResponseKind::PartialContent {
        Some(RangeFragmentResponseBinding::ResponseKind)
    } else if response.offset != request.offset() {
        Some(RangeFragmentResponseBinding::Offset)
    } else if response.complete_length != request.artifact_descriptor().byte_len() {
        Some(RangeFragmentResponseBinding::CompleteLength)
    } else if response.bytes.len() != request.length() as usize {
        Some(RangeFragmentResponseBinding::BodyLength)
    } else {
        None
    };
    match binding {
        Some(binding) => Err(RangeFragmentCacheError::ResponseBindingDrift { binding }),
        None => Ok(()),
    }
}

fn check_budget(
    kind: RangeFragmentCacheBudgetKind,
    limit: usize,
    actual: usize,
) -> Result<(), RangeFragmentCacheError> {
    if actual > limit {
        Err(RangeFragmentCacheError::BudgetExceeded {
            kind,
            limit,
            actual,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::{
        manifest::tests::{activated_snapshot, qualified_snapshot_identity},
        LookupMachine, LookupSessionId, LookupStep, Pc4ArtifactRole, SupplyError,
    };

    fn nonzero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).expect("non-zero cache test limit")
    }

    fn limits(entries: usize, total: usize, fragment: usize) -> RangeFragmentCacheLimits {
        RangeFragmentCacheLimits::new(nonzero(entries), nonzero(total), nonzero(fragment))
    }

    fn session(value: u64) -> LookupSessionId {
        LookupSessionId::new(value).expect("non-zero lookup session")
    }

    fn request(
        snapshot: QualifiedSnapshotIdentity,
        profile: Pc4RuleProfile,
        artifact: ArtifactDescriptor,
        session_id: u64,
        request_id: u64,
        offset: u64,
        length: u32,
    ) -> RangeRequest {
        RangeRequest::new(
            session(session_id),
            request_id,
            snapshot,
            profile,
            artifact,
            offset,
            length,
        )
    }

    fn response(request: &RangeRequest, bytes: &[u8]) -> RangeResponse {
        RangeResponse {
            lookup_session: request.lookup_session(),
            request_id: request.request_id(),
            snapshot: request.snapshot().clone(),
            profile: request.profile(),
            artifact: request.artifact(),
            artifact_content_identity: request.artifact_descriptor().content_identity().to_owned(),
            kind: RangeResponseKind::PartialContent,
            offset: request.offset(),
            complete_length: request.artifact_descriptor().byte_len(),
            bytes: bytes.to_vec(),
        }
    }

    fn fixture_request(offset: u64, length: u32) -> RangeRequest {
        let activated = activated_snapshot(2, 32);
        request(
            activated.qualified_identity().clone(),
            Pc4RuleProfile::Srs,
            activated
                .profile(Pc4RuleProfile::Srs)
                .field_hash_index()
                .clone(),
            1,
            7,
            offset,
            length,
        )
    }

    fn lookup_hit(cache: &RangeFragmentCache, request: &RangeRequest) -> Option<RangeResponse> {
        cache.lookup(request, &|| false).expect("cache lookup")
    }

    #[test]
    fn warm_exact_hit_rebinds_live_session_and_stale_mixing_fails_closed() {
        let activated = activated_snapshot(2, 32);
        let mut first = LookupMachine::start(&activated, Pc4RuleProfile::Srs, 0, session(1))
            .expect("first lookup");
        let mut second = LookupMachine::start(&activated, Pc4RuleProfile::Srs, 0, session(2))
            .expect("second lookup");
        let LookupStep::NeedRange(first_request) = first.step() else {
            panic!("first lookup requires a range");
        };
        let LookupStep::NeedRange(second_request) = second.step() else {
            panic!("second lookup requires a range");
        };
        let mut header = b"FHIDIDX1".to_vec();
        header.extend_from_slice(&1_u32.to_le_bytes());
        header.extend_from_slice(&2_u32.to_le_bytes());

        let mut cache = RangeFragmentCache::new(limits(4, 128, 64));
        assert!(matches!(
            cache.insert(&first_request, &response(&first_request, &header), &|| {
                false
            }),
            Ok(RangeFragmentCacheInsert::Inserted { .. })
        ));
        let first_hit = lookup_hit(&cache, &first_request).expect("warm first hit");
        assert_eq!(
            second.supply(first_hit),
            Err(SupplyError::LookupSessionMismatch)
        );

        let second_hit = lookup_hit(&cache, &second_request).expect("warm second-session hit");
        assert_eq!(second_hit.lookup_session, session(2));
        assert_eq!(second_hit.request_id, second_request.request_id());
        let stale_request = request(
            second_request.snapshot().clone(),
            second_request.profile(),
            second_request.artifact_descriptor().clone(),
            second_request.lookup_session().get(),
            second_request.request_id() + 1,
            second_request.offset(),
            second_request.length(),
        );
        let stale_hit = lookup_hit(&cache, &stale_request).expect("stale-id cache hit");
        assert_eq!(
            second.supply(stale_hit),
            Err(SupplyError::StaleRequest {
                expected: second_request.request_id(),
                actual: stale_request.request_id(),
            })
        );
        second
            .supply(second_hit)
            .expect("response rebound to second request");
        first.cancel();
    }

    #[test]
    fn generation_manifest_profile_artifact_content_and_range_drift_are_misses() {
        let base = fixture_request(0, 4);
        let mut cache = RangeFragmentCache::new(limits(8, 128, 32));
        cache
            .insert(&base, &response(&base, &[1, 2, 3, 4]), &|| false)
            .expect("seed cache");

        let changed_generation = request(
            qualified_snapshot_identity("generation-b", "synthetic-manifest:2:32"),
            base.profile(),
            base.artifact_descriptor().clone(),
            2,
            1,
            0,
            4,
        );
        let changed_manifest = request(
            qualified_snapshot_identity("generation-a", "manifest-b"),
            base.profile(),
            base.artifact_descriptor().clone(),
            2,
            1,
            0,
            4,
        );
        let changed_profile = request(
            base.snapshot().clone(),
            Pc4RuleProfile::SrsX,
            base.artifact_descriptor().clone(),
            2,
            1,
            0,
            4,
        );
        let changed_artifact = request(
            base.snapshot().clone(),
            base.profile(),
            ArtifactDescriptor::new(Pc4ArtifactRole::Graph, "other.bin", 32, "content-a")
                .expect("other artifact"),
            2,
            1,
            0,
            4,
        );
        let changed_content = request(
            base.snapshot().clone(),
            base.profile(),
            ArtifactDescriptor::new(
                base.artifact(),
                base.artifact_descriptor().path(),
                base.artifact_descriptor().byte_len(),
                "different-content",
            )
            .expect("content-drift artifact"),
            2,
            1,
            0,
            4,
        );
        let overlapping = request(
            base.snapshot().clone(),
            base.profile(),
            base.artifact_descriptor().clone(),
            2,
            1,
            1,
            2,
        );
        let prefix = request(
            base.snapshot().clone(),
            base.profile(),
            base.artifact_descriptor().clone(),
            2,
            1,
            0,
            2,
        );

        for drifted in [
            changed_generation,
            changed_manifest,
            changed_profile,
            changed_artifact,
            changed_content,
            overlapping,
            prefix,
        ] {
            assert_eq!(lookup_hit(&cache, &drifted), None);
        }
        assert_eq!(
            lookup_hit(&cache, &base).expect("base hit").bytes,
            [1, 2, 3, 4]
        );
    }

    #[test]
    fn response_length_content_and_other_binding_drift_are_rejected_transactionally() {
        let request = fixture_request(2, 3);
        let base = response(&request, &[1, 2, 3]);
        let cases = [
            (
                RangeFragmentResponseBinding::LookupSession,
                RangeResponse {
                    lookup_session: session(9),
                    ..base.clone()
                },
            ),
            (
                RangeFragmentResponseBinding::RequestId,
                RangeResponse {
                    request_id: 99,
                    ..base.clone()
                },
            ),
            (
                RangeFragmentResponseBinding::Snapshot,
                RangeResponse {
                    snapshot: qualified_snapshot_identity("other-generation", "other-manifest"),
                    ..base.clone()
                },
            ),
            (
                RangeFragmentResponseBinding::Profile,
                RangeResponse {
                    profile: Pc4RuleProfile::SrsX,
                    ..base.clone()
                },
            ),
            (
                RangeFragmentResponseBinding::ArtifactRole,
                RangeResponse {
                    artifact: Pc4ArtifactRole::Graph,
                    ..base.clone()
                },
            ),
            (
                RangeFragmentResponseBinding::ArtifactContentIdentity,
                RangeResponse {
                    artifact_content_identity: "drift".to_owned(),
                    ..base.clone()
                },
            ),
            (
                RangeFragmentResponseBinding::ResponseKind,
                RangeResponse {
                    kind: RangeResponseKind::WholeContent,
                    ..base.clone()
                },
            ),
            (
                RangeFragmentResponseBinding::Offset,
                RangeResponse {
                    offset: 1,
                    ..base.clone()
                },
            ),
            (
                RangeFragmentResponseBinding::CompleteLength,
                RangeResponse {
                    complete_length: request.artifact_descriptor().byte_len() - 1,
                    ..base.clone()
                },
            ),
            (
                RangeFragmentResponseBinding::BodyLength,
                RangeResponse {
                    bytes: vec![1, 2],
                    ..base.clone()
                },
            ),
        ];

        let mut cache = RangeFragmentCache::new(limits(4, 64, 16));
        for (binding, drifted) in cases {
            assert_eq!(
                cache.insert(&request, &drifted, &|| false),
                Err(RangeFragmentCacheError::ResponseBindingDrift { binding })
            );
            assert_eq!(cache.entry_count(), 0);
            assert_eq!(cache.total_bytes(), 0);
        }
    }

    #[test]
    fn fragment_and_total_budgets_refuse_without_mutation() {
        let request = fixture_request(0, 4);
        let mut fragment_limited = RangeFragmentCache::new(limits(2, 8, 3));
        assert_eq!(
            fragment_limited.insert(&request, &response(&request, &[0; 4]), &|| false),
            Err(RangeFragmentCacheError::BudgetExceeded {
                kind: RangeFragmentCacheBudgetKind::FragmentBytes,
                limit: 3,
                actual: 4,
            })
        );
        assert_eq!(fragment_limited.entry_count(), 0);

        let limits = RangeFragmentCacheLimits::new(nonzero(2), nonzero(3), nonzero(4));
        let mut total_limited = RangeFragmentCache::new(limits);
        assert_eq!(
            total_limited.insert(&request, &response(&request, &[0; 4]), &|| false),
            Err(RangeFragmentCacheError::BudgetExceeded {
                kind: RangeFragmentCacheBudgetKind::TotalBytes,
                limit: 3,
                actual: 4,
            })
        );
        assert_eq!(total_limited.entry_count(), 0);
    }

    #[test]
    fn fifo_eviction_is_deterministic_for_entry_and_byte_limits() {
        let base = fixture_request(0, 2);
        let second = request(
            base.snapshot().clone(),
            base.profile(),
            base.artifact_descriptor().clone(),
            1,
            8,
            2,
            2,
        );
        let third = request(
            base.snapshot().clone(),
            base.profile(),
            base.artifact_descriptor().clone(),
            1,
            9,
            4,
            3,
        );
        let mut entry_limited = RangeFragmentCache::new(limits(2, 16, 3));
        entry_limited
            .insert(&base, &response(&base, &[1, 2]), &|| false)
            .expect("first fragment");
        entry_limited
            .insert(&second, &response(&second, &[3, 4]), &|| false)
            .expect("second fragment");
        assert_eq!(
            entry_limited.insert(&third, &response(&third, &[5, 6, 7]), &|| false),
            Ok(RangeFragmentCacheInsert::Inserted {
                evicted_entries: 1,
                evicted_bytes: 2,
            })
        );
        assert_eq!(entry_limited.entry_count(), 2);
        assert_eq!(entry_limited.total_bytes(), 5);
        assert_eq!(lookup_hit(&entry_limited, &base), None);
        assert!(lookup_hit(&entry_limited, &second).is_some());
        assert!(lookup_hit(&entry_limited, &third).is_some());

        let mut byte_limited = RangeFragmentCache::new(limits(3, 5, 3));
        byte_limited
            .insert(&base, &response(&base, &[1, 2]), &|| false)
            .expect("first fragment");
        byte_limited
            .insert(&second, &response(&second, &[3, 4]), &|| false)
            .expect("second fragment");
        assert_eq!(
            byte_limited.insert(&third, &response(&third, &[5, 6, 7]), &|| false),
            Ok(RangeFragmentCacheInsert::Inserted {
                evicted_entries: 1,
                evicted_bytes: 2,
            })
        );
        assert_eq!(byte_limited.entry_count(), 2);
        assert_eq!(byte_limited.total_bytes(), 5);
        assert_eq!(lookup_hit(&byte_limited, &base), None);
        assert!(lookup_hit(&byte_limited, &second).is_some());
        assert!(lookup_hit(&byte_limited, &third).is_some());
    }

    #[test]
    fn conflicting_bytes_do_not_overwrite_an_exact_identity() {
        let request = fixture_request(0, 3);
        let mut cache = RangeFragmentCache::new(limits(2, 16, 8));
        cache
            .insert(&request, &response(&request, &[1, 2, 3]), &|| false)
            .expect("first fragment");
        assert_eq!(
            cache.insert(&request, &response(&request, &[9, 9, 9]), &|| false),
            Err(RangeFragmentCacheError::ConflictingFragment)
        );
        assert_eq!(
            lookup_hit(&cache, &request)
                .expect("original fragment")
                .bytes,
            [1, 2, 3]
        );
        assert_eq!(
            cache.insert(&request, &response(&request, &[1, 2, 3]), &|| false),
            Ok(RangeFragmentCacheInsert::AlreadyPresent)
        );
    }

    #[test]
    fn invalid_range_arithmetic_fails_before_cache_access() {
        let snapshot = qualified_snapshot_identity("overflow-generation", "overflow-manifest");
        let artifact = ArtifactDescriptor::new(
            Pc4ArtifactRole::Graph,
            "graph.bin",
            u64::MAX,
            "graph-content",
        )
        .expect("large synthetic artifact");
        let overflow = request(snapshot, Pc4RuleProfile::Srs, artifact, 1, 1, u64::MAX, 1);
        let cache = RangeFragmentCache::new(limits(1, 8, 8));
        assert_eq!(
            cache.lookup(&overflow, &|| false),
            Err(RangeFragmentCacheError::RangeEndOverflow)
        );
        let mut cache = cache;
        assert_eq!(
            cache.insert(&overflow, &response(&overflow, &[1]), &|| false),
            Err(RangeFragmentCacheError::RangeEndOverflow)
        );
        assert_eq!(cache.entry_count(), 0);
    }

    #[test]
    fn cancellation_before_commit_or_after_hit_copy_is_transactional() {
        let original = fixture_request(0, 4);
        let replacement = request(
            original.snapshot().clone(),
            original.profile(),
            original.artifact_descriptor().clone(),
            1,
            8,
            4,
            4,
        );
        let mut cache = RangeFragmentCache::new(limits(1, 16, 8));
        cache
            .insert(&original, &response(&original, &[1, 2, 3, 4]), &|| false)
            .expect("seed fragment");
        let insert_checks = Cell::new(0_u8);
        let cancel_on_final_insert_check = || {
            let next = insert_checks.get() + 1;
            insert_checks.set(next);
            next >= 2
        };
        assert_eq!(
            cache.insert(
                &replacement,
                &response(&replacement, &[5, 6, 7, 8]),
                &cancel_on_final_insert_check,
            ),
            Err(RangeFragmentCacheError::Cancelled)
        );
        assert_eq!(cache.entry_count(), 1);
        assert_eq!(cache.total_bytes(), 4);
        assert_eq!(
            lookup_hit(&cache, &original).expect("seed survives").bytes,
            [1, 2, 3, 4]
        );
        assert_eq!(lookup_hit(&cache, &replacement), None);

        let lookup_checks = Cell::new(0_u8);
        let cancel_after_copy = || {
            let next = lookup_checks.get() + 1;
            lookup_checks.set(next);
            next >= 2
        };
        assert_eq!(
            cache.lookup(&original, &cancel_after_copy),
            Err(RangeFragmentCacheError::Cancelled)
        );
        assert_eq!(cache.entry_count(), 1);
        assert_eq!(cache.total_bytes(), 4);
    }
}
