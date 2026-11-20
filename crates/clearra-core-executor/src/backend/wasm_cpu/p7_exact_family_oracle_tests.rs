//! Exact whole-family P7 check with independent cover, physics and hold.

use super::{
    independent_row_lift_skeletons, instantiate_independent_realization,
    merge_deleted_rows_independent, place_and_clear_independent, CandidateFrame,
    IndependentSkeleton, MAX_CANDIDATES_PER_CASE, MAX_COVER_NODES_PER_CASE,
    MAX_ORDERED_SUBSET_STATES_PER_CASE, PROOF_FIELD_MASK, PROOF_HEIGHT, PROOF_WIDTH,
};
use crate::reachability_reference::reference_spawn_lock_anchors;
use crate::{CoreExecutionResult, WasmCpuSearchBackend};
use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity,
    },
};
use clearra_coverage::pattern::pattern_id::PatternId;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
use clearra_rules::{
    kicks::KickTableProfileId,
    profile::{
        builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
        rule_profile::RuleProfile,
    },
};
use std::collections::{BTreeMap, BTreeSet};

const CTK_ORIGIN: u64 = 0x3c0f_03c0f;
const SEVEN_LOCK_ORIGIN: u64 = 0x3c0f_03c0;
const JOINT_CLEAR_ORIGIN: u64 = 0x90_747d_83e5;
const FIELD_SIZE: usize = PROOF_WIDTH as usize * PROOF_HEIGHT as usize;
const DRAWS: usize = 7;
const QUEUE_COUNT: usize = 5_040;
const WORDS: usize = QUEUE_COUNT.div_ceil(64);
const MAX_PHYSICAL_QUERIES: usize = 250_000;
type Ordinals = [u64; WORDS];
type Queue = [PieceKind; DRAWS];

#[derive(Clone, Copy)]
struct P7Origin {
    initial: u64,
    required: u64,
    locks: usize,
}

impl P7Origin {
    fn new(initial: u64) -> Self {
        assert_eq!(initial & !PROOF_FIELD_MASK, 0);
        for row in 0..PROOF_HEIGHT {
            let cells = (initial >> (PROOF_WIDTH * row)) & super::PROOF_ROW_MASK;
            // Every initially occupied row is mandatory. If the only
            // empty row is the top one, it is mandatory too: skipping it
            // would leave an empty physical gap and no support for the
            // first lock wholly above the gap.
            assert_ne!(cells, super::PROOF_ROW_MASK, "precleared row");
            assert!(cells != 0 || row + 1 == PROOF_HEIGHT, "empty internal row");
        }
        let required = PROOF_FIELD_MASK & !initial;
        let count = required.count_ones() as usize;
        assert!(count.is_multiple_of(4));
        let locks = count / 4;
        assert!((1..=DRAWS).contains(&locks));
        Self {
            initial,
            required,
            locks,
        }
    }
}

fn piece_index(piece: PieceKind) -> usize {
    PieceKind::STANDARD_TETROMINOES
        .iter()
        .position(|&kind| kind == piece)
        .expect("standard tetromino")
}

fn append_word(word: u32, piece: PieceKind, length: usize) -> u32 {
    // Nonzero piece code keeps every prefix distinct from shorter words.
    word | (((piece_index(piece) + 1) as u32) << (3 * length))
}

fn source_queues() -> Vec<Queue> {
    fn enumerate(queues: &mut Vec<Queue>, queue: &mut Queue, length: usize, used: u8) {
        if length == DRAWS {
            queues.push(*queue);
            return;
        }
        for (index, piece) in PieceKind::STANDARD_TETROMINOES.into_iter().enumerate() {
            let bit = 1_u8 << index;
            if used & bit == 0 {
                queue[length] = piece;
                enumerate(queues, queue, length + 1, used | bit);
            }
        }
    }
    let mut queues = Vec::with_capacity(QUEUE_COUNT);
    enumerate(&mut queues, &mut [PieceKind::I; DRAWS], 0, 0);
    queues
}

