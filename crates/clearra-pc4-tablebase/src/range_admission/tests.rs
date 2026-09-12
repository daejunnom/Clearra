use std::cell::Cell;

use super::*;
use crate::{
    manifest::tests::{activated_snapshot, qualified_snapshot_identity},
    ArtifactDescriptor, LookupMachine, LookupStep, Pc4ArtifactRole, Pc4RuleProfile,
};

#[derive(Clone)]
struct Guard {
    current: QualifiedSnapshotIdentity,
    cancelled: bool,
}

impl RangeAdmissionGuard for Guard {
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool {
        &self.current == expected
    }
}

fn nonzero_u16(value: u16) -> NonZeroU16 {
    NonZeroU16::new(value).expect("non-zero u16 test value")
}

fn nonzero_u32(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).expect("non-zero u32 test value")
}

fn nonzero_u64(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).expect("non-zero u64 test value")
}

fn limits(response: u64, session: u64, requests: u32, active: u16) -> RangeAdmissionLimits {
    RangeAdmissionLimits::new(
        nonzero_u64(response),
        nonzero_u64(session),
        nonzero_u32(requests),
        nonzero_u16(active),
        60,
    )
}

fn session_id(value: u64) -> LookupSessionId {
    LookupSessionId::new(value).expect("non-zero lookup session")
}

fn fixture() -> (RangeRequest, Guard) {
    let activated = activated_snapshot(2, 32);
    let machine = LookupMachine::start(&activated, Pc4RuleProfile::Srs, 0, session_id(7))
        .expect("synthetic lookup");
    let LookupStep::NeedRange(request) = machine.step() else {
        panic!("lookup should request its index header");
    };
    let guard = Guard {
        current: request.snapshot().clone(),
        cancelled: false,
    };
    (request, guard)
}

fn attempt(ordinal: u32, active: u16) -> RangeAdmissionAttempt {
    RangeAdmissionAttempt::new(nonzero_u32(ordinal), nonzero_u16(active))
}

fn response(request: &RangeRequest, bytes: Vec<u8>) -> RangeResponse {
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
        bytes,
    }
}

fn partial_http(request: &RangeRequest, bytes: Vec<u8>) -> RangeHttpResponse {
    let end = request.end_exclusive() - 1;
    RangeHttpResponse::new(
        206,
        Some(format!(
            "bytes {}-{end}/{}",
            request.offset(),
            request.artifact_descriptor().byte_len()
        )),
        None,
        Some(response(request, bytes)),
    )
}

#[test]
fn exact_206_is_admitted_and_accounted_once() {
    let (request, guard) = fixture();
    let mut session = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        limits(32, 64, 4, 3),
    );
    let bytes = vec![9; request.length() as usize];
    assert_eq!(
        session.admit(
            &request,
            attempt(1, 2),
            RangeAdmissionInput::http(partial_http(&request, bytes.clone())),
            &guard,
        ),
        Ok(RangeAdmissionOutcome::PartialContent(response(
            &request, bytes
        )))
    );
    assert_eq!(session.usage().request_count(), 1);
    assert_eq!(
        session.usage().admitted_bytes(),
        u64::from(request.length())
    );
}

#[test]
fn status_200_is_rejected_without_accounting_or_body_acceptance() {
    let (request, guard) = fixture();
    let mut session = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        limits(32, 64, 4, 1),
    );
    let whole = RangeHttpResponse::new(
        200,
        None,
        None,
        Some(RangeResponse {
            kind: RangeResponseKind::WholeContent,
            bytes: vec![7; 4096],
            ..response(&request, Vec::new())
        }),
    );
    assert_eq!(
        session.admit(
            &request,
            attempt(1, 1),
            RangeAdmissionInput::http(whole),
            &guard,
        ),
        Err(RangeAdmissionError::WholeContentRejected)
    );
    assert_eq!(session.usage(), RangeAdmissionUsage::default());
}

