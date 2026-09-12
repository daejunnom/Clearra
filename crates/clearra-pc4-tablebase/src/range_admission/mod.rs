// SRP rationale: this module admits one host-observed HTTP Range result into
// the transport-independent reader protocol. It owns HTTP status/header
// validation and bounded session accounting, but performs no I/O, retry,
// discovery, cache mutation, lookup execution, or product fallback.
use core::{fmt, num::NonZeroU16, num::NonZeroU32, num::NonZeroU64};

use crate::{
    LookupSessionId, QualifiedSnapshotIdentity, RangeRequest, RangeResponse, RangeResponseKind,
    RangeTransportFailure,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RangeAdmissionLimits {
    max_response_bytes: NonZeroU64,
    max_session_bytes: NonZeroU64,
    max_request_count: NonZeroU32,
    max_active_requests: NonZeroU16,
    max_retry_after_seconds: u64,
}

impl RangeAdmissionLimits {
    pub const fn new(
        max_response_bytes: NonZeroU64,
        max_session_bytes: NonZeroU64,
        max_request_count: NonZeroU32,
        max_active_requests: NonZeroU16,
        max_retry_after_seconds: u64,
    ) -> Self {
        Self {
            max_response_bytes,
            max_session_bytes,
            max_request_count,
            max_active_requests,
            max_retry_after_seconds,
        }
    }

    pub const fn max_response_bytes(self) -> u64 {
        self.max_response_bytes.get()
    }

    pub const fn max_session_bytes(self) -> u64 {
        self.max_session_bytes.get()
    }

    pub const fn max_request_count(self) -> u32 {
        self.max_request_count.get()
    }

    pub const fn max_active_requests(self) -> u16 {
        self.max_active_requests.get()
    }

    pub const fn max_retry_after_seconds(self) -> u64 {
        self.max_retry_after_seconds
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RangeAdmissionAttempt {
    request_ordinal: NonZeroU32,
    active_requests: NonZeroU16,
}

impl RangeAdmissionAttempt {
    pub const fn new(request_ordinal: NonZeroU32, active_requests: NonZeroU16) -> Self {
        Self {
            request_ordinal,
            active_requests,
        }
    }

    pub const fn request_ordinal(self) -> u32 {
        self.request_ordinal.get()
    }

    pub const fn active_requests(self) -> u16 {
        self.active_requests.get()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RangeAdmissionUsage {
    request_count: u32,
    admitted_bytes: u64,
}

impl RangeAdmissionUsage {
    pub const fn request_count(self) -> u32 {
        self.request_count
    }

    pub const fn admitted_bytes(self) -> u64 {
        self.admitted_bytes
    }
}

/// Host-owned cancellation and immutable-snapshot freshness observation.
///
/// Implementations must not fetch or refresh a snapshot from inside these
/// methods. The admission transaction checks the guard before validation and
/// immediately before committing its counters.
pub trait RangeAdmissionGuard {
    fn is_cancelled(&self) -> bool;

    fn is_current_snapshot(&self, expected: &QualifiedSnapshotIdentity) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RangeHttpResponse {
    status_code: u16,
    content_range: Option<String>,
    retry_after: Option<String>,
    response: Option<RangeResponse>,
}

impl RangeHttpResponse {
    pub fn new(
        status_code: u16,
        content_range: Option<String>,
        retry_after: Option<String>,
        response: Option<RangeResponse>,
    ) -> Self {
        Self {
            status_code,
            content_range,
            retry_after,
            response,
        }
    }

    pub const fn status_code(&self) -> u16 {
        self.status_code
    }

    pub fn content_range(&self) -> Option<&str> {
        self.content_range.as_deref()
    }

    pub fn retry_after(&self) -> Option<&str> {
        self.retry_after.as_deref()
    }

    pub const fn response(&self) -> Option<&RangeResponse> {
        self.response.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RangeAdmissionInput {
    Http(Box<RangeHttpResponse>),
    TransportFailure(RangeTransportFailure),
}

impl RangeAdmissionInput {
    pub fn http(response: RangeHttpResponse) -> Self {
        Self::Http(Box::new(response))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RangeAdmissionOutcome {
    PartialContent(RangeResponse),
    RangeNotSatisfiable { complete_length: u64 },
    TransportFailure(RangeTransportFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeAdmissionBinding {
    LookupSession,
    RequestId,
    Snapshot,
    Profile,
    ArtifactRole,
    ArtifactContentIdentity,
    ResponseKind,
    Offset,
    RequestedLength,
    CompleteLength,
    BodyLength,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeAdmissionBudgetKind {
    ResponseBytes,
    SessionBytes,
    RequestCount,
    ActiveRequests,
    RetryAfterSeconds,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RangeAdmissionError {
    Cancelled,
    SnapshotStale,
    RequestSessionMismatch,
    RequestSnapshotMismatch,
    RequestOrdinalMismatch {
        expected: u32,
        actual: u32,
    },
    ZeroLengthRange,
    RangeEndOverflow,
    RangeOutsideArtifact {
        end_exclusive: u64,
        artifact_length: u64,
    },
    WholeContentRejected,
    MissingPartialResponse,
    UnexpectedPartialResponse,
    MissingContentRange,
    MalformedContentRange,
    UnsatisfiedContentRangeRequired,
    SatisfiedContentRangeRequired,
    ResponseBindingDrift {
        binding: RangeAdmissionBinding,
    },
    BudgetExceeded {
        kind: RangeAdmissionBudgetKind,
        limit: u64,
        actual: u64,
    },
    RetryAfterMalformed,
    UnexpectedStatus {
        status_code: u16,
    },
    AccountingOverflow,
}

impl RangeAdmissionError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "pc4_range_admission_cancelled",
            Self::SnapshotStale => "pc4_range_admission_snapshot_stale",
            Self::RequestSessionMismatch => "pc4_range_admission_request_session_mismatch",
            Self::RequestSnapshotMismatch => "pc4_range_admission_request_snapshot_mismatch",
            Self::RequestOrdinalMismatch { .. } => "pc4_range_admission_request_ordinal_mismatch",
            Self::ZeroLengthRange => "pc4_range_admission_zero_length_range",
            Self::RangeEndOverflow => "pc4_range_admission_range_end_overflow",
            Self::RangeOutsideArtifact { .. } => "pc4_range_admission_range_outside_artifact",
            Self::WholeContentRejected => "pc4_range_admission_whole_content_rejected",
            Self::MissingPartialResponse => "pc4_range_admission_partial_response_missing",
            Self::UnexpectedPartialResponse => "pc4_range_admission_partial_response_unexpected",
            Self::MissingContentRange => "pc4_range_admission_content_range_missing",
            Self::MalformedContentRange => "pc4_range_admission_content_range_malformed",
            Self::UnsatisfiedContentRangeRequired => {
                "pc4_range_admission_unsatisfied_content_range_required"
            }
            Self::SatisfiedContentRangeRequired => {
                "pc4_range_admission_satisfied_content_range_required"
            }
            Self::ResponseBindingDrift { .. } => "pc4_range_admission_response_binding_drift",
            Self::BudgetExceeded { .. } => "pc4_range_admission_budget_exceeded",
            Self::RetryAfterMalformed => "pc4_range_admission_retry_after_malformed",
            Self::UnexpectedStatus { .. } => "pc4_range_admission_unexpected_status",
            Self::AccountingOverflow => "pc4_range_admission_accounting_overflow",
        }
    }
}

impl fmt::Display for RangeAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}

impl std::error::Error for RangeAdmissionError {}

#[derive(Clone, Debug)]
pub struct RangeAdmissionSession {
    lookup_session: LookupSessionId,
    snapshot: QualifiedSnapshotIdentity,
    limits: RangeAdmissionLimits,
    usage: RangeAdmissionUsage,
}

impl RangeAdmissionSession {
    pub fn new(
        lookup_session: LookupSessionId,
        snapshot: QualifiedSnapshotIdentity,
        limits: RangeAdmissionLimits,
    ) -> Self {
        Self {
            lookup_session,
            snapshot,
            limits,
            usage: RangeAdmissionUsage::default(),
        }
    }

    pub const fn lookup_session(&self) -> LookupSessionId {
        self.lookup_session
    }

    pub const fn snapshot(&self) -> &QualifiedSnapshotIdentity {
        &self.snapshot
    }

    pub const fn limits(&self) -> RangeAdmissionLimits {
        self.limits
    }

    pub const fn usage(&self) -> RangeAdmissionUsage {
        self.usage
    }

    /// Admits one already-observed transport result without performing I/O.
    ///
    /// Every validation and final guard check completes before accounting is
    /// committed. Returned transport failures are observations only: this
    /// method never retries them and never starts an offline solver.
    pub fn admit<G>(
        &mut self,
        request: &RangeRequest,
        attempt: RangeAdmissionAttempt,
        input: RangeAdmissionInput,
        guard: &G,
    ) -> Result<RangeAdmissionOutcome, RangeAdmissionError>
    where
        G: RangeAdmissionGuard + ?Sized,
    {
        ensure_guard(guard, &self.snapshot)?;
        self.validate_request(request)?;

        let next_request_count = self
            .usage
            .request_count
            .checked_add(1)
            .ok_or(RangeAdmissionError::AccountingOverflow)?;
        if attempt.request_ordinal() != next_request_count {
            return Err(RangeAdmissionError::RequestOrdinalMismatch {
                expected: next_request_count,
                actual: attempt.request_ordinal(),
            });
        }
        check_budget(
            RangeAdmissionBudgetKind::RequestCount,
            u64::from(self.limits.max_request_count()),
            u64::from(next_request_count),
        )?;
        check_budget(
            RangeAdmissionBudgetKind::ActiveRequests,
            u64::from(self.limits.max_active_requests()),
            u64::from(attempt.active_requests()),
        )?;

        let (outcome, admitted_response_bytes) = match input {
            RangeAdmissionInput::Http(response) => self.admit_http(request, *response)?,
            RangeAdmissionInput::TransportFailure(failure) => {
                validate_retry_after_failure(&failure, self.limits.max_retry_after_seconds())?;
                (RangeAdmissionOutcome::TransportFailure(failure), 0)
            }
        };
        let next_admitted_bytes = self
            .usage
            .admitted_bytes
            .checked_add(admitted_response_bytes)
            .ok_or(RangeAdmissionError::AccountingOverflow)?;
        check_budget(
            RangeAdmissionBudgetKind::SessionBytes,
            self.limits.max_session_bytes(),
            next_admitted_bytes,
        )?;

        ensure_guard(guard, &self.snapshot)?;
        self.usage = RangeAdmissionUsage {
            request_count: next_request_count,
            admitted_bytes: next_admitted_bytes,
        };
        Ok(outcome)
    }

    fn validate_request(&self, request: &RangeRequest) -> Result<(), RangeAdmissionError> {
        if request.lookup_session() != self.lookup_session {
            return Err(RangeAdmissionError::RequestSessionMismatch);
        }
        if request.snapshot() != &self.snapshot {
            return Err(RangeAdmissionError::RequestSnapshotMismatch);
        }
        if request.length() == 0 {
            return Err(RangeAdmissionError::ZeroLengthRange);
        }
        let end_exclusive = request
            .offset()
            .checked_add(u64::from(request.length()))
            .ok_or(RangeAdmissionError::RangeEndOverflow)?;
        let artifact_length = request.artifact_descriptor().byte_len();
        if end_exclusive > artifact_length {
            return Err(RangeAdmissionError::RangeOutsideArtifact {
                end_exclusive,
                artifact_length,
            });
        }
        check_budget(
            RangeAdmissionBudgetKind::ResponseBytes,
            self.limits.max_response_bytes(),
            u64::from(request.length()),
        )
    }

    fn admit_http(
        &self,
        request: &RangeRequest,
        http: RangeHttpResponse,
    ) -> Result<(RangeAdmissionOutcome, u64), RangeAdmissionError> {
        match http.status_code {
            206 => self.admit_partial(request, http),
            200 => Err(RangeAdmissionError::WholeContentRejected),
            416 => self.admit_unsatisfied(request, http),
            429 => self.admit_rate_limit(http),
            status_code => Err(RangeAdmissionError::UnexpectedStatus { status_code }),
        }
    }

    fn admit_partial(
        &self,
        request: &RangeRequest,
        http: RangeHttpResponse,
    ) -> Result<(RangeAdmissionOutcome, u64), RangeAdmissionError> {
        let parsed = parse_content_range(
            http.content_range
                .as_deref()
                .ok_or(RangeAdmissionError::MissingContentRange)?,
        )?;
        let ParsedContentRange::Satisfied {
            start,
            end_inclusive,
            complete_length,
        } = parsed
        else {
            return Err(RangeAdmissionError::SatisfiedContentRangeRequired);
        };
        let response = http
            .response
            .ok_or(RangeAdmissionError::MissingPartialResponse)?;
        validate_partial_bindings(request, &response, start, end_inclusive, complete_length)?;
        let response_bytes = u64::try_from(response.bytes.len())
            .map_err(|_| RangeAdmissionError::AccountingOverflow)?;
        check_budget(
            RangeAdmissionBudgetKind::ResponseBytes,
            self.limits.max_response_bytes(),
            response_bytes,
        )?;
        Ok((
            RangeAdmissionOutcome::PartialContent(response),
            response_bytes,
        ))
    }

    fn admit_unsatisfied(
        &self,
        request: &RangeRequest,
        http: RangeHttpResponse,
    ) -> Result<(RangeAdmissionOutcome, u64), RangeAdmissionError> {
        if http.response.is_some() {
            return Err(RangeAdmissionError::UnexpectedPartialResponse);
        }
        let parsed = parse_content_range(
            http.content_range
                .as_deref()
                .ok_or(RangeAdmissionError::MissingContentRange)?,
        )?;
        let ParsedContentRange::Unsatisfied { complete_length } = parsed else {
            return Err(RangeAdmissionError::UnsatisfiedContentRangeRequired);
        };
        if complete_length != request.artifact_descriptor().byte_len() {
            return Err(RangeAdmissionError::ResponseBindingDrift {
                binding: RangeAdmissionBinding::CompleteLength,
            });
        }
        Ok((
            RangeAdmissionOutcome::RangeNotSatisfiable { complete_length },
            0,
        ))
    }

    fn admit_rate_limit(
        &self,
        http: RangeHttpResponse,
    ) -> Result<(RangeAdmissionOutcome, u64), RangeAdmissionError> {
        if http.response.is_some() || http.content_range.is_some() {
            return Err(RangeAdmissionError::UnexpectedPartialResponse);
        }
        let retry_after_seconds = match http.retry_after {
            Some(value) => Some(parse_retry_after(
                &value,
                self.limits.max_retry_after_seconds(),
            )?),
            None => None,
        };
        Ok((
            RangeAdmissionOutcome::TransportFailure(RangeTransportFailure::RateLimited {
                retry_after_seconds,
            }),
            0,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParsedContentRange {
    Satisfied {
        start: u64,
        end_inclusive: u64,
        complete_length: u64,
    },
    Unsatisfied {
        complete_length: u64,
    },
}

fn parse_content_range(value: &str) -> Result<ParsedContentRange, RangeAdmissionError> {
    let remainder = value
        .strip_prefix("bytes ")
        .ok_or(RangeAdmissionError::MalformedContentRange)?;
    let (range, complete) = remainder
        .split_once('/')
        .ok_or(RangeAdmissionError::MalformedContentRange)?;
    if complete.is_empty() || complete.contains('/') {
        return Err(RangeAdmissionError::MalformedContentRange);
    }
    let complete_length = complete
        .parse::<u64>()
        .map_err(|_| RangeAdmissionError::MalformedContentRange)?;
    if range == "*" {
        return Ok(ParsedContentRange::Unsatisfied { complete_length });
    }
    let (start, end) = range
        .split_once('-')
        .ok_or(RangeAdmissionError::MalformedContentRange)?;
    if start.is_empty() || end.is_empty() || end.contains('-') {
        return Err(RangeAdmissionError::MalformedContentRange);
    }
    let start = start
        .parse::<u64>()
        .map_err(|_| RangeAdmissionError::MalformedContentRange)?;
    let end_inclusive = end
        .parse::<u64>()
        .map_err(|_| RangeAdmissionError::MalformedContentRange)?;
    if end_inclusive < start {
        return Err(RangeAdmissionError::MalformedContentRange);
    }
    Ok(ParsedContentRange::Satisfied {
        start,
        end_inclusive,
        complete_length,
    })
}

fn validate_partial_bindings(
    request: &RangeRequest,
    response: &RangeResponse,
    range_start: u64,
    range_end_inclusive: u64,
    header_complete_length: u64,
) -> Result<(), RangeAdmissionError> {
    let expected_end_inclusive = request
        .offset()
        .checked_add(u64::from(request.length()))
        .and_then(|end| end.checked_sub(1))
        .ok_or(RangeAdmissionError::RangeEndOverflow)?;
    let binding = if response.lookup_session != request.lookup_session() {
        Some(RangeAdmissionBinding::LookupSession)
    } else if response.request_id != request.request_id() {
        Some(RangeAdmissionBinding::RequestId)
    } else if response.snapshot != *request.snapshot() {
        Some(RangeAdmissionBinding::Snapshot)
    } else if response.profile != request.profile() {
        Some(RangeAdmissionBinding::Profile)
    } else if response.artifact != request.artifact() {
        Some(RangeAdmissionBinding::ArtifactRole)
    } else if response.artifact_content_identity != request.artifact_descriptor().content_identity()
    {
        Some(RangeAdmissionBinding::ArtifactContentIdentity)
    } else if response.kind != RangeResponseKind::PartialContent {
        Some(RangeAdmissionBinding::ResponseKind)
    } else if response.offset != request.offset() || range_start != request.offset() {
        Some(RangeAdmissionBinding::Offset)
    } else if range_end_inclusive != expected_end_inclusive {
        Some(RangeAdmissionBinding::RequestedLength)
    } else if header_complete_length != request.artifact_descriptor().byte_len()
        || response.complete_length != header_complete_length
    {
        Some(RangeAdmissionBinding::CompleteLength)
    } else if response.bytes.len() != request.length() as usize {
        Some(RangeAdmissionBinding::BodyLength)
    } else {
        None
    };
    match binding {
        Some(binding) => Err(RangeAdmissionError::ResponseBindingDrift { binding }),
        None => Ok(()),
    }
}

fn parse_retry_after(value: &str, limit: u64) -> Result<u64, RangeAdmissionError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(RangeAdmissionError::RetryAfterMalformed);
    }
    let seconds = value
        .parse::<u64>()
        .map_err(|_| RangeAdmissionError::RetryAfterMalformed)?;
    check_budget(RangeAdmissionBudgetKind::RetryAfterSeconds, limit, seconds)?;
    Ok(seconds)
}

fn validate_retry_after_failure(
    failure: &RangeTransportFailure,
    limit: u64,
) -> Result<(), RangeAdmissionError> {
    if let RangeTransportFailure::RateLimited {
        retry_after_seconds: Some(seconds),
    } = failure
    {
        check_budget(RangeAdmissionBudgetKind::RetryAfterSeconds, limit, *seconds)?;
    }
    Ok(())
}

fn ensure_guard<G>(
    guard: &G,
    snapshot: &QualifiedSnapshotIdentity,
) -> Result<(), RangeAdmissionError>
where
    G: RangeAdmissionGuard + ?Sized,
{
    if guard.is_cancelled() {
        Err(RangeAdmissionError::Cancelled)
    } else if !guard.is_current_snapshot(snapshot) {
        Err(RangeAdmissionError::SnapshotStale)
    } else {
        Ok(())
    }
}

fn check_budget(
    kind: RangeAdmissionBudgetKind,
    limit: u64,
    actual: u64,
) -> Result<(), RangeAdmissionError> {
    if actual > limit {
        Err(RangeAdmissionError::BudgetExceeded {
            kind,
            limit,
            actual,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
