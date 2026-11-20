//! Private terminal reduction experiment for the native fixed-queue Oracle
//! All/Omit lane. The coordinator must first prove candidate-range completeness
//! and enumerate the exact worker row count under the original lease. This
//! cursor reduces those rows, not execution counts, coverage, or replay proof.
//! In particular, an opaque hash-table iterator is not treated as bounded.

use std::{cmp::Reverse, collections::BinaryHeap, mem::size_of};

use clearra_core_domain::{
    board::standard_pc_board::{
        Board256Mask, STANDARD_PC_BOARD_WIDTH, STANDARD_PC_EXTENDED_MIN_LINES,
        STANDARD_PC_MAX_LINES,
    },
    solution::{ExtendedTilingSolutionKey, NormalizedTilingSolutionSetHasher},
};

use crate::resource::ExecutionMemoryBound;

use super::{
    super::{super::extended_board::ExtendedBoard, ExtendedTilingKey},
    ExecutionControl, WasmExactSearchError,
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Collect,
    Drain,
    Complete,
    Failed,
}

struct CanonicalFamily {
    keys: Vec<String>,
    hash: String,
}

#[derive(Clone, Copy)]
enum SourceKey<'a> {
    Concrete(&'a ExtendedTilingKey),
    // Independent lexical/codec oracle, not an input to the product.
    Fixture(&'a str),
}

struct CanonicalReduction {
    // All heap slots and all output slots are pre-admitted; moving a key from
    // one to the other does not copy or reallocate its text.
    pending: BinaryHeap<Reverse<String>>,
    unique: Vec<String>,
    hasher: NormalizedTilingSolutionSetHasher,
    hash: Option<String>,
    phase: Phase,
    expected_rows: usize,
    accepted_rows: usize,
    initial: Board256Mask,
    height: u8,
    depth: usize,
    key_len: usize,
    // Conservative bound on everything the caller keeps live for this phase.
    // It must include source-row storage and exclude the cursor's storage.
    fixed_outside: u128,
    memory_bound: ExecutionMemoryBound,
    key_bytes: u128,
}

impl CanonicalReduction {
    fn new(
        expected_rows: usize,
        height: u8,
        initial: Board256Mask,
        depth: usize,
        fixed_outside: u128,
        memory_bound: ExecutionMemoryBound,
    ) -> Result<Self, WasmExactSearchError> {
        let cells = u16::from(height) * STANDARD_PC_BOARD_WIDTH;
        if !(STANDARD_PC_EXTENDED_MIN_LINES..=STANDARD_PC_MAX_LINES).contains(&height)
            || !initial.fits_cell_count(cells).unwrap_or(false)
            || depth == 0
            || depth.checked_mul(4) != Some(usize::from(cells) - initial.count_ones() as usize)
        {
            return Err(contract_error());
        }
        let key_len = ExtendedTilingKey::empty().canonical_key_len(height)
            + depth * 66
            + depth.saturating_sub(1);
        let mut owner = Self {
            pending: BinaryHeap::new(),
            unique: Vec::new(),
            hasher: NormalizedTilingSolutionSetHasher::default(),
            hash: None,
            phase: Phase::Collect,
            expected_rows,
            accepted_rows: 0,
            initial,
            height,
            depth,
            key_len,
            fixed_outside,
            memory_bound,
            key_bytes: 0,
        };
        let slots = (expected_rows as u128)
            .checked_mul((size_of::<Reverse<String>>() + size_of::<String>()) as u128)
            .ok_or_else(contract_error)?;
        owner.guard(slots)?;
        owner
            .pending
            .try_reserve_exact(expected_rows)
            .map_err(|_| storage_error())?;
        owner
            .unique
            .try_reserve_exact(expected_rows)
            .map_err(|_| storage_error())?;
        owner.guard(0)?;
        Ok(owner)
    }

    fn guard(&self, future: u128) -> Result<(), WasmExactSearchError> {
        let observed = self
            .fixed_outside
            .checked_add(size_of::<Self>() as u128)
            .and_then(|bytes| bytes.checked_add(self.key_bytes))
            .and_then(|bytes| {
                bytes.checked_add(
                    (self.pending.capacity() as u128)
                        .checked_mul(size_of::<Reverse<String>>() as u128)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    (self.unique.capacity() as u128).checked_mul(size_of::<String>() as u128)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(self.hash.as_ref().map_or(0, |hash| hash.capacity()) as u128)
            })
            .ok_or_else(contract_error)?;
        self.memory_bound
            .ensure(observed, future)
            .map_err(WasmExactSearchError::resource_admission)
    }

    fn check_control(&self, control: &ExecutionControl) -> Result<(), WasmExactSearchError> {
        if self.phase == Phase::Failed {
            return Err(contract_error());
        }
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        Ok(())
    }

    /// One source row per debit and at most one fixed-width string allocation.
    /// Use the concrete key's existing encoder after charging its exact format
    /// length, not a caller-supplied render callback or arbitrary iterator.
    fn collect_one(
        &mut self,
        remaining: &mut usize,
        control: &ExecutionControl,
        source: SourceKey<'_>,
    ) -> Result<bool, WasmExactSearchError> {
        let result = self.collect_inner(remaining, control, source);
        if result.is_err() {
            self.phase = Phase::Failed;
        }
        result
    }

    fn collect_inner(
        &mut self,
        remaining: &mut usize,
        control: &ExecutionControl,
        source: SourceKey<'_>,
    ) -> Result<bool, WasmExactSearchError> {
        self.check_control(control)?;
        if *remaining == 0 {
            return Ok(false);
        }
        let key_len = match source {
            SourceKey::Concrete(key) => key.canonical_key_len(self.height),
            SourceKey::Fixture(text) => text.len(),
        };
        if self.phase != Phase::Collect
            || self.accepted_rows == self.expected_rows
            || key_len != self.key_len
        {
            return Err(contract_error());
        }
        *remaining -= 1;
        self.guard(key_len as u128)?;
        let key = match source {
            SourceKey::Concrete(key) => {
                key.canonical_key(ExtendedBoard::from_mask(self.initial), self.height)
            }
            SourceKey::Fixture(text) => text.to_owned(),
        };
        if key.len() != key_len || key.capacity() != key_len {
            return Err(contract_error());
        }
        let parsed =
            ExtendedTilingSolutionKey::parse_canonical(&key).map_err(|_| contract_error())?;
        if parsed.height() != self.height
            || parsed.initial_board() != self.initial
            || parsed.placement_count() != self.depth
        {
            return Err(contract_error());
        }
        self.key_bytes = self
            .key_bytes
            .checked_add(key.capacity() as u128)
            .ok_or_else(contract_error)?;
        self.pending.push(Reverse(key));
        self.accepted_rows += 1;
        Ok(true)
    }

    /// At most one lexically smallest source key per debit, even when it is a
    /// duplicate. Heap work per key grows logarithmically with admitted rows;
    /// there is no whole-family sort, dedup, hash, or retained-storage recount.
    fn advance_output(
        &mut self,
        remaining: &mut usize,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        let result = self.output_inner(remaining, control);
        if result.is_err() {
            self.phase = Phase::Failed;
        }
        result
    }

    fn output_inner(
        &mut self,
        remaining: &mut usize,
        control: &ExecutionControl,
    ) -> Result<bool, WasmExactSearchError> {
        self.check_control(control)?;
        if self.phase == Phase::Complete || *remaining == 0 {
            return Ok(self.phase == Phase::Complete);
        }
        if self.accepted_rows != self.expected_rows {
            return Err(contract_error());
        }
        *remaining -= 1;
        self.phase = Phase::Drain;
        if let Some(Reverse(key)) = self.pending.pop() {
            if self.unique.last() == Some(&key) {
                self.key_bytes = self
                    .key_bytes
                    .checked_sub(key.capacity() as u128)
                    .ok_or_else(contract_error)?;
            } else {
                let parsed = ExtendedTilingSolutionKey::parse_canonical(&key)
                    .map_err(|_| contract_error())?;
                self.hasher.update_extended_canonical_key(parsed);
                self.unique.push(key);
            }
            return Ok(false);
        }
        // Both the cts1 prefix and the hex digest have fixed width. Reserve a
        // conservative small formatting envelope after dropping empty backing.
        self.pending = BinaryHeap::new();
        self.guard(64)?;
        self.hash = Some(std::mem::take(&mut self.hasher).finish());
        self.guard(0)?;
        self.phase = Phase::Complete;
        Ok(true)
    }

    fn into_family(
        self,
        control: &ExecutionControl,
    ) -> Result<CanonicalFamily, WasmExactSearchError> {
        self.check_control(control)?;
        if self.phase != Phase::Complete {
            return Err(contract_error());
        }
        Ok(CanonicalFamily {
            keys: self.unique,
            hash: self.hash.ok_or_else(contract_error)?,
        })
    }
}

fn contract_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_pc_cooperative_canonical_contract_invalid")
}

fn storage_error() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("extended_pc_cooperative_canonical_storage_unavailable")
}

