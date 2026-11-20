//! Checked closure of **ordinal** bounds in a finite Oracle fixture. The
//! caller still owns the physical lower/upper proof and candidate completeness.

use core::marker::PhantomData;

use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InvalidBounds {
    UnsupportedPolicy,
    IncompleteUniverse,
    DifferentSupply,
    WrongOrdinalWidth,
    DifferentScope,
    UnsealedRows,
    ContradictoryBounds,
}

#[derive(Clone, Copy)]
struct OracleOrdinals<'a> {
    universe: &'a MaterializedPatternUniverse,
    initial_hold: HoldAutomatonState,
    hold_enabled: bool,
    unplaced_lookahead: bool,
    standard_bag_lookahead: bool,
}

impl<'a> OracleOrdinals<'a> {
    fn new(
        universe: &'a MaterializedPatternUniverse,
        policy: QueueObservationPolicy,
        initial_hold: HoldAutomatonState,
        hold_enabled: bool,
        unplaced_lookahead: bool,
        standard_bag_lookahead: bool,
    ) -> Result<Self, InvalidBounds> {
        if policy != QueueObservationPolicy::FullQueueOracle
            || initial_hold.observation.policy != policy
        {
            return Err(InvalidBounds::UnsupportedPolicy);
        }
        if !universe.complete()
            || universe.total_possible_pattern_count() != universe.pattern_count() as u128
        {
            return Err(InvalidBounds::IncompleteUniverse);
        }
        Ok(Self {
            universe,
            initial_hold,
            hold_enabled,
            unplaced_lookahead,
            standard_bag_lookahead,
        })
    }

    fn matches(self, other: Self) -> bool {
        // Same ids, count and weights aren't enough: an immutable universe
        // owner pins the actual original-ordinal indexing and its denominator.
        core::ptr::eq(self.universe, other.universe)
            && self.initial_hold == other.initial_hold
            && self.hold_enabled == other.hold_enabled
            && self.unplaced_lookahead == other.unplaced_lookahead
            && self.standard_bag_lookahead == other.standard_bag_lookahead
    }

    fn evaluate(self, mask: u8, count_paths: bool) -> CoverageProductResult {
        CoverageProductEvaluator::default()
            .evaluate(
                &build_graph(mask),
                &PatternPiecePositionIndex::compile(self.universe).unwrap(),
                self.initial_hold,
                self.hold_enabled,
                self.unplaced_lookahead,
                self.standard_bag_lookahead,
                count_paths,
                false,
                &ExecutionControl::default(),
            )
            .unwrap()
    }
}

// In the fixture this object denotes an already closed candidate family,
// not evidence that an arbitrary list of keys is the complete family.
struct ClosedFamily(&'static [&'static str]);

#[derive(Clone, Copy)]
struct PerSolution<'a> {
    family: &'a ClosedFamily,
    index: usize,
}

#[derive(Clone, Copy)]
struct WholeFamily<'a>(&'a ClosedFamily);

trait Scope: Copy {
    fn matches(self, other: Self) -> bool;
}

impl Scope for PerSolution<'_> {
    fn matches(self, other: Self) -> bool {
        core::ptr::eq(self.family, other.family) && self.index == other.index
    }
}

impl Scope for WholeFamily<'_> {
    fn matches(self, other: Self) -> bool {
        core::ptr::eq(self.0, other.0)
    }
}

struct Lower;
struct Upper;

struct OrdinalBound<'a, S, Direction> {
    source: OracleOrdinals<'a>,
    scope: S,
    bits: PatternBitSet,
    direction: PhantomData<Direction>,
}

impl<'a, D> OrdinalBound<'a, PerSolution<'a>, D> {
    fn candidate(
        source: OracleOrdinals<'a>,
        family: &'a ClosedFamily,
        index: usize,
        bits: PatternBitSet,
    ) -> Result<Self, InvalidBounds> {
        if index >= family.0.len() {
            return Err(InvalidBounds::DifferentScope);
        }
        if bits.pattern_count() != source.universe.pattern_count() {
            return Err(InvalidBounds::WrongOrdinalWidth);
        }
        Ok(Self {
            source,
            scope: PerSolution { family, index },
            bits,
            direction: PhantomData,
        })
    }
}

impl<'a, D> OrdinalBound<'a, WholeFamily<'a>, D> {
    fn union(
        source: OracleOrdinals<'a>,
        family: &'a ClosedFamily,
        rows: &[OrdinalBound<'a, PerSolution<'a>, D>],
    ) -> Result<Self, InvalidBounds> {
        if rows.len() != family.0.len() {
            return Err(InvalidBounds::UnsealedRows);
        }
        let mut bits = PatternBitSet::new(source.universe.pattern_count());
        for (index, row) in rows.iter().enumerate() {
            if !row.scope.matches(PerSolution { family, index }) {
                return Err(InvalidBounds::UnsealedRows);
            }
            if !source.matches(row.source) {
                return Err(InvalidBounds::DifferentSupply);
            }
            bits.union_with(&row.bits)
                .map_err(|_| InvalidBounds::WrongOrdinalWidth)?;
        }
        Ok(Self {
            source,
            scope: WholeFamily(family),
            bits,
            direction: PhantomData,
        })
    }
}