#[test]
fn status_416_and_bounded_429_are_typed_terminal_observations() {
    let (request, guard) = fixture();
    let mut session = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        limits(32, 64, 4, 1),
    );
    assert_eq!(
        session.admit(
            &request,
            attempt(1, 1),
            RangeAdmissionInput::http(RangeHttpResponse::new(
                416,
                Some(format!(
                    "bytes */{}",
                    request.artifact_descriptor().byte_len()
                )),
                None,
                None,
            )),
            &guard,
        ),
        Ok(RangeAdmissionOutcome::RangeNotSatisfiable {
            complete_length: request.artifact_descriptor().byte_len(),
        })
    );
    assert_eq!(
        session.admit(
            &request,
            attempt(2, 1),
            RangeAdmissionInput::http(RangeHttpResponse::new(
                429,
                None,
                Some("17".to_owned()),
                None,
            )),
            &guard,
        ),
        Ok(RangeAdmissionOutcome::TransportFailure(
            RangeTransportFailure::RateLimited {
                retry_after_seconds: Some(17),
            }
        ))
    );
    assert_eq!(session.usage().request_count(), 2);
    assert_eq!(session.usage().admitted_bytes(), 0);
}

#[test]
fn transport_failures_are_reported_without_retry_or_fallback() {
    let (request, guard) = fixture();
    for failure in [
        RangeTransportFailure::Offline,
        RangeTransportFailure::Timeout,
        RangeTransportFailure::Unavailable,
        RangeTransportFailure::RateLimited {
            retry_after_seconds: None,
        },
    ] {
        let mut session = RangeAdmissionSession::new(
            request.lookup_session(),
            request.snapshot().clone(),
            limits(32, 64, 1, 1),
        );
        assert_eq!(
            session.admit(
                &request,
                attempt(1, 1),
                RangeAdmissionInput::TransportFailure(failure),
                &guard,
            ),
            Ok(RangeAdmissionOutcome::TransportFailure(failure))
        );
        assert_eq!(session.usage().request_count(), 1);
        assert_eq!(session.usage().admitted_bytes(), 0);
    }
}

#[test]
fn malformed_or_semantically_wrong_content_ranges_do_not_mutate_usage() {
    let (request, guard) = fixture();
    let cases = [
        None,
        Some("octets 0-15/32"),
        Some("bytes 0/32"),
        Some("bytes 15-0/32"),
        Some("bytes */32"),
        Some("bytes 0-15/*"),
        Some("bytes 0-15/32/33"),
    ];
    for value in cases {
        let mut session = RangeAdmissionSession::new(
            request.lookup_session(),
            request.snapshot().clone(),
            limits(32, 64, 1, 1),
        );
        let http = RangeHttpResponse::new(
            206,
            value.map(str::to_owned),
            None,
            Some(response(&request, vec![0; request.length() as usize])),
        );
        assert!(session
            .admit(
                &request,
                attempt(1, 1),
                RangeAdmissionInput::http(http),
                &guard,
            )
            .is_err());
        assert_eq!(session.usage(), RangeAdmissionUsage::default());
    }
}