mod tests {
    use super::*;
    use crate::backend::wasm_cpu::extended_build_probability::ExtendedBuildProbabilitySession;
    use clearra_core_domain::{
        execution_cancellation::ExecutionCancellationToken, piece::piece_kind::PieceKind,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{BuildProbabilityField, ProblemCompiler};
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::super::ExtendedGeometryAdvance;

    const HEIGHT: u8 = 8;
    const DEPTH: usize = 4;

    fn mask(cells: impl IntoIterator<Item = u16>) -> Board256Mask {
        cells
            .into_iter()
            .map(|cell| Board256Mask::singleton(cell).unwrap())
            .fold(Board256Mask::EMPTY, Board256Mask::union)
    }

    fn fixture_initial() -> Board256Mask {
        Board256Mask::all_cells(80).unwrap().without(mask(
            (4_u16..8).flat_map(|y| (3_u16..7).map(move |x| y * 10 + x)),
        ))
    }

    fn hex(mask: Board256Mask) -> String {
        let [a, b, c, d] = mask.words();
        format!("{d:016x}{c:016x}{b:016x}{a:016x}")
    }

    fn key(piece: char, placements: impl IntoIterator<Item = Board256Mask>) -> String {
        let mut pieces: Vec<_> = placements.into_iter().collect();
        pieces.sort_unstable_by_key(|mask| mask.words());
        format!(
            "ctk2|height={HEIGHT}|initial={}|placements={}",
            hex(fixture_initial()),
            pieces
                .into_iter()
                .map(|placement| format!("{piece}:{}", hex(placement)))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    fn horizontal() -> String {
        key(
            'I',
            (4_u16..8).map(|y| mask((3_u16..7).map(move |x| y * 10 + x))),
        )
    }

    fn vertical() -> String {
        key(
            'I',
            (3_u16..7).map(|x| mask((4_u16..8).map(move |y| y * 10 + x))),
        )
    }

    fn squares() -> String {
        key(
            'O',
            [43, 45, 63, 65].map(|c| mask([c, c + 1, c + 10, c + 11])),
        )
    }

    fn hooks() -> String {
        key(
            'J',
            [
                [43, 53, 54, 55],
                [44, 45, 46, 56],
                [63, 73, 74, 75],
                [64, 65, 66, 76],
            ]
            .map(mask),
        )
    }

    fn new(rows: usize, cap: u128) -> CanonicalReduction {
        CanonicalReduction::new(
            rows,
            HEIGHT,
            fixture_initial(),
            DEPTH,
            0,
            ExecutionMemoryBound::for_test_cap(cap),
        )
        .unwrap()
    }

    #[test]
    fn cooperative_canonical_reduction_matches_lexical_reference() {
        let control = ExecutionControl::default();
        let rows = [squares(), horizontal(), hooks(), vertical(), horizontal()];
        let mut expected = rows.to_vec();
        expected.sort_unstable();
        expected.dedup();
        // ASCII J < O, unlike the enum; the two I partitions also have
        // opposite derived low-word-first and lexical high-word-first order.
        assert!(rows[2] < rows[0], "public ASCII order is J before O");
        let h = ExtendedTilingSolutionKey::parse_canonical(&rows[1]).unwrap();
        let v = ExtendedTilingSolutionKey::parse_canonical(&rows[3]).unwrap();
        assert_eq!(h.placements().cmp(v.placements()), rows[3].cmp(&rows[1]));
        assert_ne!(rows[3], rows[1]);
        let mut reference = NormalizedTilingSolutionSetHasher::default();
        for text in &expected {
            reference.update_extended_canonical_key(
                ExtendedTilingSolutionKey::parse_canonical(text).unwrap(),
            );
        }
        let mut owner = new(rows.len(), u128::MAX);
        assert!(!owner
            .collect_one(&mut 0, &control, SourceKey::Fixture("invalid"))
            .unwrap());
        assert_eq!(owner.accepted_rows, 0);
        for text in &rows {
            let mut remaining = 1;
            assert!(owner
                .collect_one(&mut remaining, &control, SourceKey::Fixture(text))
                .unwrap());
            assert_eq!(remaining, 0);
        }
        for _ in 0..rows.len() {
            assert!(!owner.advance_output(&mut 0, &control).unwrap());
            let mut remaining = 1;
            assert!(!owner.advance_output(&mut remaining, &control).unwrap());
            assert_eq!(remaining, 0);
        }
        assert!(owner.advance_output(&mut 1, &control).unwrap());
        let actual = owner.into_family(&control).unwrap();
        assert_eq!(actual.keys, expected);
        assert_eq!(actual.hash, reference.finish());
    }

    #[test]
    fn cooperative_canonical_reduction_rejects_missing_rows_and_cancellation() {
        let control = ExecutionControl::default();
        let mut incomplete = new(1, u128::MAX);
        assert!(incomplete.advance_output(&mut 1, &control).is_err());
        assert!(incomplete.into_family(&control).is_err());

        let mut invalid = new(1, u128::MAX);
        assert!(invalid
            .collect_one(
                &mut 1,
                &control,
                SourceKey::Concrete(&ExtendedTilingKey::empty()),
            )
            .is_err());
        assert!(invalid.into_family(&control).is_err());

        let mut owner = new(1, u128::MAX);
        let key = horizontal();
        owner
            .collect_one(&mut 1, &control, SourceKey::Fixture(&key))
            .unwrap();
        assert!(!owner.advance_output(&mut 1, &control).unwrap());
        let cancellation = ExecutionCancellationToken::new();
        let cancelled = ExecutionControl::new(cancellation.clone());
        cancellation.handle().cancel();
        assert!(matches!(
            owner.advance_output(&mut 1, &cancelled),
            Err(WasmExactSearchError::Cancelled)
        ));
        assert!(owner.into_family(&control).is_err());
    }

    #[test]
    fn cooperative_canonical_reduction_consumes_real_extended_geometry_key() {
        const HEIGHT: u8 = 7;
        const DEPTH: usize = 7;
        // The same two offset apertures used by the real worker-wave probe.
        let holes = mask((0_u16..7).flat_map(|y| {
            let x = if y < 3 { 0 } else { 6 };
            (x..x + 4).map(move |x| 10 * y + x)
        }));
        let initial = Board256Mask::all_cells(70).unwrap().without(holes);
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10_from_words(u16::from(HEIGHT), initial.words()).unwrap(),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; DEPTH])),
            PieceWindow::new(DEPTH),
        )
        .with_exact_pieces(Some(DEPTH))
        .with_allow_hold(false);
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let field = BuildProbabilityField::from_words_preserving_height(
            HEIGHT,
            initial.words(),
            holes.words(),
        )
        .unwrap();
        let bound = ExecutionMemoryBound::for_test_cap(u128::MAX);
        let mut engine =
            ExtendedBuildProbabilitySession::new_pc_family(&problem, field, bound, 0).unwrap();
        let candidate = (0..100_000)
            .find_map(|_| match engine.advance_geometry_bounded().unwrap() {
                ExtendedGeometryAdvance::Pending => None,
                ExtendedGeometryAdvance::Candidate(candidate) => Some(candidate),
                _ => panic!("expected a real extended Geometry candidate"),
            })
            .expect("bounded seven-line aperture");
        let key = ExtendedTilingKey::from_candidate(&engine.catalog, &candidate);
        let expected = key.canonical_key(ExtendedBoard::from_mask(initial), HEIGHT);
        let mut reference = NormalizedTilingSolutionSetHasher::default();
        reference.update_extended_canonical_key(
            ExtendedTilingSolutionKey::parse_canonical(&expected).unwrap(),
        );
        let control = ExecutionControl::default();
        let mut owner = CanonicalReduction::new(1, HEIGHT, initial, DEPTH, 0, bound).unwrap();
        assert!(owner
            .collect_one(&mut 1, &control, SourceKey::Concrete(&key))
            .unwrap());
        assert!(!owner.advance_output(&mut 1, &control).unwrap());
        assert!(owner.advance_output(&mut 1, &control).unwrap());
        let actual = owner.into_family(&control).unwrap();
        assert_eq!(actual.keys, [expected]);
        assert_eq!(actual.hash, reference.finish());
    }

    #[test]
    fn cooperative_canonical_reduction_memory_boundary_precedes_rendering() {
        let control = ExecutionControl::default();
        let key = horizontal();
        let collection_limit = (size_of::<CanonicalReduction>()
            + size_of::<Reverse<String>>()
            + size_of::<String>()
            + key.len()) as u128;
        let mut rejected = new(1, collection_limit - 1);
        assert!(matches!(
            rejected.collect_one(&mut 1, &control, SourceKey::Fixture(&key)),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        assert!(rejected.pending.is_empty());
        assert!(rejected.into_family(&control).is_err());

        // Terminal formatting uses its own future window after the empty
        // heap backing is released. Its failure must not expose the keys.
        let terminal_limit = collection_limit - size_of::<Reverse<String>>() as u128 + 64;
        let mut almost = new(1, terminal_limit - 1);
        assert!(almost
            .collect_one(&mut 1, &control, SourceKey::Fixture(&key))
            .unwrap());
        assert!(!almost.advance_output(&mut 1, &control).unwrap());
        assert!(matches!(
            almost.advance_output(&mut 1, &control),
            Err(WasmExactSearchError::ResourceAdmission(_))
        ));
        assert!(almost.into_family(&control).is_err());

        let mut accepted = new(1, terminal_limit);
        assert!(accepted
            .collect_one(&mut 1, &control, SourceKey::Fixture(&key))
            .unwrap());
        assert!(!accepted.advance_output(&mut 1, &control).unwrap());
        assert!(accepted.advance_output(&mut 1, &control).unwrap());
        assert_eq!(accepted.into_family(&control).unwrap().keys, [key]);
    }
}
