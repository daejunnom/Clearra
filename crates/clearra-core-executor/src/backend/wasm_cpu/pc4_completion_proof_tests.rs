//! Test-only exact completion oracle for the thirteen HF-omitted PC4 edges.
//! SRP rationale: this test module's single change reason is independently
//! proving the finite completion status of omitted-transition fixtures.
//!
//! This module deliberately does not participate in product search or profile
//! qualification. It independently constructs a row-lift superset, streams
//! exact covers in MRV order, and then proves each concrete candidate dead or
//! live in its finite ordered-subset graph. A deterministic budget exhaustion
//! is reported as `unknown`; it is never converted into a negative proof.

use super::super::{
    catalog::GeometryCatalog,
    reachability::{
        optimistic_reachable_poses, search_reachable_locks, ReachabilityScratch,
        ReachabilityTemplate, ReachableLocks,
    },
};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_rules::kicks::KickTableProfileId;
use std::collections::{BTreeMap, BTreeSet};

const PROOF_WIDTH: u8 = 10;
const PROOF_HEIGHT: u8 = 4;
const PROOF_ROW_MASK: u64 = (1_u64 << PROOF_WIDTH) - 1;
const PROOF_FIELD_MASK: u64 = (1_u64 << (PROOF_WIDTH * PROOF_HEIGHT)) - 1;