#[test]
fn every_partial_response_binding_is_checked_transactionally() {
    let (request, guard) = fixture();
    let base = response(&request, vec![0; request.length() as usize]);
    let cases = [
        (
            RangeAdmissionBinding::LookupSession,
            RangeResponse {
                lookup_session: session_id(99),
                ..base.clone()
            },
        ),
        (
            RangeAdmissionBinding::RequestId,
            RangeResponse {
                request_id: 99,
                ..base.clone()
            },
        ),
        (
            RangeAdmissionBinding::Snapshot,
            RangeResponse {
                snapshot: qualified_snapshot_identity("drift", "drift-manifest"),
                ..base.clone()
            },
        ),
        (
            RangeAdmissionBinding::Profile,
            RangeResponse {
                profile: Pc4RuleProfile::SrsX,
                ..base.clone()
            },
        ),
        (
            RangeAdmissionBinding::ArtifactRole,
            RangeResponse {
                artifact: Pc4ArtifactRole::Graph,
                ..base.clone()
            },
        ),
        (
            RangeAdmissionBinding::ArtifactContentIdentity,
            RangeResponse {
                artifact_content_identity: "drift".to_owned(),
                ..base.clone()
            },
        ),
        (
            RangeAdmissionBinding::ResponseKind,
            RangeResponse {
                kind: RangeResponseKind::WholeContent,
                ..base.clone()
            },
        ),
        (
            RangeAdmissionBinding::Offset,
            RangeResponse {
                offset: request.offset() + 1,
                ..base.clone()
            },
        ),
        (
            RangeAdmissionBinding::CompleteLength,
            RangeResponse {
                complete_length: request.artifact_descriptor().byte_len() - 1,
                ..base.clone()
            },
        ),
        (
            RangeAdmissionBinding::BodyLength,
            RangeResponse {
                bytes: vec![0; request.length() as usize - 1],
                ..base
            },
        ),
    ];
    for (binding, drifted) in cases {
        let mut session = RangeAdmissionSession::new(
            request.lookup_session(),
            request.snapshot().clone(),
            limits(32, 64, 1, 1),
        );
        let mut http = partial_http(&request, Vec::new());
        http.response = Some(drifted);
        assert_eq!(
            session.admit(
                &request,
                attempt(1, 1),
                RangeAdmissionInput::http(http),
                &guard,
            ),
            Err(RangeAdmissionError::ResponseBindingDrift { binding })
        );
        assert_eq!(session.usage(), RangeAdmissionUsage::default());
    }

    for (header, binding) in [
        ("bytes 1-15/32", RangeAdmissionBinding::Offset),
        ("bytes 0-14/32", RangeAdmissionBinding::RequestedLength),
        ("bytes 0-15/31", RangeAdmissionBinding::CompleteLength),
    ] {
        let mut session = RangeAdmissionSession::new(
            request.lookup_session(),
            request.snapshot().clone(),
            limits(32, 64, 1, 1),
        );
        let http = RangeHttpResponse::new(
            206,
            Some(header.to_owned()),
            None,
            Some(response(&request, vec![0; request.length() as usize])),
        );
        assert_eq!(
            session.admit(
                &request,
                attempt(1, 1),
                RangeAdmissionInput::http(http),
                &guard,
            ),
            Err(RangeAdmissionError::ResponseBindingDrift { binding })
        );
        assert_eq!(session.usage(), RangeAdmissionUsage::default());
    }
}

#[test]
fn request_range_arithmetic_and_all_budgets_fail_without_mutation() {
    let (request, guard) = fixture();
    let mut response_limited = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        limits(u64::from(request.length() - 1), 64, 4, 2),
    );
    assert!(matches!(
        response_limited.admit(
            &request,
            attempt(1, 1),
            RangeAdmissionInput::http(partial_http(&request, vec![0; request.length() as usize])),
            &guard,
        ),
        Err(RangeAdmissionError::BudgetExceeded {
            kind: RangeAdmissionBudgetKind::ResponseBytes,
            ..
        })
    ));
    assert_eq!(response_limited.usage(), RangeAdmissionUsage::default());

    let mut concurrency_limited = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        limits(32, 64, 4, 1),
    );
    assert!(matches!(
        concurrency_limited.admit(
            &request,
            attempt(1, 2),
            RangeAdmissionInput::TransportFailure(RangeTransportFailure::Timeout),
            &guard,
        ),
        Err(RangeAdmissionError::BudgetExceeded {
            kind: RangeAdmissionBudgetKind::ActiveRequests,
            ..
        })
    ));

    let mut request_limited = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        limits(32, 64, 1, 1),
    );
    request_limited
        .admit(
            &request,
            attempt(1, 1),
            RangeAdmissionInput::TransportFailure(RangeTransportFailure::Timeout),
            &guard,
        )
        .expect("first attempt");
    let before = request_limited.usage();
    assert!(matches!(
        request_limited.admit(
            &request,
            attempt(2, 1),
            RangeAdmissionInput::TransportFailure(RangeTransportFailure::Timeout),
            &guard,
        ),
        Err(RangeAdmissionError::BudgetExceeded {
            kind: RangeAdmissionBudgetKind::RequestCount,
            ..
        })
    ));
    assert_eq!(request_limited.usage(), before);

    let overflow_artifact = ArtifactDescriptor::new(
        Pc4ArtifactRole::Graph,
        "graph.bin",
        u64::MAX,
        "dynamic-content-identity",
    )
    .expect("synthetic large artifact");
    let overflow_request = RangeRequest::new(
        request.lookup_session(),
        request.request_id(),
        request.snapshot().clone(),
        request.profile(),
        overflow_artifact,
        u64::MAX,
        1,
    );
    let mut overflow_session = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        limits(32, 64, 1, 1),
    );
    assert_eq!(
        overflow_session.admit(
            &overflow_request,
            attempt(1, 1),
            RangeAdmissionInput::TransportFailure(RangeTransportFailure::Unavailable),
            &guard,
        ),
        Err(RangeAdmissionError::RangeEndOverflow)
    );
    assert_eq!(overflow_session.usage(), RangeAdmissionUsage::default());
}