// Intentionally only an exact ordinal result. No conversion to multiplicity,
// a labeled witness, an observation policy result, or another result scope.
struct ExactOrdinals<'a, 'b, S> {
    source: OracleOrdinals<'a>,
    scope: S,
    bits: &'b PatternBitSet,
}

impl<S> ExactOrdinals<'_, '_, S> {
    fn covered_weight(&self) -> Option<ProbabilityValue> {
        self.source.universe.weights().covered_weight(self.bits)
    }
}

fn close<'a, 'b, S: Scope>(
    lower: &'b OrdinalBound<'a, S, Lower>,
    upper: &OrdinalBound<'a, S, Upper>,
) -> Result<Option<ExactOrdinals<'a, 'b, S>>, InvalidBounds> {
    if !lower.source.matches(upper.source) {
        return Err(InvalidBounds::DifferentSupply);
    }
    if !lower.scope.matches(upper.scope) {
        return Err(InvalidBounds::DifferentScope);
    }
    if lower.bits == upper.bits {
        return Ok(Some(ExactOrdinals {
            source: lower.source,
            scope: lower.scope,
            bits: &lower.bits,
        }));
    }
    // No allocation or scalar/weight comparison, even with sparse storage.
    if (0..lower.bits.word_count())
        .any(|word| lower.bits.word_at(word) & !upper.bits.word_at(word) != 0)
    {
        return Err(InvalidBounds::ContradictoryBounds);
    }
    Ok(None)
}

struct FamilyBounds<'a> {
    lower: OrdinalBound<'a, WholeFamily<'a>, Lower>,
    upper: OrdinalBound<'a, WholeFamily<'a>, Upper>,
}

impl<'a> FamilyBounds<'a> {
    fn from_complete_rows(
        source: OracleOrdinals<'a>,
        family: &'a ClosedFamily,
        lowers: &[OrdinalBound<'a, PerSolution<'a>, Lower>],
        uppers: &[OrdinalBound<'a, PerSolution<'a>, Upper>],
    ) -> Result<Self, InvalidBounds> {
        if lowers.len() != family.0.len() || uppers.len() != family.0.len() {
            return Err(InvalidBounds::UnsealedRows);
        }
        // A global union may mask an unresolved row, but never a contradiction.
        for (lower, upper) in lowers.iter().zip(uppers) {
            let _ = close(lower, upper)?;
        }
        Ok(Self {
            lower: OrdinalBound::union(source, family, lowers)?,
            upper: OrdinalBound::union(source, family, uppers)?,
        })
    }

    fn close(&self) -> Result<Option<ExactOrdinals<'a, '_, WholeFamily<'a>>>, InvalidBounds> {
        close(&self.lower, &self.upper)
    }
}

fn hold(provenance: u64) -> HoldAutomatonState {
    HoldAutomatonState::new(
        PieceSourceId::new(63),
        0,
        Some(O),
        0,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyProvenanceId(provenance),
    )
}

fn oracle(universe: &MaterializedPatternUniverse) -> OracleOrdinals<'_> {
    OracleOrdinals::new(
        universe,
        QueueObservationPolicy::FullQueueOracle,
        hold(63),
        true,
        false,
        false,
    )
    .unwrap()
}

fn bits(ids: &[u32]) -> PatternBitSet {
    PatternBitSet::from_pattern_indices(4, ids.to_vec()).unwrap()
}

#[test]
fn semiring_contract_ordinal_bounds_require_one_immutable_oracle_supply() {
    let universe = universe();
    let source = oracle(&universe);
    let family = ClosedFamily(&["first", "second"]);
    let lower = OrdinalBound::<_, Lower>::candidate(source, &family, 0, bits(&[0])).unwrap();
    let other_universe = universe.clone();
    let foreign = [
        oracle(&other_universe),
        OracleOrdinals {
            initial_hold: hold(64),
            ..source
        },
        OracleOrdinals {
            unplaced_lookahead: true,
            ..source
        },
    ];
    for changed in foreign {
        let upper = OrdinalBound::<_, Upper>::candidate(changed, &family, 0, bits(&[0])).unwrap();
        assert!(matches!(
            close(&lower, &upper),
            Err(InvalidBounds::DifferentSupply)
        ));
    }
    assert!(matches!(
        OracleOrdinals::new(
            &universe,
            QueueObservationPolicy::VisibleSeven,
            hold(63),
            true,
            false,
            false
        ),
        Err(InvalidBounds::UnsupportedPolicy)
    ));
    let mut visible_hold = hold(63);
    visible_hold.observation.policy = QueueObservationPolicy::VisibleSeven;
    assert!(matches!(
        OracleOrdinals::new(
            &universe,
            QueueObservationPolicy::FullQueueOracle,
            visible_hold,
            true,
            false,
            false
        ),
        Err(InvalidBounds::UnsupportedPolicy)
    ));
    let truncated = MaterializedPatternUniverse::from_sequences(
        universe.pattern_universe_id(),
        universe.pattern_weight_model_id(),
        vec![vec![O, T]],
        vec![ProbabilityValue::ZERO],
        4,
        false,
        None,
    )
    .unwrap();
    assert!(matches!(
        OracleOrdinals::new(
            &truncated,
            QueueObservationPolicy::FullQueueOracle,
            hold(63),
            true,
            false,
            false
        ),
        Err(InvalidBounds::IncompleteUniverse)
    ));
    assert!(matches!(
        OrdinalBound::<_, Upper>::candidate(source, &family, 0, PatternBitSet::new(5)),
        Err(InvalidBounds::WrongOrdinalWidth)
    ));
}