fn playable_words(queues: &[Queue], locks: usize, allow_hold: bool) -> BTreeMap<u32, Ordinals> {
    fn visit(
        queue: &Queue,
        cursor: usize,
        held: Option<PieceKind>,
        length: usize,
        word: u32,
        locks: usize,
        allow_hold: bool,
        words: &mut BTreeSet<u32>,
    ) {
        if length == locks {
            words.insert(word);
            return;
        }
        let Some(&current) = queue.get(cursor) else {
            // An exact seven-lock PC may release one known held piece
            // against an unplaced next-bag lookahead. The hidden draw
            // cannot itself be placed and does not create a new ordinal.
            if allow_hold && locks == DRAWS && length + 1 == locks {
                if let Some(last) = held {
                    words.insert(append_word(word, last, length));
                }
            }
            return;
        };
        visit(
            queue,
            cursor + 1,
            held,
            length + 1,
            append_word(word, current, length),
            locks,
            allow_hold,
            words,
        );
        if !allow_hold {
            return;
        }
        if let Some(piece) = held {
            visit(
                queue,
                cursor + 1,
                Some(current),
                length + 1,
                append_word(word, piece, length),
                locks,
                allow_hold,
                words,
            );
        } else if let Some(&next) = queue.get(cursor + 1) {
            visit(
                queue,
                cursor + 2,
                Some(current),
                length + 1,
                append_word(word, next, length),
                locks,
                allow_hold,
                words,
            );
        }
    }
    let mut coverage = BTreeMap::new();
    for (ordinal, queue) in queues.iter().enumerate() {
        let mut words = BTreeSet::new();
        visit(queue, 0, None, 0, 0, locks, allow_hold, &mut words);
        for word in words {
            coverage.entry(word).or_insert([0; WORDS])[ordinal / 64] |= 1_u64 << (ordinal % 64);
        }
    }
    coverage
}

struct FamilyOracle<'a> {
    origin: P7Origin,
    profile: KickTableProfileId,
    skeletons: Vec<IndependentSkeleton>,
    supports: [Vec<usize>; FIELD_SIZE],
    anchors: BTreeMap<(u64, PieceKind), [u64; 4]>,
    word_coverage: &'a BTreeMap<u32, Ordinals>,
    solutions: BTreeMap<StandardBoard64TilingIdentity, Ordinals>,
    cover_nodes: usize,
    covers: usize,
    ordered_subsets: usize,
    first_clear_masks: BTreeSet<u16>,
}

impl<'a> FamilyOracle<'a> {
    fn new(
        origin: P7Origin,
        profile: KickTableProfileId,
        word_coverage: &'a BTreeMap<u32, Ordinals>,
    ) -> Self {
        let skeletons =
            independent_row_lift_skeletons(origin.initial, origin.required, PROOF_HEIGHT);
        let mut supports: [Vec<usize>; FIELD_SIZE] = std::array::from_fn(|_| Vec::new());
        for (index, skeleton) in skeletons.iter().enumerate() {
            let mut cells = skeleton.cells;
            while cells != 0 {
                supports[cells.trailing_zeros() as usize].push(index);
                cells &= cells - 1;
            }
        }
        Self {
            origin,
            profile,
            skeletons,
            supports,
            anchors: BTreeMap::new(),
            word_coverage,
            solutions: BTreeMap::new(),
            cover_nodes: 0,
            covers: 0,
            ordered_subsets: 0,
            first_clear_masks: BTreeSet::new(),
        }
    }

    fn enumerate(&mut self, remaining: u64, used_pieces: u8, selected: &mut Vec<usize>) {
        self.cover_nodes += 1;
        assert!(
            self.cover_nodes <= MAX_COVER_NODES_PER_CASE,
            "oracle cover-node limit"
        );
        if remaining == 0 {
            self.covers += 1;
            assert!(self.covers <= MAX_CANDIDATES_PER_CASE, "oracle cover limit");
            self.evaluate(selected);
            return;
        }
        let cell = remaining.trailing_zeros() as usize;
        for index in 0..self.supports[cell].len() {
            let next = self.supports[cell][index];
            let row = &self.skeletons[next];
            let piece = 1_u8 << piece_index(row.piece);
            if row.cells & !remaining != 0 || used_pieces & piece != 0 {
                continue;
            }
            let updated = remaining ^ row.cells;
            selected.push(next);
            self.enumerate(updated, used_pieces | piece, selected);
            selected.pop();
        }
    }

    fn evaluate(&mut self, selected: &[usize]) {
        assert_eq!(selected.len(), self.origin.locks);
        let frame =
            CandidateFrame::compile(selected, &self.skeletons, self.origin.initial, PROOF_HEIGHT);
        let mut ordinals = [0; WORDS];
        self.visit_physical_words(&frame, 0, 0, 0, &mut [None; 1 << DRAWS], &mut ordinals);
        if ordinals.iter().all(|&word| word == 0) {
            return;
        }
        let identity = StandardBoard64TilingIdentity::from_placements(
            self.origin.initial,
            selected.iter().map(|&index| {
                let row = &self.skeletons[index];
                PiecePlacementMask::new(row.piece, row.cells)
            }),
        )
        .expect("disjoint exact cover");
        assert!(
            self.solutions.insert(identity, ordinals).is_none(),
            "duplicate oracle cover"
        );
    }