#[test]
fn cumulative_bytes_retry_after_and_ordinals_are_bounded() {
    let (request, guard) = fixture();
    let mut session = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        limits(32, u64::from(request.length()), 3, 1),
    );
    session
        .admit(
            &request,
            attempt(1, 1),
            RangeAdmissionInput::http(partial_http(&request, vec![0; request.length() as usize])),
            &guard,
        )
        .expect("first body");
    let before = session.usage();
    assert!(matches!(
        session.admit(
            &request,
            attempt(2, 1),
            RangeAdmissionInput::http(partial_http(&request, vec![0; request.length() as usize])),
            &guard,
        ),
        Err(RangeAdmissionError::BudgetExceeded {
            kind: RangeAdmissionBudgetKind::SessionBytes,
            ..
        })
    ));
    assert_eq!(session.usage(), before);
    assert_eq!(
        session.admit(
            &request,
            attempt(3, 1),
            RangeAdmissionInput::TransportFailure(RangeTransportFailure::Timeout),
            &guard,
        ),
        Err(RangeAdmissionError::RequestOrdinalMismatch {
            expected: 2,
            actual: 3,
        })
    );
    assert_eq!(session.usage(), before);

    let mut retry_session = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        limits(32, 64, 1, 1),
    );
    assert!(matches!(
        retry_session.admit(
            &request,
            attempt(1, 1),
            RangeAdmissionInput::http(RangeHttpResponse::new(
                429,
                None,
                Some("61".to_owned()),
                None
            )),
            &guard,
        ),
        Err(RangeAdmissionError::BudgetExceeded {
            kind: RangeAdmissionBudgetKind::RetryAfterSeconds,
            ..
        })
    ));
    assert_eq!(retry_session.usage(), RangeAdmissionUsage::default());
}

#[test]
fn cancellation_and_generation_change_at_final_check_are_transactional() {
    let (request, base_guard) = fixture();
    struct FlippingGuard {
        checks: Cell<u8>,
        current: QualifiedSnapshotIdentity,
        cancel_on_second: bool,
        stale_on_second: bool,
    }
    impl RangeAdmissionGuard for FlippingGuard {
        fn is_cancelled(&self) -> bool {
            let check = self.checks.get() + 1;
            self.checks.set(check);
            self.cancel_on_second && check >= 2
        }

        fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool {
            !(self.stale_on_second && self.checks.get() >= 2) && &self.current == expected
        }
    }

    for (cancel_on_second, stale_on_second, expected) in [
        (true, false, RangeAdmissionError::Cancelled),
        (false, true, RangeAdmissionError::SnapshotStale),
    ] {
        let guard = FlippingGuard {
            checks: Cell::new(0),
            current: base_guard.current.clone(),
            cancel_on_second,
            stale_on_second,
        };
        let mut session = RangeAdmissionSession::new(
            request.lookup_session(),
            request.snapshot().clone(),
            limits(32, 64, 1, 1),
        );
        assert_eq!(
            session.admit(
                &request,
                attempt(1, 1),
                RangeAdmissionInput::TransportFailure(RangeTransportFailure::Timeout),
                &guard,
            ),
            Err(expected)
        );
        assert_eq!(session.usage(), RangeAdmissionUsage::default());
    }
}

