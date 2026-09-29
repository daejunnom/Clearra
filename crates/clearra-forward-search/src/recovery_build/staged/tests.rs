use super::super::{RecoveryBuildFields, RecoveryBuildFixedQuery, RecoveryBuildQuery};
use super::*;
use crate::{
    board::{place_and_clear, ForwardBoard},
    CrossStageEarlyLimit,
};
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask as Mask, piece::piece_kind::PieceKind,
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;
fn mask(n: u64) -> Mask {
    Mask::from_words([n, 0, 0, 0])
}
fn query() -> RecoveryBuildQuery {
    RecoveryBuildQuery {
        all_solutions: false,
        minimum_solutions: false,
        required_solution_keys: Vec::new(),
        minimum_source_identity: None,
        fields: RecoveryBuildFields {
            height: 8,
            initial: mask(0),
            middle: mask(15),
            result: mask(0xc030),
        },
        first_supply: "[IO]".into(),
        second_supply: "[IO]".into(),
        early_limit: CrossStageEarlyLimit::Auto,
        hold_enabled: true,
        allow_piece_exchange: true,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    }
}
fn reflected(mask: Mask, height: u8) -> Mask {
    let mut result = Mask::EMPTY;
    for y in 0..height {
        for x in 0..10 {
            if mask.contains_index(u16::from(y) * 10 + x) {
                result = result.union(Mask::singleton(u16::from(y) * 10 + (9 - x)).unwrap());
            }
        }
    }
    result
}
fn orientations(q: &RecoveryBuildQuery) -> Vec<RecoveryBuildFields> {
    let mut candidates = vec![q.fields.clone()];
    if reflected(q.fields.initial, q.fields.height) == q.fields.initial {
        let mut f = q.fields.clone();
        f.middle = reflected(f.middle, f.height);
        f.result = reflected(f.result, f.height);
        candidates.push(f);
    }
    let mut result = Vec::new();
    for original in candidates {
        let (base, _, _) = place_and_clear(
            10,
            original.height,
            ForwardBoard::from_mask(original.initial.union(original.middle)),
        );
        let base = Mask::from_words(base.words());
        if !result.contains(&original) {
            result.push(original.clone());
        }
        if reflected(base, original.height) == base {
            let mut f = original;
            f.result = reflected(f.result, f.height);
            if !result.contains(&f) {
                result.push(f);
            }
        }
    }
    result
}