    fn visit_physical_words(
        &mut self,
        frame: &CandidateFrame,
        subset: usize,
        word: u32,
        first_clear: u16,
        edges: &mut [Option<u16>; 1 << DRAWS],
        covered: &mut Ordinals,
    ) {
        if subset == frame.all_placed {
            if let Some(available) = self.word_coverage.get(&word) {
                assert_ne!(
                    first_clear, 0,
                    "complete PC must have cleared the original field"
                );
                self.first_clear_masks.insert(first_clear);
                for (dest, &source) in covered.iter_mut().zip(available) {
                    *dest |= source;
                }
            }
            return;
        }
        let mut possible = match edges[subset] {
            Some(known) => known,
            None => {
                let known = self.allowed_operations(frame, subset);
                edges[subset] = Some(known);
                known
            }
        };
        while possible != 0 {
            let operation = possible.trailing_zeros() as usize;
            possible &= possible - 1;
            let piece = self.skeletons[frame.skeleton_ids[operation]].piece;
            let updated = append_word(word, piece, subset.count_ones() as usize);
            let next = subset | (1 << operation);
            let first = if first_clear == 0 {
                frame.state(next).1
            } else {
                first_clear
            };
            self.visit_physical_words(frame, next, updated, first, edges, covered);
        }
    }

    fn allowed_operations(&mut self, frame: &CandidateFrame, subset: usize) -> u16 {
        self.ordered_subsets += 1;
        assert!(
            self.ordered_subsets <= MAX_ORDERED_SUBSET_STATES_PER_CASE,
            "oracle subset limit"
        );
        let (board, deleted) = frame.state(subset);
        let mut allowed = 0;
        for (operation, &id) in frame.skeleton_ids.iter().enumerate() {
            if subset & (1 << operation) != 0 {
                continue;
            }
            let row = &self.skeletons[id];
            let expected = frame.state(subset | (1 << operation));
            for &realization in &row.realizations {
                let Some(lock) = instantiate_independent_realization(
                    PROOF_HEIGHT,
                    row.piece,
                    row.cells,
                    realization,
                    deleted,
                ) else {
                    continue;
                };
                if board & lock.mask != 0 {
                    continue;
                }
                let queries = self.anchors.len();
                let profile = self.profile;
                let anchors = self.anchors.entry((board, row.piece)).or_insert_with(|| {
                    assert!(
                        queries < MAX_PHYSICAL_QUERIES,
                        "oracle physical query limit"
                    );
                    reference_spawn_lock_anchors(
                        PROOF_WIDTH,
                        PROOF_HEIGHT,
                        board,
                        row.piece,
                        profile,
                    )
                    .expect("independent ordered-kick locks")
                });
                let anchor = lock.y as usize * PROOF_WIDTH as usize + lock.x as usize;
                if anchors[lock.rotation.quarter_turns() as usize] & (1 << anchor) == 0 {
                    continue;
                }
                let (next_board, cleared) =
                    place_and_clear_independent(PROOF_WIDTH, PROOF_HEIGHT, board | lock.mask);
                if (
                    next_board,
                    merge_deleted_rows_independent(PROOF_HEIGHT, deleted, cleared),
                ) == (expected.0, Some(expected.1))
                {
                    allowed |= 1_u16 << operation;
                    break;
                }
            }
        }
        allowed
    }
}

fn check_ordinary_result(result: &CoreExecutionResult, oracle: &FamilyOracle<'_>) {
    for flag in [
        "count_complete",
        "probability_complete",
        "solution_probability_complete",
    ] {
        assert_eq!(result.bool_field(flag), Some(true), "{flag}");
    }
    assert_eq!(result.bool_field("resource_truncated"), Some(false));
    assert_eq!(
        result.usize_field("coverage_pattern_count"),
        Some(QUEUE_COUNT)
    );
    let mut ids = BTreeSet::new();
    let mut union = [0; WORDS];
    for row in result.solution_coverages() {
        let identity = row.identity();
        assert!(
            ids.insert(identity),
            "duplicate product identity: {identity:?}"
        );
        let expected = oracle
            .solutions
            .get(&identity)
            .expect("unexpected product identity");
        assert_eq!(
            row.covered_patterns().words(),
            expected,
            "ordinal mismatch for {identity:?}"
        );
        for (out, &mask) in union.iter_mut().zip(expected) {
            *out |= mask;
        }
    }
    assert_eq!(ids, oracle.solutions.keys().copied().collect());
    assert_eq!(result.coverage_pattern_words(), union);
    let covered = union
        .iter()
        .map(|word| word.count_ones() as usize)
        .sum::<usize>();
    assert_eq!(result.usize_field("covered_pattern_count"), Some(covered));
    let chance: f64 = result
        .field("coverage_probability")
        .unwrap()
        .parse()
        .unwrap();
    assert!((chance - covered as f64 / QUEUE_COUNT as f64).abs() <= 1e-12);
}