#[test]
fn semiring_contract_ordinal_bounds_bind_family_rows_and_keep_zero_weight_ordinals() {
    let universe = universe();
    let source = oracle(&universe);
    let family = ClosedFamily(&["first", "second"]);
    let complete = source.evaluate(LOWER, false).coverage_bits;
    let missing_zero = bits(&[1, 2]);
    assert_eq!(
        universe.weights().covered_weight(&complete),
        universe.weights().covered_weight(&missing_zero)
    );
    let lowers = [
        OrdinalBound::<_, Lower>::candidate(source, &family, 0, missing_zero).unwrap(),
        OrdinalBound::<_, Lower>::candidate(source, &family, 1, bits(&[0])).unwrap(),
    ];
    let uppers = [
        OrdinalBound::<_, Upper>::candidate(source, &family, 0, complete.clone()).unwrap(),
        OrdinalBound::<_, Upper>::candidate(source, &family, 1, bits(&[0])).unwrap(),
    ];
    assert!(close(&lowers[0], &uppers[0]).unwrap().is_none());
    assert!(matches!(
        close(&lowers[0], &uppers[1]),
        Err(InvalidBounds::DifferentScope)
    ));
    let contradictory =
        OrdinalBound::<_, Upper>::candidate(source, &family, 0, bits(&[0, 2])).unwrap();
    assert!(matches!(
        close(&lowers[0], &contradictory),
        Err(InvalidBounds::ContradictoryBounds)
    ));
    assert!(matches!(
        FamilyBounds::from_complete_rows(source, &family, &lowers[..1], &uppers),
        Err(InvalidBounds::UnsealedRows)
    ));
    let overlap_hides_contradiction = [
        contradictory,
        OrdinalBound::<_, Upper>::candidate(source, &family, 1, bits(&[0, 1])).unwrap(),
    ];
    assert!(matches!(
        FamilyBounds::from_complete_rows(source, &family, &lowers, &overlap_hides_contradiction),
        Err(InvalidBounds::ContradictoryBounds)
    ));
    let family_result =
        FamilyBounds::from_complete_rows(source, &family, &lowers, &uppers).unwrap();
    let global_exact = family_result.close().unwrap().unwrap();
    assert!(global_exact.scope.matches(WholeFamily(&family)));
    assert_eq!(*global_exact.bits, complete);
    assert_eq!(
        global_exact.covered_weight(),
        ProbabilityValue::new(0.75).ok()
    );
    // Union equality cannot mint exact per-solution results.
    assert!(close(&lowers[0], &uppers[0]).unwrap().is_none());
}

#[test]
fn semiring_contract_ordinal_closure_is_not_multiplicity_or_witness_closure() {
    let universe = universe();
    let source = oracle(&universe);
    let family = ClosedFamily(&["only"]);
    let concrete_lower = source.evaluate(LOWER, true);
    let concrete_upper = source.evaluate(LOWER | DIFFERENT_WITNESSES, true);
    let lower =
        OrdinalBound::<_, Lower>::candidate(source, &family, 0, concrete_lower.coverage_bits)
            .unwrap();
    let upper =
        OrdinalBound::<_, Upper>::candidate(source, &family, 0, concrete_upper.coverage_bits)
            .unwrap();
    let exact = close(&lower, &upper).unwrap().unwrap();
    assert!(exact.scope.matches(PerSolution {
        family: &family,
        index: 0
    }));
    assert_eq!(exact.bits.to_owned_words(), [0b0111]);
    assert!(concrete_lower.count_complete && concrete_upper.count_complete);
    assert_eq!(
        (concrete_lower.path_count, concrete_upper.path_count),
        (8, 12)
    );
    let queue = universe.sequence_at(0);
    let paths_lower = labeled_witnesses(LOWER, queue.as_ref());
    let paths_upper = labeled_witnesses(LOWER | DIFFERENT_WITNESSES, queue.as_ref());
    assert!(paths_lower.is_subset(&paths_upper));
    assert_ne!(paths_lower, paths_upper);
}