fn oracle(
    q: &RecoveryBuildQuery,
    first: &[PieceKind],
    second: &[PieceKind],
) -> (RecoveryBuildStatus, usize) {
    let mut status = RecoveryBuildStatus::NoPath;
    let mut states = 0;
    for fields in orientations(q) {
        let result = RecoveryBuildFixedQuery {
            fields,
            first_supply: first.to_vec(),
            second_supply: second.to_vec(),
            early_limit: q.early_limit,
            allow_piece_exchange: q.allow_piece_exchange,
            hold_enabled: q.hold_enabled,
            preserve_b2b: q.preserve_b2b,
            initial_b2b: q.initial_b2b,
            rule_profile: q.rule_profile,
            spin_profile: q.spin_profile,
        }
        .search(&ExecutionControl::default())
        .unwrap();
        states += result.states;
        if result.status == RecoveryBuildStatus::Normal {
            return (result.status, states);
        }
        if result.status == RecoveryBuildStatus::Recovery {
            status = result.status;
        }
    }
    (status, states)
}
fn compare(q: RecoveryBuildQuery) {
    let control = ExecutionControl::default();
    let p = PreparedPopulation::new(q.clone()).unwrap();
    assert!(p.first.pattern_count() <= 32);
    let geometry = Geometry::new(&q, &control).unwrap();
    let mut block = Block::new(&p, geometry, 0, p.first.pattern_count(), 3, &control).unwrap();
    while !block.advance(&control).unwrap() {}
    let overlap = block
        .solver
        .diagram
        .intersect(block.solver.normal, block.solver.recovery)
        .unwrap();
    assert_eq!(overlap, diagram::NONE);
    let mut expected = [0_u128; 3];
    let mut slow_states = 0;
    for i in 0..p.first.pattern_count() {
        let first = p.first.sequence_at(i);
        let normal =
            block
                .solver
                .source
                .follow_first(&block.solver.diagram, block.solver.normal, &first);
        let recovery =
            block
                .solver
                .source
                .follow_first(&block.solver.diagram, block.solver.recovery, &first);
        for j in 0..p.second.pattern_count() {
            let second = p.second.sequence_at(j);
            let actual =
                if block
                    .solver
                    .source
                    .accepts_second(&block.solver.diagram, normal, &second)
                {
                    RecoveryBuildStatus::Normal
                } else if block.solver.source.accepts_second(
                    &block.solver.diagram,
                    recovery,
                    &second,
                ) {
                    RecoveryBuildStatus::Recovery
                } else {
                    RecoveryBuildStatus::NoPath
                };
            let (answer, states) = oracle(&q, &first, &second);
            slow_states += states;
            assert_eq!(actual, answer, "first={first:?} second={second:?} q={q:?}");
            expected[match answer {
                RecoveryBuildStatus::Normal => 0,
                RecoveryBuildStatus::Recovery => 1,
                RecoveryBuildStatus::NoPath => 2,
            }] += 1;
        }
    }
    let (result, _) = block.finish(&p, &control).unwrap();
    assert_eq!(result.counts, expected);
    assert!((result.probabilities.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    for example in [result.normal, result.recovery].into_iter().flatten() {
        assert_eq!(
            oracle(&q, &example.first_queue, &example.second_queue).0,
            example.path.status
        );
        let (mut board, _, _) = place_and_clear(
            10,
            q.fields.height,
            ForwardBoard::from_mask(q.fields.initial),
        );
        let supply = example
            .first_queue
            .iter()
            .chain(&example.second_queue)
            .copied()
            .collect::<Vec<_>>();
        let mut consumed = std::collections::HashSet::new();
        for step in &example.path.steps {
            assert!(consumed.insert(step.source_index));
            assert_eq!(supply[step.source_index], step.piece);
            assert_eq!(board.words(), step.board_before);
            let placement = ForwardBoard::from_words(step.placement);
            assert_eq!(
                step.placement.iter().map(|w| w.count_ones()).sum::<u32>(),
                4
            );
            let (next, rows, lines) = place_and_clear(
                10,
                q.fields.height,
                board.union_for_height(placement, q.fields.height),
            );
            assert_eq!(next.words(), step.board_after);
            assert_eq!((rows, lines), (step.cleared_rows, step.cleared_lines));
            board = next;
        }
        assert_eq!(board.words(), example.path.terminal_board);
    }
    eprintln!(
        "stage comparison: pairs={} states={} reference_states={slow_states}",
        p.possible, result.states
    );
}
#[test]
fn recovery_build_staged_decision_diagram_exhaustive_set_algebra() {
    let mut d = diagram::Diagram::default();
    for seed in 0..16_u64 {
        let mut a = diagram::NONE;
        let mut b = diagram::NONE;
        let mut aa = Vec::new();
        let mut bb = Vec::new();
        for x in 0..7 {
            for y in 0..7 {
                let n = x * 7 + y;
                let a_member = (n as u64 * 17 + seed * 11) % 5 < 2;
                let b_member = (n as u64 * 13 + seed * 7) % 7 < 3;
                aa.push(a_member);
                bb.push(b_member);
                let tail = d.prepend(1, y, diagram::ALL).unwrap();
                let word = d.prepend(0, x, tail).unwrap();
                if a_member {
                    a = d.union(a, word).unwrap();
                }
                if b_member {
                    b = d.union(b, word).unwrap();
                }
            }
        }
        let or = d.union(a, b).unwrap();
        let and = d.intersect(a, b).unwrap();
        let diff = d.difference(a, b).unwrap();
        let not = d.difference(diagram::ALL, a).unwrap();
        for (root, expected) in [
            (
                or,
                aa.iter()
                    .zip(&bb)
                    .map(|(a, b)| *a || *b)
                    .collect::<Vec<_>>(),
            ),
            (and, aa.iter().zip(&bb).map(|(a, b)| *a && *b).collect()),
            (diff, aa.iter().zip(&bb).map(|(a, b)| *a && !*b).collect()),
            (not, aa.iter().map(|a| !*a).collect()),
        ] {
            assert_eq!(
                d.count(root, 0, 2).unwrap(),
                expected.iter().filter(|&&v| v).count() as u128
            );
            for x in 0..7 {
                for y in 0..7 {
                    assert_eq!(
                        d.follow(d.follow(root, 0, x), 1, y) == diagram::ALL,
                        expected[x * 7 + y]
                    );
                }
            }
        }
    }
}
#[test]
fn recovery_build_staged_p7_remainder_is_not_reexpanded_as_fresh_bag() {
    let mut q = query();
    q.first_supply = "I".into();
    q.second_supply = "P7".into();
    let p = PreparedPopulation::new(q).unwrap();
    let mut d = diagram::Diagram::default();
    let source = Source::compile(
        &mut d,
        &p.first,
        &p.second,
        0,
        1,
        &ExecutionControl::default(),
    )
    .unwrap();
    assert!(source.compact_second);
    assert!(d.node_count() <= 129, "nodes={}", d.node_count());
    assert_eq!(d.count(source.second, 1, 8).unwrap(), 5040);
    for piece in 0..7 {
        let suffix = d.follow(source.second, 1, piece);
        assert_eq!(d.count(suffix, 2, 8).unwrap(), 720);
        assert_eq!(
            d.follow(suffix, 2, piece),
            diagram::NONE,
            "borrowed token cannot be drawn again"
        );
    }
}
#[test]
fn recovery_build_staged_matches_fixed_oracle_hold_exchange_clear_and_b2b() {
    for hold in [false, true] {
        for exchange in [false, true] {
            for b2b in [false, true] {
                for base in [0, 0x3f0] {
                    let mut q = query();
                    q.hold_enabled = hold;
                    q.allow_piece_exchange = exchange;
                    q.preserve_b2b = b2b;
                    q.fields.initial = mask(base);
                    compare(q);
                }
            }
        }
    }
}
#[test]
fn recovery_build_staged_multiple_early_and_held_terminal_are_not_lost() {
    for count in [2_usize, 3] {
        let mut q = query();
        let columns = (0..count).fold(0_u64, |m, x| m | (1 << 2 * x));
        q.fields.middle = mask((0..4).fold(0, |m, y| m | (columns << 10 * y)));
        q.fields.result = mask((0..count * 2).fold(0, |m, y| m | (0x300 << 10 * y)));
        q.first_supply = "O".repeat(count);
        q.second_supply = "I".repeat(count);
        for hold in [false, true] {
            for limit in [
                CrossStageEarlyLimit::Auto,
                CrossStageEarlyLimit::AtMost(count - 1),
            ] {
                q.hold_enabled = hold;
                q.early_limit = limit;
                compare(q.clone());
            }
        }
    }
    let mut q = query();
    q.first_supply = "[IO]".into();
    q.second_supply = "[IO]I".into();
    compare(q);
}
#[test]
fn recovery_build_staged_generated_small_boards_match_every_input_pair() {
    use crate::reachability::ReachabilityWorkspace;
    for seed in 0..8_u64 {
        let mut q = query();
        q.first_supply = "*".into();
        q.second_supply = "*".into();
        q.hold_enabled = seed & 1 != 0;
        q.allow_piece_exchange = seed & 2 != 0;
        q.preserve_b2b = seed & 4 != 0;
        let initial = ForwardBoard::from_mask(mask((seed * 97 + 37) & 1023));
        let mut reach = ReachabilityWorkspace::new(8, RuleProfileId::SrsPlus).unwrap();
        let locks = reach
            .reachable_locks(initial, source::PIECES[seed as usize % 7], true, true)
            .to_vec();
        let first = locks[(seed as usize * 13) % locks.len()];
        let (after, _, _) = place_and_clear(10, 8, initial.union_for_height(first.mask, 8));
        let locks = reach
            .reachable_locks(after, source::PIECES[(seed as usize + 3) % 7], true, true)
            .to_vec();
        let second = locks[(seed as usize * 17) % locks.len()];
        q.fields.initial = Mask::from_words(initial.words());
        q.fields.middle = Mask::from_words(first.mask.words());
        q.fields.result = Mask::from_words(second.mask.words());
        compare(q);
    }
}
#[test]
fn recovery_build_staged_original_image_coordinates_remain_unchanged() {
    let mut q = query();
    q.fields.height = 10;
    q.fields.initial = mask(0xc0383f3fc7);
    q.fields.middle = mask(0x3ff3fc7c0c038);
    let result = 0xc120fc1fcfe3c000000000000_u128;
    q.fields.result = Mask::from_words([result as u64, (result >> 64) as u64, 0, 0]);
    q.first_supply = "ITOLSZJ".into();
    q.second_supply = "JTOSILZ".into();
    q.hold_enabled = false;
    compare(q);
}
#[test]
#[ignore = "finite managed performance run, separate from unit acceptance"]
fn recovery_build_staged_fixture_benchmark() {
    let control = ExecutionControl::default();
    let mut q = query();
    q.fields.initial = mask(0xc0383f3fc7);
    q.fields.middle = mask(0x3ff3fc7c0c038);
    q.fields.result = mask(0x30483f07f3f8f);
    q.first_supply = "P7".into();
    q.second_supply = "P7".into();
    q.preserve_b2b = true;
    let rows = std::env::var("CLEARRA_STAGE_BENCH_ROWS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(32)
        .min(5040)
        .max(1);
    for exchange in [false, true] {
        q.allow_piece_exchange = exchange;
        let p = PreparedPopulation::new(q.clone()).unwrap();
        let now = std::time::Instant::now();
        let geometry = Geometry::new(&q, &control).unwrap();
        let catalogs = geometry
            .stages
            .iter()
            .map(|s| {
                [
                    s.middle_domain.skeleton_count(),
                    s.result_domain.skeleton_count(),
                ]
            })
            .collect::<Vec<_>>();
        let mut b = Block::new(&p, geometry, 0, rows, 3, &control).unwrap();
        while !b.advance(&control).unwrap() {}
        eprintln!("stage_benchmark exchange={exchange} first_rows={rows} represented_pairs={} elapsed_ms={} states={} middle={} tail={} repair={} tail_hits={} geometry_queries={} geometry_hits={} diagram_nodes={} catalogs={catalogs:?}",
            rows*5040,now.elapsed().as_millis(),b.solver.states,b.solver.middle_states,b.solver.tail_states,b.solver.repair_states,b.solver.suffix_hits,b.solver.geometry.lock_queries,b.solver.geometry.cache_hits,b.solver.diagram.node_count());
        let (report, _) = b.finish(&p, &control).unwrap();
        eprintln!("stage_benchmark_result exchange={exchange} counts={:?} probabilities={:?} elapsed_ms={}",report.counts,report.probabilities,now.elapsed().as_millis());
    }
}

#[test]
fn recovery_build_joint_inventory_does_not_spend_the_same_piece_twice() {
    let control = ExecutionControl::default();
    let mut q = query();
    // Two isolated horizontal I targets: each fits bounds {I:1,J:1}, but
    // both together cannot consume I:1,J:1. This is not a movement assertion.
    q.fields.result = mask(15 << 10);
    let mut g = Geometry::new(&q, &control).unwrap();
    let root = g.roots[0];
    let caps = [1, 1, 0, 0, 0, 0, 0];
    assert!(g.feasible(root, caps, caps, None, &control).unwrap());
    assert!(!g.feasible(root, caps, caps, Some(caps), &control).unwrap());
    assert!(g
        .feasible(
            root,
            [2, 0, 0, 0, 0, 0, 0],
            [2, 0, 0, 0, 0, 0, 0],
            Some([2, 0, 0, 0, 0, 0, 0]),
            &control
        )
        .unwrap());
}

#[test]
fn recovery_build_early_limit_counts_second_source_placements_before_checkpoint() {
    // Hold can expose both second-supply pieces while the first-supply I is
    // still pending. Pin exact colored placements in the independent serial
    // search so that two O locks are required before the middle I can enter.
    use PieceKind::{I, O};
    let q = RecoveryBuildFixedQuery {
        fields: RecoveryBuildFields {
            height: 6,
            initial: mask(0),
            middle: mask(15 << 40),
            result: mask(3 | (3 << 10) | (3 << 20) | (3 << 30)),
        },
        first_supply: vec![I],
        second_supply: vec![O, O],
        early_limit: CrossStageEarlyLimit::AtMost(1),
        hold_enabled: true,
        allow_piece_exchange: true,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    };
    let one = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(one.status, RecoveryBuildStatus::NoPath);
    let mut two = q.clone();
    two.early_limit = CrossStageEarlyLimit::AtMost(2);
    let result = two.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, RecoveryBuildStatus::Recovery);
    assert_eq!(result.actual_early, 2);
    let query = RecoveryBuildQuery {
        all_solutions: false,
        minimum_solutions: false,
        required_solution_keys: Vec::new(),
        minimum_source_identity: None,
        fields: two.fields.clone(),
        first_supply: "I".into(),
        second_supply: "OO".into(),
        early_limit: CrossStageEarlyLimit::AtMost(1),
        hold_enabled: true,
        allow_piece_exchange: true,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    };
    compare(query.clone());
    compare(RecoveryBuildQuery {
        early_limit: CrossStageEarlyLimit::AtMost(2),
        ..query.clone()
    });
    compare(RecoveryBuildQuery {
        early_limit: CrossStageEarlyLimit::Auto,
        ..query
    });
}

#[test]
fn recovery_build_mirrored_example_preserves_its_authorized_target() {
    let control = ExecutionControl::default();
    let mut q = query();
    q.fields.initial = mask(0x3f0);
    q.fields.middle = mask(15);
    q.fields.result = mask(7 | (1 << 10)); // J, whose mirror is L.
    q.first_supply = "I".into();
    q.second_supply = "L".into();
    q.hold_enabled = false;
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    let report = q.search(&control).unwrap();
    assert_eq!(report.normal_count, 1);
    let example = report.normal_example.unwrap();
    assert_eq!(
        example.path.result_target,
        reflected(q.fields.result, q.fields.height).words()
    );
    assert_ne!(example.path.result_target, q.fields.result.words());
}

#[test]
fn recovery_build_early_limit_counts_second_source_before_middle_completion() {
    // O from source 1 and another O from source 2 must both be placed before
    // the trailing I when hold is off. No geometric/queue oracle supplies the
    // answer: there is only one three-token placement order and both O targets
    // are on the floor beside the Middle I.
    let mut q = query();
    q.fields.middle = mask(0xf);
    q.fields.result = mask(0x3c0f0);
    q.first_supply = "O".into();
    q.second_supply = "OI".into();
    q.hold_enabled = false;
    q.early_limit = CrossStageEarlyLimit::AtMost(1);
    let denied = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(
        (
            denied.normal_count,
            denied.recovery_count,
            denied.no_path_count
        ),
        (0, 0, 1)
    );
    q.early_limit = CrossStageEarlyLimit::AtMost(2);
    let allowed = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(
        (
            allowed.normal_count,
            allowed.recovery_count,
            allowed.no_path_count
        ),
        (0, 1, 0)
    );
    assert_eq!(allowed.recovery_example.unwrap().path.actual_early, 2);
    // Hold permits storing the second O, placing I, then releasing that O.
    q.hold_enabled = true;
    q.early_limit = CrossStageEarlyLimit::AtMost(1);
    let held = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(held.recovery_count, 1);
    assert_eq!(held.recovery_example.unwrap().path.actual_early, 1);
    for early in [0, 1, 2] {
        q.early_limit = CrossStageEarlyLimit::AtMost(early);
        compare(q.clone());
    }
}

#[test]
fn recovery_build_support_quotient_matches_independent_input_enumeration() {
    use super::diagram::{Diagram, ALL};
    use std::collections::BTreeSet;
    let control = ExecutionControl::default();
    for seed in 0_u64..24 {
        let mut d = Diagram::default();
        let predicates = (0..5)
            .map(|row| {
                (0..49)
                    .map(|pair| (pair * (row + 3) + seed as usize * 7) % (row + 5) < 2)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut roots = Vec::new();
        for accepted in &predicates {
            let mut first = [NONE; 7];
            for a in 0..7 {
                first[a] = d
                    .branch(
                        1,
                        core::array::from_fn(|b| if accepted[a * 7 + b] { ALL } else { NONE }),
                    )
                    .unwrap();
            }
            roots.push(d.branch(0, first).unwrap());
        }
        let expected = (0..49)
            .map(|i| {
                predicates
                    .iter()
                    .enumerate()
                    .filter_map(|(r, p)| p[i].then_some(r))
                    .collect::<Vec<_>>()
            })
            .filter(|s| !s.is_empty())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            d.support_classes(&roots, &control).unwrap(),
            expected.into_iter().collect::<Vec<_>>()
        );
    }
}