// These are deterministic work bounds, not claims that an exhausted search is
// impossible. The current thirteen-case fixture is expected to stay far below
// them; source drift produces an explicit unknown receipt instead of a false
// dead certificate.
const MAX_COVER_NODES_PER_CASE: usize = 2_000_000;
const MAX_CANDIDATES_PER_CASE: usize = 250_000;
const MAX_ORDERED_SUBSET_STATES_PER_CASE: usize = 8_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionFixtureCase {
    source_case: String,
    omitted_edge_piece: PieceKind,
    target_cells: u64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct IndependentRealization {
    required_deleted_rows: u16,
    rotation: RotationState,
    x: i8,
    target_anchor_y: i8,
}

#[derive(Clone, Debug)]
struct IndependentSkeleton {
    piece: PieceKind,
    cells: u64,
    realizations: Vec<IndependentRealization>,
}

#[derive(Clone, Copy, Debug)]
struct InstantiatedLock {
    mask: u64,
    rotation: RotationState,
    x: i8,
    y: i8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReachabilityMode {
    Optimistic,
    Exact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OrderedOutcome {
    Dead,
    Live,
    Unknown(&'static str),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CompletionOutcome {
    Dead,
    Live(Vec<WitnessStep>),
    Unknown(&'static str),
}

impl CompletionOutcome {
    const fn label(&self) -> &'static str {
        match self {
            Self::Dead => "dead",
            Self::Live(_) => "live",
            Self::Unknown(_) => "unknown",
        }
    }

    const fn reason(&self) -> Option<&'static str> {
        match self {
            Self::Unknown(reason) => Some(reason),
            Self::Dead | Self::Live(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WitnessStep {
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    logical_cells: u64,
    board_before: u64,
    board_after: u64,
    physical_cleared_rows: u16,
    target_deleted_rows: u16,
}

#[derive(Clone, Debug)]
struct CandidateFrame {
    skeleton_ids: Vec<usize>,
    operation_cells: Vec<u64>,
    row_contributors: [u16; PROOF_HEIGHT as usize],
    initial_board: u64,
    height: u8,
    all_placed: usize,
}

impl CandidateFrame {
    fn compile(
        skeleton_ids: &[usize],
        skeletons: &[IndependentSkeleton],
        initial_board: u64,
        height: u8,
    ) -> Self {
        let mut canonical_ids = skeleton_ids.to_vec();
        canonical_ids.sort_unstable();
        let mut operation_cells = Vec::with_capacity(canonical_ids.len());
        let mut row_contributors = [0_u16; PROOF_HEIGHT as usize];
        for (operation_index, &skeleton_id) in canonical_ids.iter().enumerate() {
            let cells = skeletons[skeleton_id].cells;
            operation_cells.push(cells);
            let mut occupied = occupied_rows(PROOF_WIDTH, cells);
            while occupied != 0 {
                let row = occupied.trailing_zeros() as usize;
                occupied &= occupied - 1;
                row_contributors[row] |= 1_u16 << operation_index;
            }
        }
        let all_placed = (1_usize << canonical_ids.len()) - 1;
        Self {
            skeleton_ids: canonical_ids,
            operation_cells,
            row_contributors,
            initial_board,
            height,
            all_placed,
        }
    }

    fn state(&self, subset: usize) -> (u64, u16) {
        let subset_bits = subset as u16;
        let mut logical_board = self.initial_board;
        let mut selected = subset;
        while selected != 0 {
            let operation_index = selected.trailing_zeros() as usize;
            selected &= selected - 1;
            logical_board |= self.operation_cells[operation_index];
        }

        let mut deleted_rows = 0_u16;
        for (row, &contributors) in self
            .row_contributors
            .iter()
            .take(usize::from(self.height))
            .enumerate()
        {
            if contributors != 0 && contributors & !subset_bits == 0 {
                deleted_rows |= 1_u16 << row;
            }
        }
        (
            compact_target_board_independent(PROOF_WIDTH, self.height, logical_board, deleted_rows),
            deleted_rows,
        )
    }
}

#[derive(Clone, Debug, Default)]
struct ProofStats {
    cover_nodes: usize,
    candidate_covers: usize,
    candidate_dead: usize,
    candidate_live: usize,
    candidate_unknown: usize,
    optimistic_pruned_candidates: usize,
    exact_checked_candidates: usize,
    optimistic_subset_states: usize,
    exact_subset_states: usize,
    optimistic_cache_misses: usize,
    exact_cache_misses: usize,
    cover_enumeration_complete: bool,
    candidate_receipt_digest: u64,
}

struct ReachabilityCaches {
    templates: BTreeMap<PieceKind, ReachabilityTemplate>,
    optimistic: BTreeMap<(u64, PieceKind), ReachableLocks>,
    exact: BTreeMap<(u64, PieceKind), ReachableLocks>,
    scratch: ReachabilityScratch,
}

impl ReachabilityCaches {
    fn compile(height: u8) -> Self {
        let templates = PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .map(|piece| {
                (
                    piece,
                    ReachabilityTemplate::compile(
                        PROOF_WIDTH,
                        height,
                        piece,
                        KickTableProfileId::Jstris180,
                    ),
                )
            })
            .collect();
        Self {
            templates,
            optimistic: BTreeMap::new(),
            exact: BTreeMap::new(),
            scratch: ReachabilityScratch::default(),
        }
    }

    fn locks(
        &mut self,
        board: u64,
        piece: PieceKind,
        mode: ReachabilityMode,
        stats: &mut ProofStats,
    ) -> Result<ReachableLocks, &'static str> {
        let key = (board, piece);
        let cache = match mode {
            ReachabilityMode::Optimistic => &self.optimistic,
            ReachabilityMode::Exact => &self.exact,
        };
        if let Some(locks) = cache.get(&key) {
            return Ok(*locks);
        }

        let template = self
            .templates
            .get(&piece)
            .ok_or("completion_proof_missing_reachability_template")?;
        let locks = match mode {
            ReachabilityMode::Optimistic => {
                stats.optimistic_cache_misses += 1;
                optimistic_reachable_poses(template, board)
            }
            ReachabilityMode::Exact => {
                stats.exact_cache_misses += 1;
                let result = search_reachable_locks(template, board, &mut self.scratch, None);
                if !result.exhaustive {
                    return Err("completion_proof_exact_reachability_not_exhaustive");
                }
                result.locks
            }
        };
        match mode {
            ReachabilityMode::Optimistic => {
                self.optimistic.insert(key, locks);
            }
            ReachabilityMode::Exact => {
                self.exact.insert(key, locks);
            }
        }
        Ok(locks)
    }
}

#[derive(Clone, Debug)]
struct CaseReceipt {
    fixture: CompletionFixtureCase,
    source_cleared_prefix: u8,
    physical_height: u8,
    remaining_piece_count: usize,
    independent_row_lift_realizations: usize,
    independent_skeletons: usize,
    stats: ProofStats,
    outcome: CompletionOutcome,
}

impl CaseReceipt {
    fn as_json(&self) -> serde_json::Value {
        let witness = match &self.outcome {
            CompletionOutcome::Live(steps) => steps
                .iter()
                .map(|step| {
                    serde_json::json!({
                        "piece": step.piece.as_ascii().to_string(),
                        "rotation": step.rotation.quarter_turns(),
                        "x": step.x,
                        "y": step.y,
                        "logical_cells": step.logical_cells,
                        "board_before": step.board_before,
                        "board_after": step.board_after,
                        "physical_cleared_rows": step.physical_cleared_rows,
                        "target_deleted_rows": step.target_deleted_rows,
                    })
                })
                .collect::<Vec<_>>(),
            CompletionOutcome::Dead | CompletionOutcome::Unknown(_) => Vec::new(),
        };
        serde_json::json!({
            "source_case": self.fixture.source_case,
            "omitted_edge_piece": self.fixture.omitted_edge_piece.as_ascii().to_string(),
            "target_cells": self.fixture.target_cells,
            "source_cleared_prefix": self.source_cleared_prefix,
            "physical_height": self.physical_height,
            "remaining_piece_count": self.remaining_piece_count,
            "independent_row_lift_realizations": self.independent_row_lift_realizations,
            "independent_skeletons": self.independent_skeletons,
            "cover_nodes": self.stats.cover_nodes,
            "candidate_covers": self.stats.candidate_covers,
            "candidate_outcomes": {
                "dead": self.stats.candidate_dead,
                "live": self.stats.candidate_live,
                "unknown": self.stats.candidate_unknown,
            },
            "optimistic_pruned_candidates": self.stats.optimistic_pruned_candidates,
            "exact_checked_candidates": self.stats.exact_checked_candidates,
            "optimistic_subset_states": self.stats.optimistic_subset_states,
            "exact_subset_states": self.stats.exact_subset_states,
            "optimistic_cache_misses": self.stats.optimistic_cache_misses,
            "exact_cache_misses": self.stats.exact_cache_misses,
            "cover_enumeration_complete": self.stats.cover_enumeration_complete,
            "candidate_receipt_digest": format!("{:016x}", self.stats.candidate_receipt_digest),
            "classification": self.outcome.label(),
            "unknown_reason": self.outcome.reason(),
            "witness": witness,
        })
    }
}

struct CompletionOracle {
    initial_board: u64,
    required_cells: u64,
    height: u8,
    skeletons: Vec<IndependentSkeleton>,
    supports: [Vec<usize>; (PROOF_WIDTH * PROOF_HEIGHT) as usize],
    reachability: ReachabilityCaches,
    stats: ProofStats,
}

impl CompletionOracle {
    fn compile(initial_board: u64, required_cells: u64, height: u8) -> Self {
        let skeletons = independent_row_lift_skeletons(initial_board, required_cells, height);
        let mut supports: [Vec<usize>; (PROOF_WIDTH * PROOF_HEIGHT) as usize] =
            std::array::from_fn(|_| Vec::new());
        for (skeleton_id, skeleton) in skeletons.iter().enumerate() {
            let mut cells = skeleton.cells;
            while cells != 0 {
                let cell = cells.trailing_zeros() as usize;
                cells &= cells - 1;
                supports[cell].push(skeleton_id);
            }
        }
        Self {
            initial_board,
            required_cells,
            height,
            skeletons,
            supports,
            reachability: ReachabilityCaches::compile(height),
            stats: ProofStats {
                candidate_receipt_digest: 0xcbf2_9ce4_8422_2325,
                ..ProofStats::default()
            },
        }
    }

    fn prove(mut self) -> (CompletionOutcome, ProofStats, usize, usize) {
        let realization_count = self
            .skeletons
            .iter()
            .map(|skeleton| skeleton.realizations.len())
            .sum();
        let skeleton_count = self.skeletons.len();
        let mut candidate = Vec::with_capacity((self.required_cells.count_ones() / 4) as usize);
        let outcome = match self.enumerate_covers(self.required_cells, &mut candidate) {
            CompletionOutcome::Dead => {
                self.stats.cover_enumeration_complete = true;
                CompletionOutcome::Dead
            }
            other => other,
        };
        (outcome, self.stats, realization_count, skeleton_count)
    }

    fn enumerate_covers(
        &mut self,
        remaining: u64,
        candidate: &mut Vec<usize>,
    ) -> CompletionOutcome {
        if self.stats.cover_nodes >= MAX_COVER_NODES_PER_CASE {
            return CompletionOutcome::Unknown("completion_proof_cover_node_budget_exhausted");
        }
        self.stats.cover_nodes += 1;
        if remaining == 0 {
            if self.stats.candidate_covers >= MAX_CANDIDATES_PER_CASE {
                self.stats.candidate_unknown += 1;
                return CompletionOutcome::Unknown(
                    "completion_proof_candidate_cover_budget_exhausted",
                );
            }
            self.stats.candidate_covers += 1;
            return self.evaluate_candidate(candidate);
        }

        let mut unselected_cells = remaining;
        let mut selected_cell = None;
        let mut selected_support_count = usize::MAX;
        while unselected_cells != 0 {
            let cell = unselected_cells.trailing_zeros() as usize;
            unselected_cells &= unselected_cells - 1;
            let support_count = self.supports[cell]
                .iter()
                .filter(|&&skeleton_id| self.skeletons[skeleton_id].cells & !remaining == 0)
                .count();
            if support_count == 0 {
                return CompletionOutcome::Dead;
            }
            if support_count < selected_support_count {
                selected_cell = Some(cell);
                selected_support_count = support_count;
            }
        }

        let cell = selected_cell.expect("nonempty exact-cover residual has a cell");
        // Copy the stable support IDs so recursive mutation of the oracle does
        // not borrow the support table for the duration of the traversal.
        let support_ids = self.supports[cell].clone();
        for skeleton_id in support_ids {
            let cells = self.skeletons[skeleton_id].cells;
            if cells & !remaining != 0 {
                continue;
            }
            candidate.push(skeleton_id);
            let outcome = self.enumerate_covers(remaining ^ cells, candidate);
            candidate.pop();
            match outcome {
                CompletionOutcome::Dead => {}
                CompletionOutcome::Live(_) | CompletionOutcome::Unknown(_) => return outcome,
            }
        }
        CompletionOutcome::Dead
    }

    fn evaluate_candidate(&mut self, skeleton_ids: &[usize]) -> CompletionOutcome {
        let frame = CandidateFrame::compile(
            skeleton_ids,
            &self.skeletons,
            self.initial_board,
            self.height,
        );
        let candidate_digest = candidate_digest(&frame, &self.skeletons);
        let mut optimistic_dead = vec![false; frame.all_placed + 1];
        let mut ignored_witness = Vec::new();
        let optimistic = self.search_ordered_subsets(
            &frame,
            0,
            ReachabilityMode::Optimistic,
            &mut optimistic_dead,
            &mut ignored_witness,
        );
        match optimistic {
            OrderedOutcome::Dead => {
                self.stats.optimistic_pruned_candidates += 1;
                self.stats.candidate_dead += 1;
                self.record_candidate(candidate_digest, OrderedOutcome::Dead);
                return CompletionOutcome::Dead;
            }
            OrderedOutcome::Unknown(reason) => {
                self.stats.candidate_unknown += 1;
                self.record_candidate(candidate_digest, OrderedOutcome::Unknown(reason));
                return CompletionOutcome::Unknown(reason);
            }
            OrderedOutcome::Live => {}
        }

        self.stats.exact_checked_candidates += 1;
        let mut exact_dead = vec![false; frame.all_placed + 1];
        let mut witness = Vec::with_capacity(frame.skeleton_ids.len());
        let exact = self.search_ordered_subsets(
            &frame,
            0,
            ReachabilityMode::Exact,
            &mut exact_dead,
            &mut witness,
        );
        match exact {
            OrderedOutcome::Dead => {
                self.stats.candidate_dead += 1;
                self.record_candidate(candidate_digest, OrderedOutcome::Dead);
                CompletionOutcome::Dead
            }
            OrderedOutcome::Live => {
                self.stats.candidate_live += 1;
                self.record_candidate(candidate_digest, OrderedOutcome::Live);
                CompletionOutcome::Live(witness)
            }
            OrderedOutcome::Unknown(reason) => {
                self.stats.candidate_unknown += 1;
                self.record_candidate(candidate_digest, OrderedOutcome::Unknown(reason));
                CompletionOutcome::Unknown(reason)
            }
        }
    }

    fn search_ordered_subsets(
        &mut self,
        frame: &CandidateFrame,
        subset: usize,
        mode: ReachabilityMode,
        dead: &mut [bool],
        witness: &mut Vec<WitnessStep>,
    ) -> OrderedOutcome {
        if subset == frame.all_placed {
            return OrderedOutcome::Live;
        }
        if dead[subset] {
            return OrderedOutcome::Dead;
        }
        let states = match mode {
            ReachabilityMode::Optimistic => &mut self.stats.optimistic_subset_states,
            ReachabilityMode::Exact => &mut self.stats.exact_subset_states,
        };
        if *states >= MAX_ORDERED_SUBSET_STATES_PER_CASE {
            return OrderedOutcome::Unknown("completion_proof_ordered_subset_budget_exhausted");
        }
        *states += 1;

        let (board, deleted_rows) = frame.state(subset);
        for operation_index in 0..frame.skeleton_ids.len() {
            let operation_bit = 1_usize << operation_index;
            if subset & operation_bit != 0 {
                continue;
            }
            let skeleton_id = frame.skeleton_ids[operation_index];
            let piece = self.skeletons[skeleton_id].piece;
            let realization_count = self.skeletons[skeleton_id].realizations.len();
            for realization_index in 0..realization_count {
                let realization = self.skeletons[skeleton_id].realizations[realization_index];
                let Some(lock) = instantiate_independent_realization(
                    self.height,
                    piece,
                    self.skeletons[skeleton_id].cells,
                    realization,
                    deleted_rows,
                ) else {
                    continue;
                };
                if board & lock.mask != 0 {
                    continue;
                }
                let reachable = match self.reachability.locks(board, piece, mode, &mut self.stats) {
                    Ok(locks) => locks,
                    Err(reason) => return OrderedOutcome::Unknown(reason),
                };
                if !reachable.contains(PROOF_WIDTH, lock.rotation, lock.x, lock.y) {
                    continue;
                }

                let (next_board, cleared_current) =
                    place_and_clear_independent(PROOF_WIDTH, self.height, board | lock.mask);
                let Some(next_deleted_rows) =
                    merge_deleted_rows_independent(self.height, deleted_rows, cleared_current)
                else {
                    continue;
                };
                let child = subset | operation_bit;
                let (expected_board, expected_deleted_rows) = frame.state(child);
                if next_board != expected_board || next_deleted_rows != expected_deleted_rows {
                    continue;
                }

                if mode == ReachabilityMode::Exact {
                    witness.push(WitnessStep {
                        piece,
                        rotation: lock.rotation,
                        x: lock.x,
                        y: lock.y,
                        logical_cells: self.skeletons[skeleton_id].cells,
                        board_before: board,
                        board_after: next_board,
                        physical_cleared_rows: cleared_current,
                        target_deleted_rows: next_deleted_rows,
                    });
                }
                let child_outcome = self.search_ordered_subsets(frame, child, mode, dead, witness);
                match child_outcome {
                    OrderedOutcome::Live => return OrderedOutcome::Live,
                    OrderedOutcome::Unknown(reason) => {
                        if mode == ReachabilityMode::Exact {
                            witness.pop();
                        }
                        return OrderedOutcome::Unknown(reason);
                    }
                    OrderedOutcome::Dead => {
                        if mode == ReachabilityMode::Exact {
                            witness.pop();
                        }
                    }
                }
            }
        }
        dead[subset] = true;
        OrderedOutcome::Dead
    }

    fn record_candidate(&mut self, candidate_digest: u64, outcome: OrderedOutcome) {
        self.stats.candidate_receipt_digest =
            mix_receipt(self.stats.candidate_receipt_digest, candidate_digest);
        self.stats.candidate_receipt_digest = mix_receipt(
            self.stats.candidate_receipt_digest,
            match outcome {
                OrderedOutcome::Dead => 0xd3ad,
                OrderedOutcome::Live => 0x11a7,
                OrderedOutcome::Unknown(_) => 0x0bad,
            },
        );
    }
}

fn fixture_cases() -> Vec<CompletionFixtureCase> {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/pc4-hf-unresolved-completions-20260913.json"
    ))
    .expect("unresolved completion fixture parses");
    assert_eq!(
        fixture["schema"],
        "clearra.pc4.hf-unresolved-completions.v1"
    );
    assert_eq!(fixture["profile"], "jstris-180");
    fixture["cases"]
        .as_array()
        .expect("completion cases are an array")
        .iter()
        .map(|case| {
            let piece = case["omitted_edge_piece"]
                .as_str()
                .and_then(|value| value.chars().next())
                .and_then(|value| PieceKind::from_ascii(value).ok())
                .expect("fixture piece is a standard tetromino");
            CompletionFixtureCase {
                source_case: case["source_case"]
                    .as_str()
                    .expect("fixture source case")
                    .to_owned(),
                omitted_edge_piece: piece,
                target_cells: case["target_cells"].as_u64().expect("fixture target cells"),
            }
        })
        .collect()
}

fn prove_case(fixture: CompletionFixtureCase) -> CaseReceipt {
    let (source_cleared_prefix, physical_height, initial_board, required_cells) =
        match normalize_completion_target(fixture.target_cells) {
            Ok(normalized) => normalized,
            Err(reason) => {
                return CaseReceipt {
                    fixture,
                    source_cleared_prefix: 0,
                    physical_height: 0,
                    remaining_piece_count: 0,
                    independent_row_lift_realizations: 0,
                    independent_skeletons: 0,
                    stats: ProofStats::default(),
                    outcome: CompletionOutcome::Unknown(reason),
                };
            }
        };
    let remaining_piece_count = (required_cells.count_ones() / 4) as usize;
    if required_cells == 0 {
        return CaseReceipt {
            fixture,
            source_cleared_prefix,
            physical_height,
            remaining_piece_count,
            independent_row_lift_realizations: 0,
            independent_skeletons: 0,
            stats: ProofStats {
                cover_enumeration_complete: true,
                ..ProofStats::default()
            },
            outcome: CompletionOutcome::Live(Vec::new()),
        };
    }
    let oracle = CompletionOracle::compile(initial_board, required_cells, physical_height);
    let (outcome, stats, realization_count, skeleton_count) = oracle.prove();
    CaseReceipt {
        fixture,
        source_cleared_prefix,
        physical_height,
        remaining_piece_count,
        independent_row_lift_realizations: realization_count,
        independent_skeletons: skeleton_count,
        stats,
        outcome,
    }
}

fn normalize_completion_target(target_cells: u64) -> Result<(u8, u8, u64, u64), &'static str> {
    if target_cells & !PROOF_FIELD_MASK != 0 {
        return Err("completion_proof_target_outside_four_rows");
    }
    let mut prefix = 0_u8;
    while prefix < PROOF_HEIGHT
        && (target_cells >> (usize::from(prefix) * usize::from(PROOF_WIDTH))) & PROOF_ROW_MASK
            == PROOF_ROW_MASK
    {
        prefix += 1;
    }
    for row in prefix..PROOF_HEIGHT {
        if (target_cells >> (usize::from(row) * usize::from(PROOF_WIDTH))) & PROOF_ROW_MASK
            == PROOF_ROW_MASK
        {
            return Err("completion_proof_full_rows_are_not_bottom_prefix");
        }
    }
    let height = PROOF_HEIGHT - prefix;
    let initial_board = target_cells >> (usize::from(prefix) * usize::from(PROOF_WIDTH));
    let cell_count = usize::from(height) * usize::from(PROOF_WIDTH);
    let field_mask = if cell_count == 64 {
        u64::MAX
    } else {
        (1_u64 << cell_count) - 1
    };
    let required_cells = field_mask & !initial_board;
    if !required_cells.count_ones().is_multiple_of(4) {
        return Err("completion_proof_remaining_area_not_tetromino_aligned");
    }
    Ok((prefix, height, initial_board, required_cells))
}

fn independent_row_lift_skeletons(
    initial_board: u64,
    required_cells: u64,
    height: u8,
) -> Vec<IndependentSkeleton> {
    let registry = standard_tetromino_registry();
    let mut grouped: BTreeMap<(PieceKind, u64), BTreeSet<IndependentRealization>> = BTreeMap::new();
    for piece in PieceKind::STANDARD_TETROMINOES {
        let definition = registry.get(piece).expect("standard tetromino exists");
        for rotation in RotationState::ALL {
            let shape = definition.shape(rotation);
            if shape.width() > PROOF_WIDTH || shape.height() > height {
                continue;
            }
            let cells = shape.cells();
            let mut local_rows = cells
                .iter()
                .map(|cell| u8::try_from(cell.y()).expect("standard shape rows are nonnegative"))
                .collect::<Vec<_>>();
            local_rows.sort_unstable();
            local_rows.dedup();
            for x in 0..=PROOF_WIDTH - shape.width() {
                let mut target_rows = [0_u8; 4];
                enumerate_independent_row_lifts(
                    height,
                    initial_board,
                    required_cells,
                    piece,
                    rotation,
                    cells,
                    &local_rows,
                    &mut target_rows,
                    0,
                    x as i8,
                    &mut grouped,
                );
            }
        }
    }
    grouped
        .into_iter()
        .map(|((piece, cells), realizations)| IndependentSkeleton {
            piece,
            cells,
            realizations: realizations.into_iter().collect(),
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn enumerate_independent_row_lifts(
    height: u8,
    initial_board: u64,
    required_cells: u64,
    piece: PieceKind,
    rotation: RotationState,
    cells: [clearra_piece_registry::registry::piece_registry::ShapeCell; 4],
    local_rows: &[u8],
    target_rows: &mut [u8; 4],
    row_index: usize,
    x: i8,
    output: &mut BTreeMap<(PieceKind, u64), BTreeSet<IndependentRealization>>,
) {
    if row_index == local_rows.len() {
        let mut mask = 0_u64;
        for cell in cells {
            let local_row_index = local_rows
                .binary_search(&u8::try_from(cell.y()).expect("standard shape row"))
                .expect("shape row belongs to row-lift domain");
            let target_y = target_rows[local_row_index];
            let target_x = x + cell.x();
            if target_x < 0 || target_x >= PROOF_WIDTH as i8 {
                return;
            }
            mask |= 1_u64 << (usize::from(target_y) * usize::from(PROOF_WIDTH) + target_x as usize);
        }
        if mask & initial_board != 0 || mask & !required_cells != 0 {
            return;
        }
        let mut required_deleted_rows = 0_u16;
        for index in 1..local_rows.len() {
            let local_gap = local_rows[index] - local_rows[index - 1];
            let first_deleted = target_rows[index - 1] + local_gap;
            for row in first_deleted..target_rows[index] {
                required_deleted_rows |= 1_u16 << row;
            }
        }
        output
            .entry((piece, mask))
            .or_default()
            .insert(IndependentRealization {
                required_deleted_rows,
                rotation,
                x,
                target_anchor_y: target_rows[0] as i8,
            });
        return;
    }

    let local_row = local_rows[row_index];
    let last_local_row = local_rows[local_rows.len() - 1];
    let minimum = if row_index == 0 {
        local_row
    } else {
        target_rows[row_index - 1] + local_row - local_rows[row_index - 1]
    };
    let remaining_span = last_local_row - local_row;
    if remaining_span >= height {
        return;
    }
    let maximum = height - 1 - remaining_span;
    for target_row in minimum..=maximum {
        target_rows[row_index] = target_row;
        enumerate_independent_row_lifts(
            height,
            initial_board,
            required_cells,
            piece,
            rotation,
            cells,
            local_rows,
            target_rows,
            row_index + 1,
            x,
            output,
        );
    }
}

fn instantiate_independent_realization(
    height: u8,
    piece: PieceKind,
    target_cells: u64,
    realization: IndependentRealization,
    deleted_rows: u16,
) -> Option<InstantiatedLock> {
    if realization.required_deleted_rows & !deleted_rows != 0
        || occupied_rows(PROOF_WIDTH, target_cells) & deleted_rows != 0
    {
        return None;
    }
    let anchor = u8::try_from(realization.target_anchor_y).ok()?;
    let deleted_below = (deleted_rows & lower_row_mask(anchor)).count_ones() as i8;
    let lock_y = realization.target_anchor_y - deleted_below;
    if lock_y < 0 {
        return None;
    }
    let shape = standard_tetromino_registry()
        .get(piece)?
        .shape(realization.rotation);
    let mut physical = 0_u64;
    let mut projected = 0_u64;
    for cell in shape.cells() {
        let x = realization.x + cell.x();
        let y = lock_y + cell.y();
        if x < 0 || x >= PROOF_WIDTH as i8 || y < 0 || y >= height as i8 {
            return None;
        }
        physical |= 1_u64 << (y as usize * PROOF_WIDTH as usize + x as usize);
        let target_y = target_row_for_current_row(height, deleted_rows, y as u8)?;
        projected |= 1_u64 << (target_y as usize * PROOF_WIDTH as usize + x as usize);
    }
    (projected == target_cells).then_some(InstantiatedLock {
        mask: physical,
        rotation: realization.rotation,
        x: realization.x,
        y: lock_y,
    })
}

fn target_row_for_current_row(height: u8, deleted_rows: u16, current_row: u8) -> Option<u8> {
    let mut visible_row = 0_u8;
    for target_row in 0..height {
        if deleted_rows & (1_u16 << target_row) != 0 {
            continue;
        }
        if visible_row == current_row {
            return Some(target_row);
        }
        visible_row += 1;
    }
    None
}

fn occupied_rows(width: u8, mut cells: u64) -> u16 {
    let mut rows = 0_u16;
    while cells != 0 {
        let cell = cells.trailing_zeros() as usize;
        cells &= cells - 1;
        rows |= 1_u16 << (cell / width as usize);
    }
    rows
}

const fn lower_row_mask(row: u8) -> u16 {
    if row == 0 {
        0
    } else {
        (1_u16 << row) - 1
    }
}

fn compact_target_board_independent(width: u8, height: u8, board: u64, deleted_rows: u16) -> u64 {
    let row_mask = (1_u64 << width) - 1;
    let mut compacted = 0_u64;
    let mut output_row = 0_u8;
    for target_row in 0..height {
        if deleted_rows & (1_u16 << target_row) != 0 {
            continue;
        }
        let row = (board >> (usize::from(target_row) * usize::from(width))) & row_mask;
        compacted |= row << (usize::from(output_row) * usize::from(width));
        output_row += 1;
    }
    compacted
}

fn place_and_clear_independent(width: u8, height: u8, board: u64) -> (u64, u16) {
    let row_mask = (1_u64 << width) - 1;
    let mut cleared_rows = 0_u16;
    let mut compacted = 0_u64;
    let mut output_row = 0_u8;
    for input_row in 0..height {
        let row = (board >> (usize::from(input_row) * usize::from(width))) & row_mask;
        if row == row_mask {
            cleared_rows |= 1_u16 << input_row;
        } else {
            compacted |= row << (usize::from(output_row) * usize::from(width));
            output_row += 1;
        }
    }
    (compacted, cleared_rows)
}

fn merge_deleted_rows_independent(height: u8, previous: u16, current_physical: u16) -> Option<u16> {
    let mut original = 0_u16;
    for current_row in 0..height {
        if current_physical & (1_u16 << current_row) == 0 {
            continue;
        }
        original |= 1_u16 << target_row_for_current_row(height, previous, current_row)?;
    }
    Some(previous | original)
}

fn candidate_digest(frame: &CandidateFrame, skeletons: &[IndependentSkeleton]) -> u64 {
    let mut digest = 0xcbf2_9ce4_8422_2325;
    for &skeleton_id in &frame.skeleton_ids {
        let skeleton = &skeletons[skeleton_id];
        digest = mix_receipt(digest, u64::from(skeleton.piece.as_ascii() as u8));
        digest = mix_receipt(digest, skeleton.cells);
    }
    digest
}

const fn mix_receipt(state: u64, value: u64) -> u64 {
    (state ^ value).wrapping_mul(0x0000_0100_0000_01b3)
}

#[test]
fn unresolved_completion_fixture_has_thirteen_unique_supported_targets() {
    let cases = fixture_cases();
    assert_eq!(cases.len(), 13);
    let mut identities = BTreeSet::new();
    for case in cases {
        assert!(identities.insert((
            case.source_case.clone(),
            case.omitted_edge_piece,
            case.target_cells,
        )));
        let (_, height, initial_board, required_cells) =
            normalize_completion_target(case.target_cells).expect("fixture target is normalized");
        assert_ne!(required_cells, 0);
        let skeletons = independent_row_lift_skeletons(initial_board, required_cells, height);
        assert!(!skeletons.is_empty());
        let supported = skeletons
            .iter()
            .fold(0_u64, |cells, skeleton| cells | skeleton.cells);
        assert_eq!(supported & required_cells, required_cells);
    }
}

#[test]
fn independent_row_lift_contains_product_geometry_for_unresolved_targets() {
    for case in fixture_cases() {
        let (_, height, initial_board, required_cells) =
            normalize_completion_target(case.target_cells).expect("fixture target is normalized");
        let independent = independent_row_lift_skeletons(initial_board, required_cells, height);
        let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
            PROOF_WIDTH,
            height,
            initial_board,
            required_cells,
        )
        .expect("product geometry compiles for fixture target");
        for row_id in 0..catalog.skeleton_count() as u32 {
            let row = catalog.skeleton(row_id);
            let independent_row = independent
                .iter()
                .find(|candidate| candidate.piece == row.piece && candidate.cells == row.cells)
                .expect("independent row lift contains every product skeleton");
            for realization in catalog.realizations(row_id) {
                assert!(independent_row
                    .realizations
                    .contains(&IndependentRealization {
                        required_deleted_rows: realization.required_deleted_rows,
                        rotation: realization.rotation,
                        x: realization.x,
                        target_anchor_y: realization.target_anchor_y,
                    }));
            }
        }
    }
}

#[test]
fn optimistic_reachability_contains_every_exact_fixture_lock() {
    for case in fixture_cases() {
        let (_, height, board, _) =
            normalize_completion_target(case.target_cells).expect("fixture target is normalized");
        for piece in PieceKind::STANDARD_TETROMINOES {
            let template = ReachabilityTemplate::compile(
                PROOF_WIDTH,
                height,
                piece,
                KickTableProfileId::Jstris180,
            );
            let exact =
                search_reachable_locks(&template, board, &mut ReachabilityScratch::default(), None);
            assert!(exact.exhaustive);
            let optimistic = optimistic_reachable_poses(&template, board);
            for rotation in RotationState::ALL {
                for y in 0..height as i8 {
                    for x in 0..PROOF_WIDTH as i8 {
                        assert!(
                            !exact.locks.contains(PROOF_WIDTH, rotation, x, y)
                                || optimistic.contains(PROOF_WIDTH, rotation, x, y),
                            "optimistic superset omitted exact lock case={} target={} piece={} rotation={} x={} y={}",
                            case.source_case,
                            case.target_cells,
                            piece.as_ascii(),
                            rotation.quarter_turns(),
                            x,
                            y,
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "explicit local qualification oracle; prints a receipt before rejecting live or unknown omissions"]
fn classify_all_hf_omitted_pc4_targets_with_exact_completion_receipts() {
    let receipts = fixture_cases()
        .into_iter()
        .map(prove_case)
        .collect::<Vec<_>>();
    let qualification = receipts
        .iter()
        .all(|receipt| receipt.outcome == CompletionOutcome::Dead);
    let receipt = serde_json::json!({
        "schema": "clearra.pc4.hf-unresolved-completion-proof-receipt.v1",
        "profile": "jstris-180",
        "oracle": {
            "geometry": "independent-row-lift-superset",
            "cover": "streaming-mrv-exact-cover",
            "negative_reachability": "all-collision-free-kicks-ungrounded-superset",
            "positive_reachability": "product-exact-first-success-jstris-180",
            "ordered_state": "candidate-operation-subset-dag",
            "cover_node_limit": MAX_COVER_NODES_PER_CASE,
            "candidate_limit": MAX_CANDIDATES_PER_CASE,
            "ordered_subset_state_limit": MAX_ORDERED_SUBSET_STATES_PER_CASE,
            "budget_exhaustion": "unknown",
        },
        "qualification": if qualification { "all-omissions-dead" } else { "not-qualified" },
        "cases": receipts.iter().map(CaseReceipt::as_json).collect::<Vec<_>>(),
    });
    eprintln!("PC4_HF_COMPLETION_PROOF_RECEIPT={receipt}");
    assert!(
        qualification,
        "HF completion qualification requires every omitted target to be exactly dead; inspect receipt"
    );
}
