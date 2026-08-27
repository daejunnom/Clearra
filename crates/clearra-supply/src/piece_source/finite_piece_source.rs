use core::mem::size_of;

use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    finite_allocation::{
        FiniteSupplyAllocationError, FiniteSupplyAllocationLedger,
        FiniteSupplyAllocationTransaction,
    },
    mixed::supply_provenance::{BagBoundaryEvidence, SupplyProvenance, SupplyProvenanceError},
    pattern_universe::pattern_universe_materializer::{
        FinitePatternUniverseMaterializationError, PatternUniverseMaterializationError,
        PatternUniverseMaterializer,
    },
    queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
};

use super::{
    piece_source::PieceSource, BagUniverseDescriptor, FixedPieceSequence, PieceSourceKind,
};

#[derive(Clone, Copy, Debug)]
pub enum FiniteBuildQueueRef<'a> {
    FixedSequence(&'a FixedSequence),
    PatternExpression(&'a QueuePatternExpression),
    Standard7Bag,
    /// Fail-closed sentinel used by an upper queue enum for variants outside
    /// the finite Build contract.
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteSupplyProvenanceRef<'a> {
    bag_profile_id: &'a str,
    piece_set_id: &'a str,
    observed_window_id: Option<&'a str>,
    bag_boundary_evidence: BagBoundaryEvidence,
    duplicate_witness: bool,
    ambiguity_report: bool,
}

impl<'a> FiniteSupplyProvenanceRef<'a> {
    pub const fn new(
        bag_profile_id: &'a str,
        piece_set_id: &'a str,
        observed_window_id: Option<&'a str>,
        bag_boundary_evidence: BagBoundaryEvidence,
        duplicate_witness: bool,
        ambiguity_report: bool,
    ) -> Self {
        Self {
            bag_profile_id,
            piece_set_id,
            observed_window_id,
            bag_boundary_evidence,
            duplicate_witness,
            ambiguity_report,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FiniteBuildPieceSourceRequest<'a> {
    queue: FiniteBuildQueueRef<'a>,
    source_sequence_length: usize,
    max_patterns: usize,
    provenance: FiniteSupplyProvenanceRef<'a>,
}

impl<'a> FiniteBuildPieceSourceRequest<'a> {
    pub const fn new(
        queue: FiniteBuildQueueRef<'a>,
        source_sequence_length: usize,
        max_patterns: usize,
        provenance: FiniteSupplyProvenanceRef<'a>,
    ) -> Self {
        Self {
            queue,
            source_sequence_length,
            max_patterns,
            provenance,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FiniteBuildPieceSourceAllocationProjection {
    requested_additional_bytes: u128,
    projected_piece_source_retained_bytes: u128,
    projected_retained_queue_bytes: u128,
}

impl FiniteBuildPieceSourceAllocationProjection {
    pub const fn requested_additional_bytes(self) -> u128 {
        self.requested_additional_bytes
    }

    pub const fn projected_piece_source_retained_bytes(self) -> u128 {
        self.projected_piece_source_retained_bytes
    }

    pub const fn projected_retained_queue_bytes(self) -> u128 {
        self.projected_retained_queue_bytes
    }
}

/// The full queue copy retained by `SearchProblem::supply`.
///
/// Converting this enum into the upper `PcQueueInput` is an allocation-free
/// match. It intentionally has no `Clone` implementation: the governed copy
/// has one owner and must be moved into the final problem.
#[derive(Debug, Eq, PartialEq)]
pub enum FiniteBuildSupplyQueue {
    FixedSequence(FixedSequence),
    PatternExpression(QueuePatternExpression),
    Standard7Bag,
}

impl FiniteBuildSupplyQueue {
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::FixedSequence(sequence) => sequence.checked_retained_capacity_bytes(),
            Self::PatternExpression(expression) => expression.checked_retained_capacity_bytes(),
            Self::Standard7Bag => Some(0),
        }
    }
}

/// Move-only owners returned by the finite supply materializer.
///
/// Peak/live evidence remains on the caller-owned ledger so this wrapper adds
/// no evidence fields beyond the two final SearchProblem owners.
#[derive(Debug, PartialEq)]
pub struct FiniteBuildPieceSourceMaterialization {
    piece_source: PieceSource,
    retained_queue: FiniteBuildSupplyQueue,
}

impl FiniteBuildPieceSourceMaterialization {
    pub const fn piece_source(&self) -> &PieceSource {
        &self.piece_source
    }

    pub const fn retained_queue(&self) -> &FiniteBuildSupplyQueue {
        &self.retained_queue
    }

    pub fn into_parts(self) -> (PieceSource, FiniteBuildSupplyQueue) {
        (self.piece_source, self.retained_queue)
    }
}

/// Inline return bytes not already represented by the final PieceSource and
/// retained-queue slots reserved inside `SearchProblem`.
pub const fn finite_build_piece_source_returned_carrier_delta_bytes() -> u128 {
    let carrier = size_of::<
        Result<FiniteBuildPieceSourceMaterialization, FinitePieceSourceMaterializationError>,
    >() as u128;
    let final_owner_slots =
        size_of::<PieceSource>() as u128 + size_of::<FiniteBuildSupplyQueue>() as u128;
    carrier.saturating_sub(final_owner_slots)
}

impl PieceSource {
    /// Validates the complete finite Build supply shape without allocating.
    /// Upper compilers should call this before allocating unrelated owners
    /// such as problem identifiers.
    pub fn validate_finite_build(
        request: FiniteBuildPieceSourceRequest<'_>,
    ) -> Result<(), FinitePieceSourceMaterializationError> {
        if matches!(request.queue, FiniteBuildQueueRef::Unsupported) {
            return Err(FinitePieceSourceMaterializationError::UnsupportedQueueShape);
        }
        let provenance_id = SupplyProvenance::checked_finite_provenance_id(
            request.provenance.bag_profile_id,
            request.provenance.piece_set_id,
            request.provenance.observed_window_id,
            request.provenance.bag_boundary_evidence,
            request.provenance.duplicate_witness,
            request.provenance.ambiguity_report,
        )
        .map_err(FinitePieceSourceMaterializationError::Provenance)?;
        match request.queue {
            FiniteBuildQueueRef::FixedSequence(sequence) => {
                validate_source_window(request.source_sequence_length, sequence.len())
            }
            FiniteBuildQueueRef::PatternExpression(expression) => {
                validate_source_window(request.source_sequence_length, expression.sequence_len())?;
                PatternUniverseMaterializer::validate_queue_pattern_expression_finite(
                    expression,
                    request.source_sequence_length,
                )
                .map_err(FinitePieceSourceMaterializationError::PatternUniverse)
            }
            FiniteBuildQueueRef::Standard7Bag => PatternUniverseMaterializer::standard_7_bag(
                request.source_sequence_length,
                request.max_patterns,
                provenance_id,
            )
            .map(|_| ())
            .map_err(FinitePieceSourceMaterializationError::PatternUniverse),
            FiniteBuildQueueRef::Unsupported => unreachable!("rejected before validation"),
        }
    }

    /// Projects every requested heap payload before the finite materializer's
    /// first allocation. This is a requested-capacity projection; the ledger
    /// still remeasures actual capacity immediately after every allocation.
    pub fn checked_finite_build_allocation_projection(
        request: FiniteBuildPieceSourceRequest<'_>,
    ) -> Result<FiniteBuildPieceSourceAllocationProjection, FinitePieceSourceMaterializationError>
    {
        Self::validate_finite_build(request)?;
        SupplyProvenance::checked_finite_provenance_id(
            request.provenance.bag_profile_id,
            request.provenance.piece_set_id,
            request.provenance.observed_window_id,
            request.provenance.bag_boundary_evidence,
            request.provenance.duplicate_witness,
            request.provenance.ambiguity_report,
        )
        .map_err(FinitePieceSourceMaterializationError::Provenance)?;
        let provenance_bytes = checked_sum([
            request.provenance.bag_profile_id.len() as u128,
            request.provenance.piece_set_id.len() as u128,
            request.provenance.observed_window_id.map_or(0, str::len) as u128,
        ])?;
        let piece_size = size_of::<PieceKind>() as u128;

        let (piece_source_without_provenance, retained_queue_bytes) = match request.queue {
            FiniteBuildQueueRef::FixedSequence(sequence) => {
                validate_source_window(request.source_sequence_length, sequence.len())?;
                let prefix_bytes = checked_count(request.source_sequence_length, piece_size)?;
                let flat_offset_bytes = checked_count(2, size_of::<usize>() as u128)?;
                let piece_source_bytes =
                    checked_sum([prefix_bytes, flat_offset_bytes, prefix_bytes])?;
                let retained_queue_bytes = checked_count(sequence.len(), piece_size)?;
                (piece_source_bytes, retained_queue_bytes)
            }
            FiniteBuildQueueRef::PatternExpression(expression) => {
                validate_source_window(request.source_sequence_length, expression.sequence_len())?;
                PatternUniverseMaterializer::validate_queue_pattern_expression_finite(
                    expression,
                    request.source_sequence_length,
                )
                .map_err(FinitePieceSourceMaterializationError::PatternUniverse)?;
                let retained_queue_bytes = expression
                    .checked_finite_duplicate_requested_bytes(expression.sequence_len())
                    .ok_or(projection_overflow())?;
                let piece_source_bytes = if expression.is_factorized() {
                    expression
                        .checked_finite_duplicate_requested_bytes(request.source_sequence_length)
                        .ok_or(projection_overflow())?
                } else {
                    let sequences = expression
                        .explicit_sequences()
                        .expect("validated explicit expression storage");
                    let offset_bytes = checked_count(
                        sequences
                            .len()
                            .checked_add(1)
                            .ok_or(projection_overflow())?,
                        size_of::<usize>() as u128,
                    )?;
                    let piece_count = sequences.iter().try_fold(0usize, |total, sequence| {
                        total.checked_add(request.source_sequence_length.min(sequence.len()))
                    });
                    let piece_bytes =
                        checked_count(piece_count.ok_or(projection_overflow())?, piece_size)?;
                    checked_sum([offset_bytes, piece_bytes])?
                };
                (piece_source_bytes, retained_queue_bytes)
            }
            FiniteBuildQueueRef::Standard7Bag => {
                PatternUniverseMaterializer::standard_7_bag(
                    request.source_sequence_length,
                    request.max_patterns,
                    1,
                )
                .map_err(FinitePieceSourceMaterializationError::PatternUniverse)?;
                (
                    checked_count(PieceKind::STANDARD_TETROMINOES.len(), piece_size)?,
                    0,
                )
            }
            FiniteBuildQueueRef::Unsupported => unreachable!("rejected before projection"),
        };
        let projected_piece_source_retained_bytes = provenance_bytes
            .checked_add(piece_source_without_provenance)
            .ok_or(projection_overflow())?;
        let requested_additional_bytes = projected_piece_source_retained_bytes
            .checked_add(retained_queue_bytes)
            .ok_or(projection_overflow())?;
        Ok(FiniteBuildPieceSourceAllocationProjection {
            requested_additional_bytes,
            projected_piece_source_retained_bytes,
            projected_retained_queue_bytes: retained_queue_bytes,
        })
    }

    /// Materializes the supported finite Build supply graph and its one full
    /// retained queue duplicate under a single rollback transaction.
    ///
    /// The original borrowed queue remains live in the consumed scenario
    /// query. Callers must therefore include its measured capacity in the
    /// ledger's entry live bytes. Every new Vec/String payload is authorized
    /// before allocation and remeasured immediately afterward. On any error,
    /// the transaction is not committed and the caller retains an unchanged
    /// ledger owner.
    pub fn materialize_finite_build(
        request: FiniteBuildPieceSourceRequest<'_>,
        ledger: &mut FiniteSupplyAllocationLedger,
    ) -> Result<FiniteBuildPieceSourceMaterialization, FinitePieceSourceMaterializationError> {
        Self::validate_finite_build(request)?;
        let mut transaction = ledger.transaction();
        let materialization =
            Self::materialize_finite_build_in_transaction(request, &mut transaction)?;
        transaction.commit();
        Ok(materialization)
    }

    /// Transaction-level variant used when the upper compiler must keep its
    /// own String/Vec allocations and this supply graph under one atomic
    /// authority. On error the caller must abort (drop) the transaction; it
    /// must not commit the working counters after partially built owners have
    /// been dropped.
    pub fn materialize_finite_build_in_transaction(
        request: FiniteBuildPieceSourceRequest<'_>,
        transaction: &mut FiniteSupplyAllocationTransaction<'_>,
    ) -> Result<FiniteBuildPieceSourceMaterialization, FinitePieceSourceMaterializationError> {
        Self::validate_finite_build(request)?;

        let provenance_id = SupplyProvenance::checked_finite_provenance_id(
            request.provenance.bag_profile_id,
            request.provenance.piece_set_id,
            request.provenance.observed_window_id,
            request.provenance.bag_boundary_evidence,
            request.provenance.duplicate_witness,
            request.provenance.ambiguity_report,
        )
        .map_err(FinitePieceSourceMaterializationError::Provenance)?;

        let standard_universe = match request.queue {
            FiniteBuildQueueRef::FixedSequence(sequence) => {
                validate_source_window(request.source_sequence_length, sequence.len())?;
                None
            }
            FiniteBuildQueueRef::PatternExpression(expression) => {
                validate_source_window(request.source_sequence_length, expression.sequence_len())?;
                PatternUniverseMaterializer::validate_queue_pattern_expression_finite(
                    expression,
                    request.source_sequence_length,
                )
                .map_err(FinitePieceSourceMaterializationError::PatternUniverse)?;
                None
            }
            FiniteBuildQueueRef::Standard7Bag => Some(
                PatternUniverseMaterializer::standard_7_bag(
                    request.source_sequence_length,
                    request.max_patterns,
                    provenance_id,
                )
                .map_err(FinitePieceSourceMaterializationError::PatternUniverse)?,
            ),
            FiniteBuildQueueRef::Unsupported => unreachable!("rejected before validation"),
        };

        let provenance = SupplyProvenance::from_validated_finite_parts(
            provenance_id,
            request.provenance.bag_profile_id,
            request.provenance.piece_set_id,
            request.provenance.observed_window_id,
            request.provenance.bag_boundary_evidence,
            request.provenance.duplicate_witness,
            request.provenance.ambiguity_report,
            transaction,
        )
        .map_err(FinitePieceSourceMaterializationError::Allocation)?;

        let (piece_source, retained_queue) = match request.queue {
            FiniteBuildQueueRef::FixedSequence(sequence) => {
                let retained_pieces = duplicate_pieces(sequence.pieces(), transaction)?;
                let descriptor_pieces = duplicate_pieces(
                    &sequence.pieces()[..request.source_sequence_length],
                    transaction,
                )?;
                let universe = PatternUniverseMaterializer::fixed_sequence_finite(
                    &sequence.pieces()[..request.source_sequence_length],
                    provenance_id,
                    transaction,
                )
                .map_err(map_finite_universe_error)?;
                (
                    PieceSource::from_parts(
                        PieceSourceKind::FixedQueue,
                        provenance,
                        Some(FixedPieceSequence::new(descriptor_pieces)),
                        None,
                        None,
                        universe,
                    ),
                    FiniteBuildSupplyQueue::FixedSequence(FixedSequence::new(retained_pieces)),
                )
            }
            FiniteBuildQueueRef::PatternExpression(expression) => {
                let universe = PatternUniverseMaterializer::queue_pattern_expression_finite(
                    expression,
                    request.source_sequence_length,
                    provenance_id,
                    transaction,
                )
                .map_err(map_finite_universe_error)?;
                let retained_expression = expression
                    .duplicate_finite(expression.sequence_len(), transaction)
                    .map_err(FinitePieceSourceMaterializationError::Allocation)?;
                (
                    PieceSource::materialized_pattern_universe(universe, provenance),
                    FiniteBuildSupplyQueue::PatternExpression(retained_expression),
                )
            }
            FiniteBuildQueueRef::Standard7Bag => {
                let descriptor_pieces =
                    duplicate_pieces(&PieceKind::STANDARD_TETROMINOES, transaction)?;
                (
                    PieceSource::from_parts(
                        PieceSourceKind::BagUniverse,
                        provenance,
                        None,
                        Some(BagUniverseDescriptor::new(descriptor_pieces)),
                        None,
                        standard_universe.expect("validated standard universe"),
                    ),
                    FiniteBuildSupplyQueue::Standard7Bag,
                )
            }
            FiniteBuildQueueRef::Unsupported => unreachable!("rejected before allocation"),
        };

        Ok(FiniteBuildPieceSourceMaterialization {
            piece_source,
            retained_queue,
        })
    }
}

fn duplicate_pieces(
    pieces: &[PieceKind],
    transaction: &mut FiniteSupplyAllocationTransaction<'_>,
) -> Result<Vec<PieceKind>, FinitePieceSourceMaterializationError> {
    let mut duplicate = transaction
        .try_vec_with_capacity::<PieceKind>(pieces.len())
        .map_err(FinitePieceSourceMaterializationError::Allocation)?;
    duplicate.extend_from_slice(pieces);
    Ok(duplicate)
}

fn validate_source_window(
    requested: usize,
    available: usize,
) -> Result<(), FinitePieceSourceMaterializationError> {
    if requested > available {
        return Err(
            FinitePieceSourceMaterializationError::SourceSequenceLengthOutOfRange {
                requested,
                available,
            },
        );
    }
    Ok(())
}

fn checked_count(
    count: usize,
    item_size: u128,
) -> Result<u128, FinitePieceSourceMaterializationError> {
    (count as u128)
        .checked_mul(item_size)
        .ok_or(projection_overflow())
}

fn checked_sum<const N: usize>(
    values: [u128; N],
) -> Result<u128, FinitePieceSourceMaterializationError> {
    values.into_iter().try_fold(0_u128, |total, value| {
        total.checked_add(value).ok_or(projection_overflow())
    })
}

const fn projection_overflow() -> FinitePieceSourceMaterializationError {
    FinitePieceSourceMaterializationError::Allocation(
        FiniteSupplyAllocationError::ProjectionOverflow,
    )
}

fn map_finite_universe_error(
    error: FinitePatternUniverseMaterializationError,
) -> FinitePieceSourceMaterializationError {
    match error {
        FinitePatternUniverseMaterializationError::Allocation(error) => {
            FinitePieceSourceMaterializationError::Allocation(error)
        }
        FinitePatternUniverseMaterializationError::Materialization(error) => {
            FinitePieceSourceMaterializationError::PatternUniverse(error)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinitePieceSourceMaterializationError {
    UnsupportedQueueShape,
    SourceSequenceLengthOutOfRange { requested: usize, available: usize },
    Provenance(SupplyProvenanceError),
    PatternUniverse(PatternUniverseMaterializationError),
    Allocation(FiniteSupplyAllocationError),
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use super::*;
    use crate::mixed::supply_provenance::SupplyProvenance;

    const EXTERNAL_RETAINED_BYTES: u128 = 19;

    fn provenance_ref() -> FiniteSupplyProvenanceRef<'static> {
        FiniteSupplyProvenanceRef::new(
            "standard-7-bag",
            "standard-tetrominoes",
            None,
            BagBoundaryEvidence::FixedBoundary,
            false,
            false,
        )
    }

    fn discover_peak(
        queue: FiniteBuildQueueRef<'_>,
        source_sequence_length: usize,
        max_patterns: usize,
        entry_live_bytes: u128,
    ) -> u128 {
        let mut ledger =
            FiniteSupplyAllocationLedger::try_new(u128::MAX, entry_live_bytes).expect("ledger");
        let materialization = PieceSource::materialize_finite_build(
            FiniteBuildPieceSourceRequest::new(
                queue,
                source_sequence_length,
                max_patterns,
                provenance_ref(),
            ),
            &mut ledger,
        )
        .expect("unbounded finite materialization");
        let expected_live = entry_live_bytes
            + materialization
                .piece_source()
                .checked_retained_capacity_bytes()
                .expect("piece-source retained capacity")
            + materialization
                .retained_queue()
                .checked_retained_capacity_bytes()
                .expect("retained queue capacity");
        assert_eq!(ledger.live_memory_bytes(), expected_live);
        assert_eq!(ledger.peak_memory_bytes(), expected_live);
        ledger.peak_memory_bytes()
    }

    fn assert_exact_and_one_byte_short(
        queue: FiniteBuildQueueRef<'_>,
        source_sequence_length: usize,
        max_patterns: usize,
        entry_live_bytes: u128,
    ) {
        let peak = discover_peak(
            queue,
            source_sequence_length,
            max_patterns,
            entry_live_bytes,
        );
        assert!(peak > entry_live_bytes);

        let mut exact =
            FiniteSupplyAllocationLedger::try_new(peak, entry_live_bytes).expect("exact ledger");
        PieceSource::materialize_finite_build(
            FiniteBuildPieceSourceRequest::new(
                queue,
                source_sequence_length,
                max_patterns,
                provenance_ref(),
            ),
            &mut exact,
        )
        .expect("exact cap succeeds");
        assert_eq!(exact.peak_memory_bytes(), peak);

        let mut short = FiniteSupplyAllocationLedger::try_new(peak - 1, entry_live_bytes)
            .expect("one-byte-short ledger");
        let original_live = short.live_memory_bytes();
        let original_peak = short.peak_memory_bytes();
        let error = PieceSource::materialize_finite_build(
            FiniteBuildPieceSourceRequest::new(
                queue,
                source_sequence_length,
                max_patterns,
                provenance_ref(),
            ),
            &mut short,
        )
        .expect_err("one-byte-short cap fails");
        assert_eq!(
            error,
            FinitePieceSourceMaterializationError::Allocation(
                FiniteSupplyAllocationError::MemoryCapacityExceeded {
                    required_memory_bytes: peak,
                    max_memory_bytes: peak - 1,
                }
            )
        );
        assert_eq!(short.live_memory_bytes(), original_live);
        assert_eq!(short.peak_memory_bytes(), original_peak);
    }

    fn assert_semantically_equal(left: &PieceSource, right: &PieceSource) {
        assert_eq!(left.kind(), right.kind());
        assert_eq!(left.provenance(), right.provenance());
        assert_eq!(left.pattern_universe_id(), right.pattern_universe_id());
        assert_eq!(
            left.pattern_weight_model_id(),
            right.pattern_weight_model_id()
        );
        let left_universe = left.materialized_universe().expect("left universe");
        let right_universe = right.materialized_universe().expect("right universe");
        assert_eq!(
            left_universe.pattern_count(),
            right_universe.pattern_count()
        );
        assert_eq!(left_universe.complete(), right_universe.complete());
        assert_eq!(
            left_universe.truncation_reason(),
            right_universe.truncation_reason()
        );
        let assert_pattern = |index: usize| {
            assert_eq!(
                left_universe.sequence_at(index),
                right_universe.sequence_at(index)
            );
            assert_eq!(
                left_universe.weight_at(index),
                right_universe.weight_at(index)
            );
        };
        if left_universe.pattern_count() <= 4_096 {
            for index in 0..left_universe.pattern_count() {
                assert_pattern(index);
            }
        } else {
            for index in [
                0,
                left_universe.pattern_count() / 2,
                left_universe.pattern_count() - 1,
            ] {
                assert_pattern(index);
            }
        }
    }

    #[test]
    fn fixed_branch_is_exactly_capped_and_preserves_semantics() {
        let sequence = FixedSequence::new(vec![PieceKind::I, PieceKind::O, PieceKind::T]);
        let entry_live = EXTERNAL_RETAINED_BYTES
            + sequence
                .checked_retained_capacity_bytes()
                .expect("input capacity");
        assert_exact_and_one_byte_short(
            FiniteBuildQueueRef::FixedSequence(&sequence),
            2,
            0,
            entry_live,
        );

        let mut ledger =
            FiniteSupplyAllocationLedger::try_new(u128::MAX, entry_live).expect("ledger");
        let finite = PieceSource::materialize_finite_build(
            FiniteBuildPieceSourceRequest::new(
                FiniteBuildQueueRef::FixedSequence(&sequence),
                2,
                0,
                provenance_ref(),
            ),
            &mut ledger,
        )
        .expect("finite fixed source");
        let compatibility = PieceSource::fixed_queue(
            FixedSequence::new(sequence.pieces()[..2].to_vec()),
            SupplyProvenance::standard_7_bag(),
        );
        assert_semantically_equal(finite.piece_source(), &compatibility);
        assert_eq!(
            finite.retained_queue(),
            &FiniteBuildSupplyQueue::FixedSequence(sequence)
        );
    }

    #[test]
    fn explicit_expression_branch_is_exactly_capped_and_preserves_semantics() {
        let expression = QueuePatternExpression::parse("IO;TS", 16).expect("explicit expression");
        assert!(!expression.is_factorized());
        let entry_live = EXTERNAL_RETAINED_BYTES
            + expression
                .checked_retained_capacity_bytes()
                .expect("input capacity");
        assert_exact_and_one_byte_short(
            FiniteBuildQueueRef::PatternExpression(&expression),
            1,
            16,
            entry_live,
        );

        let mut ledger =
            FiniteSupplyAllocationLedger::try_new(u128::MAX, entry_live).expect("ledger");
        let finite = PieceSource::materialize_finite_build(
            FiniteBuildPieceSourceRequest::new(
                FiniteBuildQueueRef::PatternExpression(&expression),
                1,
                16,
                provenance_ref(),
            ),
            &mut ledger,
        )
        .expect("finite explicit source");
        let compatibility = PieceSource::queue_pattern_expression(
            expression.prefix(1),
            SupplyProvenance::standard_7_bag(),
        )
        .expect("compatibility explicit source");
        assert_semantically_equal(finite.piece_source(), &compatibility);
    }

    #[test]
    fn factorized_expression_branch_is_exactly_capped_and_preserves_semantics() {
        const PATTERN_COUNT: usize = 1_066_867_200;
        let expression =
            QueuePatternExpression::parse("P7P7P2", PATTERN_COUNT).expect("factorized expression");
        assert!(expression.is_factorized());
        let entry_live = EXTERNAL_RETAINED_BYTES
            + expression
                .checked_retained_capacity_bytes()
                .expect("input capacity");
        assert_exact_and_one_byte_short(
            FiniteBuildQueueRef::PatternExpression(&expression),
            12,
            PATTERN_COUNT,
            entry_live,
        );

        let mut ledger =
            FiniteSupplyAllocationLedger::try_new(u128::MAX, entry_live).expect("ledger");
        let finite = PieceSource::materialize_finite_build(
            FiniteBuildPieceSourceRequest::new(
                FiniteBuildQueueRef::PatternExpression(&expression),
                12,
                PATTERN_COUNT,
                provenance_ref(),
            ),
            &mut ledger,
        )
        .expect("finite factorized source");
        let compatibility = PieceSource::queue_pattern_expression(
            expression.prefix(12),
            SupplyProvenance::standard_7_bag(),
        )
        .expect("compatibility factorized source");
        assert_semantically_equal(finite.piece_source(), &compatibility);
    }

    #[test]
    fn standard_bag_branch_is_exactly_capped_and_preserves_semantics() {
        assert_exact_and_one_byte_short(
            FiniteBuildQueueRef::Standard7Bag,
            12,
            64,
            EXTERNAL_RETAINED_BYTES,
        );

        let mut ledger = FiniteSupplyAllocationLedger::try_new(u128::MAX, EXTERNAL_RETAINED_BYTES)
            .expect("ledger");
        let finite = PieceSource::materialize_finite_build(
            FiniteBuildPieceSourceRequest::new(
                FiniteBuildQueueRef::Standard7Bag,
                12,
                64,
                provenance_ref(),
            ),
            &mut ledger,
        )
        .expect("finite standard source");
        let compatibility = PieceSource::standard_7_bag(SupplyProvenance::standard_7_bag(), 12, 64)
            .expect("compatibility standard source");
        assert_semantically_equal(finite.piece_source(), &compatibility);
        assert_eq!(
            finite.retained_queue(),
            &FiniteBuildSupplyQueue::Standard7Bag
        );
    }

    #[test]
    fn unsupported_shape_rejects_before_touching_the_ledger_owner() {
        let mut ledger = FiniteSupplyAllocationLedger::try_new(37, 23).expect("ledger");
        let error = PieceSource::materialize_finite_build(
            FiniteBuildPieceSourceRequest::new(
                FiniteBuildQueueRef::Unsupported,
                0,
                0,
                provenance_ref(),
            ),
            &mut ledger,
        )
        .expect_err("unsupported shape");

        assert_eq!(
            error,
            FinitePieceSourceMaterializationError::UnsupportedQueueShape
        );
        assert_eq!(ledger.live_memory_bytes(), 23);
        assert_eq!(ledger.peak_memory_bytes(), 23);
    }
}