fn check_rendered_probabilities(result: &CoreExecutionResult, oracle: &FamilyOracle<'_>) {
    assert_eq!(
        result.usize_field("normalized_unique_solution_count"),
        Some(oracle.solutions.len())
    );
    let mut expected_probabilities: BTreeMap<_, _> = oracle
        .solutions
        .iter()
        .map(|(&identity, masks)| {
            (
                NormalizedTilingSolutionKey::from_standard_board64_identity(identity).to_string(),
                masks
                    .iter()
                    .map(|word| word.count_ones() as usize)
                    .sum::<usize>(),
            )
        })
        .collect();
    for report in result.solution_probabilities() {
        let expected = expected_probabilities
            .remove(report.solution_key())
            .expect("exactly one report per oracle identity");
        assert!(report.probability_complete());
        assert_eq!(report.covered_pattern_count(), expected);
        assert_eq!(report.pattern_count(), QUEUE_COUNT);
        let chance: f64 = report.probability().parse().unwrap();
        assert!(
            (chance - expected as f64 / QUEUE_COUNT as f64).abs() <= 1e-12,
            "{}",
            report.solution_key()
        );
    }
    assert!(
        expected_probabilities.is_empty(),
        "missing oracle probability reports"
    );
}

#[test]
fn p7_exact_family_oracle_matches_all_packed_ordinals() {
    let queues = source_queues();
    assert_eq!(queues.len(), QUEUE_COUNT);
    let origin = P7Origin::new(CTK_ORIGIN);
    let with_hold = playable_words(&queues, origin.locks, true);
    let without_hold = playable_words(&queues, origin.locks, false);
    for (profile, rule, allow_hold) in [
        (KickTableProfileId::Jstris180, jstris_180(), true),
        (KickTableProfileId::Srs90, srs(), true),
        (KickTableProfileId::SrsPlus, srs_plus(), true),
        (KickTableProfileId::SrsX, srs_x(), true),
        (KickTableProfileId::NoKick, no_kick(), true),
        (KickTableProfileId::Jstris180, jstris_180(), false),
    ] {
        verify_oracle_case(
            origin,
            profile,
            rule,
            allow_hold,
            &queues,
            if allow_hold {
                &with_hold
            } else {
                &without_hold
            },
        );
    }
}

struct OracleCaseOutcome {
    covered: usize,
    first_clear_masks: BTreeSet<u16>,
}