#[test]
fn initial_guard_and_request_identity_failures_are_transactional() {
    let (request, base_guard) = fixture();
    let cancelled = Guard {
        cancelled: true,
        ..base_guard.clone()
    };
    let stale = Guard {
        current: qualified_snapshot_identity("new-generation", "new-manifest"),
        ..base_guard.clone()
    };
    for (guard, expected) in [
        (cancelled, RangeAdmissionError::Cancelled),
        (stale, RangeAdmissionError::SnapshotStale),
    ] {
        let mut session = RangeAdmissionSession::new(
            request.lookup_session(),
            request.snapshot().clone(),
            limits(32, 64, 1, 1),
        );
        assert_eq!(
            session.admit(
                &request,
                attempt(1, 1),
                RangeAdmissionInput::TransportFailure(RangeTransportFailure::Timeout),
                &guard,
            ),
            Err(expected)
        );
        assert_eq!(session.usage(), RangeAdmissionUsage::default());
    }

    let wrong_session = RangeRequest::new(
        session_id(99),
        request.request_id(),
        request.snapshot().clone(),
        request.profile(),
        request.artifact_descriptor().clone(),
        request.offset(),
        request.length(),
    );
    let wrong_snapshot = RangeRequest::new(
        request.lookup_session(),
        request.request_id(),
        qualified_snapshot_identity("request-drift", "request-drift-manifest"),
        request.profile(),
        request.artifact_descriptor().clone(),
        request.offset(),
        request.length(),
    );
    for (drifted, expected) in [
        (wrong_session, RangeAdmissionError::RequestSessionMismatch),
        (wrong_snapshot, RangeAdmissionError::RequestSnapshotMismatch),
    ] {
        let mut session = RangeAdmissionSession::new(
            request.lookup_session(),
            request.snapshot().clone(),
            limits(32, 64, 1, 1),
        );
        assert_eq!(
            session.admit(
                &drifted,
                attempt(1, 1),
                RangeAdmissionInput::TransportFailure(RangeTransportFailure::Timeout),
                &base_guard,
            ),
            Err(expected)
        );
        assert_eq!(session.usage(), RangeAdmissionUsage::default());
    }
}

#[test]
fn malformed_retry_after_and_counter_overflow_never_commit() {
    let (request, guard) = fixture();
    for retry_after in ["", " 1", "1.0", "Wed, 21 Oct 2015 07:28:00 GMT"] {
        let mut session = RangeAdmissionSession::new(
            request.lookup_session(),
            request.snapshot().clone(),
            limits(32, 64, 1, 1),
        );
        assert_eq!(
            session.admit(
                &request,
                attempt(1, 1),
                RangeAdmissionInput::http(RangeHttpResponse::new(
                    429,
                    None,
                    Some(retry_after.to_owned()),
                    None,
                )),
                &guard,
            ),
            Err(RangeAdmissionError::RetryAfterMalformed)
        );
        assert_eq!(session.usage(), RangeAdmissionUsage::default());
    }

    let mut request_overflow = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        RangeAdmissionLimits::new(
            nonzero_u64(32),
            nonzero_u64(u64::MAX),
            nonzero_u32(u32::MAX),
            nonzero_u16(1),
            60,
        ),
    );
    request_overflow.usage.request_count = u32::MAX;
    let before = request_overflow.usage();
    assert_eq!(
        request_overflow.admit(
            &request,
            attempt(u32::MAX, 1),
            RangeAdmissionInput::TransportFailure(RangeTransportFailure::Timeout),
            &guard,
        ),
        Err(RangeAdmissionError::AccountingOverflow)
    );
    assert_eq!(request_overflow.usage(), before);

    let mut byte_overflow = RangeAdmissionSession::new(
        request.lookup_session(),
        request.snapshot().clone(),
        RangeAdmissionLimits::new(
            nonzero_u64(32),
            nonzero_u64(u64::MAX),
            nonzero_u32(1),
            nonzero_u16(1),
            60,
        ),
    );
    byte_overflow.usage.admitted_bytes = u64::MAX;
    let before = byte_overflow.usage();
    assert_eq!(
        byte_overflow.admit(
            &request,
            attempt(1, 1),
            RangeAdmissionInput::http(partial_http(&request, vec![0; request.length() as usize],)),
            &guard,
        ),
        Err(RangeAdmissionError::AccountingOverflow)
    );
    assert_eq!(byte_overflow.usage(), before);
}