fn verify_oracle_case(
    origin: P7Origin,
    profile: KickTableProfileId,
    rule: RuleProfile,
    allow_hold: bool,
    queues: &[Queue],
    word_coverage: &BTreeMap<u32, Ordinals>,
) -> OracleCaseOutcome {
    use super::super::super::symbolic_prefix_orders::{
        with_packed_upper_target, with_strict_prefix_order,
    };
    let mut oracle = FamilyOracle::new(origin, profile, word_coverage);
    oracle.enumerate(origin.required, 0, &mut Vec::new());
    assert!(!oracle.solutions.is_empty());
    let problem = ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(u16::from(PROOF_HEIGHT), origin.initial),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(origin.locks),
        )
        .with_rule(rule)
        .with_allow_hold(allow_hold)
        .with_exact_pieces(Some(origin.locks))
        .with_supply_window_size(SupplyWindowSize::new(DRAWS))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_retained_trace_limit(0)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false)
                .with_workers(1)
                .with_exact_legal_board_enabled(false)
                .with_conditioned_reachability_enabled(false)
                .with_tablebase_requested(false)
                .with_max_memory_mib(Some(256)),
        ),
    )
    .unwrap()
    .with_output_policy(SearchOutputPolicy::CoverageSummary);
    assert_eq!(
        problem.supply().projects_unplaced_lookahead(),
        allow_hold && origin.locks == DRAWS,
        "terminal hold projection for {:#x}",
        origin.initial,
    );
    let universe = problem
        .piece_source()
        .materialized_universe()
        .expect("P7 universe");
    assert_eq!(universe.pattern_count(), QUEUE_COUNT);
    let weight = universe.weights().weight(PatternId::new(0)).unwrap();
    for (ordinal, queue) in queues.iter().enumerate() {
        assert_eq!(universe.sequence_at(ordinal).as_ref(), queue);
        assert_eq!(
            universe.weights().weight(PatternId::new(ordinal)),
            Some(weight)
        );
    }
    let run = || {
        WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
            .expect("complete P7 run")
    };
    let ordinary = run();
    check_ordinary_result(&ordinary, &oracle);
    let (packed, work) = with_packed_upper_target(|| with_strict_prefix_order(run));
    check_ordinary_result(&packed, &oracle);
    assert_eq!(packed.solution_coverages(), ordinary.solution_coverages());
    assert_eq!(
        packed.field("coverage_probability"),
        ordinary.field("coverage_probability")
    );
    assert_eq!(packed.usize_field("workers_used"), Some(1));
    assert_eq!(packed.usize_field("total_build_order_nodes"), Some(0));
    assert_eq!(packed.usize_field("total_reachability_states"), Some(0));
    assert!(work.candidates_certified >= oracle.solutions.len());
    assert_eq!(work.candidates_attempted, work.candidates_certified);
    assert_eq!(
        Some(work.candidates_certified),
        packed.usize_field("packing_candidate_count"),
    );
    // CoverageSummary deliberately keeps the typed identity/bitset rows
    // without materializing presentation identities. Check actual numerical
    // per-solution reports separately on the ordinary retaining projection.
    // Both proof arms already matched every equal-weight numerator bit.
    let detailed_problem = problem
        .clone()
        .with_output_policy(SearchOutputPolicy::Summary);
    let detailed =
        WasmCpuSearchBackend::execute_with_control(&detailed_problem, &ExecutionControl::default())
            .expect("ordinary P7 reports");
    check_rendered_probabilities(&detailed, &oracle);
    let covered = ordinary.usize_field("covered_pattern_count").unwrap();
    eprintln!(
        "INDEPENDENT_P7_FAMILY board={:#x} locks={} profile={profile:?} hold={allow_hold} skeletons={} cover_nodes={} covers={} subsets={} physical_queries={} identities={} covered_ordinals={covered}/{QUEUE_COUNT} first_clears={:?}",
        origin.initial, origin.locks, oracle.skeletons.len(), oracle.cover_nodes, oracle.covers,
        oracle.ordered_subsets, oracle.anchors.len(), oracle.solutions.len(), oracle.first_clear_masks,
    );
    OracleCaseOutcome {
        covered,
        first_clear_masks: oracle.first_clear_masks,
    }
}

#[test]
fn p7_oracle_handles_terminal_hold_and_different_original_row_orders() {
    let queues = source_queues();
    let seven = P7Origin::new(SEVEN_LOCK_ORIGIN);
    assert_eq!(seven.locks, DRAWS);
    let first_queue = queues[0];
    let forward_word = first_queue
        .iter()
        .enumerate()
        .fold(0, |word, (i, &piece)| append_word(word, piece, i));
    let release_last_word = first_queue
        .into_iter()
        .cycle()
        .skip(1)
        .take(DRAWS)
        .enumerate()
        .fold(0, |word, (i, piece)| append_word(word, piece, i));
    let held_probe = playable_words(&[first_queue], seven.locks, true);
    let direct_probe = playable_words(&[first_queue], seven.locks, false);
    assert_eq!(held_probe.len(), 64);
    assert!(held_probe.contains_key(&release_last_word));
    assert_eq!(direct_probe.len(), 1);
    assert!(direct_probe.contains_key(&forward_word));
    assert!(!direct_probe.contains_key(&release_last_word));
    let mut covered = [0; 2];
    for (index, allow_hold) in [false, true].into_iter().enumerate() {
        covered[index] = verify_oracle_case(
            seven,
            KickTableProfileId::SrsPlus,
            srs_plus(),
            allow_hold,
            &queues,
            &playable_words(&queues, seven.locks, allow_hold),
        )
        .covered;
    }
    assert!(
        covered[1] > covered[0],
        "last held placement must reach additional queues"
    );
    let asymmetric = P7Origin::new(JOINT_CLEAR_ORIGIN);
    assert_eq!(asymmetric.locks, 5);
    let independent = verify_oracle_case(
        asymmetric,
        KickTableProfileId::Srs90,
        srs(),
        true,
        &queues,
        &playable_words(&queues, asymmetric.locks, true),
    );
    assert!(independent
        .first_clear_masks
        .iter()
        .any(|&mask| mask & 1 == 0));
    assert!(
        independent.first_clear_masks.len() > 1,
        "fixture needs distinct successful first-clear frames"
    );
}
