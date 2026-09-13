//! Compact union vs the existing exact observation producer; fixture graph
//! qualification is synthetic, while placement materialization uses Core ILC.
use super::*;
use crate::pc4_lookup_graph_runtime_adapter::Pc4LookupGraphCache;
use crate::pc_candidate_page_boundary::compact_graph_union::{
    CompactGraphUnionError, CompactGraphUnionLimits, CompactGraphUnionStep, Pc4CompactGraphUnion,
};
use clearra_pc4_tablebase::{GraphTargetEncoding, LookupHit};
use clearra_pc_graph::request::PcQueueInput;
use clearra_supply::pattern_universe::CompactPatternUnionLimits;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

// Tiny expressions deliberately use explicit storage in the real compiler.
// Append an unobserved suffix before projecting to the ORIGINAL visible length
// to exercise real compact storage (5,040 weighted ordinals) with the same
// existential placement language. No test-only compact constructor or runtime
// support for explicit storage is introduced.
fn compact_queue(pattern: &str) -> PcQueueInput {
    let visible = QueuePatternExpression::parse(pattern, 32).unwrap();
    let suffix = match visible.pattern_count() {
        1 => "P7",
        2 => "P5",
        _ => panic!("bounded compact fixture expects one or two visible variants"),
    };
    // A bare group immediately followed by P is ambiguous in the product
    // grammar. Explicitly finish its one-draw suffix before the next atom.
    let delimiter = if pattern.ends_with(']') { "1" } else { "" };
    let compact = QueuePatternExpression::parse(&format!("{pattern}{delimiter}{suffix}"), 5040)
        .unwrap()
        .prefix(visible.sequence_len());
    assert!(compact.is_factorized());
    assert_eq!(compact.pattern_count(), 5040);
    assert_eq!(compact.sequence_len(), visible.sequence_len());
    PcQueueInput::pattern_expression(compact)
}

fn limits() -> CompactGraphUnionLimits {
    CompactGraphUnionLimits {
        states: nonzero(4096),
        supply_states: nonzero(65_536),
        work: nonzero(100_000),
        candidates: nonzero(512),
        waiting_fields: nonzero(128),
        edge_placements: nonzero(256),
        canonicalization_bytes: nonzero(1024 * 1024),
        language: CompactPatternUnionLimits::new(nonzero(16), nonzero(4096), nonzero(65_536)),
    }
}

fn prepare(
    fixture: &ClearPath,
    lines: u8,
    profile: Pc4RuleProfile,
    pattern: &str,
    hold: FixedQueueHoldState,
    limits: CompactGraphUnionLimits,
) -> (Pc4CompactGraphUnion, Pc4LookupGraphCache, Guard) {
    let request = product_contracts::request_with_queue_input(
        lines,
        profile,
        fixture.initial_board,
        product_contracts::Product::All,
        compact_queue(pattern),
        hold,
    );
    let crate::AppCommand::Scenario(command) = request.command() else {
        panic!("scenario fixture")
    };
    let problem = std::sync::Arc::new(
        clearra_problem::ProblemCompiler::compile_scenario_pc(command.query()).unwrap(),
    );
    prepare_problem(fixture, lines, profile, problem, hold, limits)
}

fn prepare_problem(
    fixture: &ClearPath,
    lines: u8,
    profile: Pc4RuleProfile,
    problem: std::sync::Arc<clearra_problem::SearchProblem>,
    hold: FixedQueueHoldState,
    limits: CompactGraphUnionLimits,
) -> (Pc4CompactGraphUnion, Pc4LookupGraphCache, Guard) {
    let count = ((fixture.dataset.field_index.len() - 16) / 8) as u32;
    let snapshot = activated_snapshot_for_dataset(
        "compact-graph-union",
        Some(profile),
        Pc4TargetLines::new(lines).unwrap(),
        count,
        *fixture.ids_by_step.last().unwrap(),
        &fixture.dataset,
    );
    let target = snapshot
        .qualified_target(
            profile,
            Pc4TerminalUseCase::PcSearch,
            Pc4TargetLines::new(lines).unwrap(),
        )
        .unwrap();
    let mut preparation = crate::Pc4CompiledPatternPreparation::begin(
        problem,
        crate::Pc4CompiledPatternLimits::new(nonzero(5_000_000), nonzero(16), nonzero(1)),
    )
    .unwrap();
    while !preparation.advance(nonzero(1), &|| false).unwrap() {}
    let prepared = crate::Pc4PreparedOnlineInput::for_compiled_pattern(
        target.clone(),
        Pc4InputSurface::NonInteractiveCli,
        preparation.finish().unwrap(),
    )
    .unwrap();
    let board = StandardPcBoard::from_words(lines, [fixture.initial_board, 0, 0, 0]).unwrap();
    let source = canonical_source(&prepared, board, hold);
    let guard = Guard::new(source.clone());
    let cache = Pc4LookupGraphCache::new(
        &snapshot,
        target,
        Pc4LookupGraphCacheLimits::new(nonzero(count as usize), nonzero(65_536), nonzero(65_536)),
    )
    .unwrap();
    let union = Pc4CompactGraphUnion::prepare(
        &source,
        &prepared,
        &cache,
        fixture.ids_by_step[0],
        limits,
        &guard,
    )
    .unwrap()
    .expect("actual factorized input");
    (union, cache, guard)
}

fn admit(cache: &mut Pc4LookupGraphCache, fixture: &ClearPath, id: u32) {
    let fields = &fixture.dataset.field_index[16 + id as usize * 8..];
    let mut hash = [0; 8];
    hash[..5].copy_from_slice(&fields[..5]);
    let offset = 16 + id as usize * 4;
    let start = u32::from_le_bytes(
        fixture.dataset.graph_offsets[offset..offset + 4]
            .try_into()
            .unwrap(),
    ) as usize;
    let end = u32::from_le_bytes(
        fixture.dataset.graph_offsets[offset + 4..offset + 8]
            .try_into()
            .unwrap(),
    ) as usize;
    let target = cache.target().clone();
    cache
        .admit(
            &target,
            LookupHit {
                lookup_session: LookupSessionId::new(id as u64 + 1).unwrap(),
                snapshot: target.snapshot().clone(),
                profile: target.profile(),
                field_id: id,
                field_hash: u64::from_le_bytes(hash),
                graph_target_encoding: GraphTargetEncoding::U24LittleEndian,
                graph_record: fixture.dataset.graph[start..end].to_vec(),
            },
        )
        .unwrap();
}

fn drive(
    union: &mut Pc4CompactGraphUnion,
    cache: &mut Pc4LookupGraphCache,
    fixture: &ClearPath,
    guard: &Guard,
    reverse: bool,
) {
    for _ in 0..100_000 {
        let step = union.advance(cache, nonzero(3), guard).unwrap();
        if step == CompactGraphUnionStep::Complete {
            return;
        }
        let mut pending = union.pending_fields(128);
        if reverse {
            pending.reverse();
        }
        // Deliberately admit one record at a time; no eager full-file shortcut.
        if let Some(id) = pending.first() {
            admit(cache, fixture, *id);
        }
    }
    panic!("bounded compact graph fixture stalled");
}

#[test]
fn pc4_compact_graph_union_matches_legacy_patterns_holds_profiles_and_nonbottom_clears() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let mut pairs = 0;
    for lines in 1..=4 {
        let fixture = clear_path(lines);
        for profile in Pc4RuleProfile::ALL {
            for (pattern, hold) in [
                (
                    format!("[IO]{}", "I".repeat(lines as usize - 1)),
                    FixedQueueHoldState::Disabled,
                ),
                (
                    format!("[OT]{}", "I".repeat(lines as usize)),
                    FixedQueueHoldState::Empty,
                ),
                (
                    format!("[IO]{}", "I".repeat(lines as usize - 1)),
                    FixedQueueHoldState::Occupied(Pc4GraphPiece::I),
                ),
            ] {
                let (mut old, old_guard) =
                    observation_contracts::start_pattern(&fixture, lines, profile, &pattern, hold);
                let (status, _) = observation_contracts::drive(&mut old, &old_guard, &fixture);
                assert!(
                    matches!(status, AppOnlinePc4FixedQueueCandidateStep::Complete { .. }),
                    "{status:?}"
                );
                let expected = old.into_completed_reducer_input(&old_guard).unwrap();
                let (mut union, mut cache, guard) =
                    prepare(&fixture, lines, profile, &pattern, hold, limits());
                drive(&mut union, &mut cache, &fixture, &guard, pairs % 2 == 0);
                let actual = union.into_reducer_input(&guard).unwrap();
                assert_eq!(
                    actual.candidates(),
                    expected.candidates(),
                    "{lines}L {profile:?} {pattern} {hold:?}"
                );
                assert_eq!(actual.candidates().len(), 1);
                pairs += 1;
            }
        }
    }
    assert_eq!(pairs, 60);
    println!("pc4_compact_graph_union_exact_pairs={pairs}");
}

#[test]
fn pc4_compact_graph_union_enters_existing_products_without_reveal_or_probability_changes() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let fixture = clear_path(4);
    let (mut union, mut cache, guard) = prepare(
        &fixture,
        4,
        Pc4RuleProfile::Jstris180,
        "[IO]III",
        FixedQueueHoldState::Disabled,
        limits(),
    );
    drive(&mut union, &mut cache, &fixture, &guard, false);
    let input = union.into_reducer_input(&guard).unwrap();
    product_contracts::assert_pattern_product_parity_with_queue(
        4,
        Pc4RuleProfile::Jstris180,
        fixture.initial_board,
        "[IO]III",
        compact_queue("[IO]III"),
        FixedQueueHoldState::Disabled,
        2520,
        input,
        &guard,
    );
}

#[test]
fn pc4_compact_graph_union_zero_hit_and_incomplete_never_become_partial_completion() {
    let fixture = clear_path(2);
    let (union, _, guard) = prepare(
        &fixture,
        2,
        Pc4RuleProfile::Srs,
        "OO",
        FixedQueueHoldState::Disabled,
        limits(),
    );
    assert_eq!(
        union.into_reducer_input(&guard).unwrap_err().reason(),
        "pc4_compact_union_incomplete"
    );
    let (mut union, mut cache, guard) = prepare(
        &fixture,
        2,
        Pc4RuleProfile::Srs,
        "OO",
        FixedQueueHoldState::Disabled,
        limits(),
    );
    drive(&mut union, &mut cache, &fixture, &guard, false);
    assert!(union
        .into_reducer_input(&guard)
        .unwrap()
        .candidates()
        .is_empty());
    let mut bounded = limits();
    bounded.work = nonzero(1);
    let (mut union, mut cache, guard) = prepare(
        &fixture,
        2,
        Pc4RuleProfile::Srs,
        "II",
        FixedQueueHoldState::Disabled,
        bounded,
    );
    admit(&mut cache, &fixture, fixture.ids_by_step[0]);
    let error = union.advance(&cache, nonzero(8), &guard).unwrap_err();
    assert!(matches!(
        error,
        CompactGraphUnionError::Limit {
            kind: "pc4_compact_union_work_limit",
            limit: 1,
            attempted: 2
        }
    ));
    assert!(union.pending_fields(10).is_empty());
    assert_eq!(
        union.into_reducer_input(&guard).unwrap_err().reason(),
        "pc4_compact_union_incomplete"
    );
}

#[test]
fn pc4_compact_graph_union_cancellation_and_stale_sources_fail_closed() {
    let fixture = clear_path(4);
    for stale in 0..3 {
        let (mut union, cache, guard) = prepare(
            &fixture,
            4,
            Pc4RuleProfile::Jstris180,
            "IIII",
            FixedQueueHoldState::Disabled,
            limits(),
        );
        match stale {
            0 => guard.cancelled.set(true),
            1 => guard.source_current.set(false),
            _ => guard.snapshot_current.set(false),
        }
        assert!(union.advance(&cache, nonzero(1), &guard).is_err());
        assert!(union.pending_fields(128).is_empty());
        assert!(union.into_reducer_input(&guard).is_err());
    }
}

#[test]
fn pc4_compact_graph_union_finalization_budget_and_late_revocation_cannot_publish() {
    let fixture = clear_path(2);
    let make = |limits| {
        prepare(
            &fixture,
            2,
            Pc4RuleProfile::Srs,
            "II",
            FixedQueueHoldState::Disabled,
            limits,
        )
    };
    let (mut reference, mut cache, guard) = make(limits());
    drive(&mut reference, &mut cache, &fixture, &guard, false);
    let usage = reference.usage();
    assert!(usage.canonicalization_work > 1);
    assert!(usage.peak_canonicalization_buffer_bytes > 0);
    let full_work = usage.work + usage.canonicalization_work;
    for memory_limit in [false, true] {
        let mut budget = limits();
        if memory_limit {
            budget.canonicalization_bytes = nonzero(1);
        } else {
            budget.work = nonzero(full_work - 1);
        }
        let (mut owner, mut cache, guard) = make(budget);
        let mut failed = None;
        for _ in 0..1000 {
            match owner.advance(&cache, nonzero(1), &guard) {
                Err(error) => {
                    failed = Some(error);
                    break;
                }
                Ok(CompactGraphUnionStep::Complete) => {
                    panic!("an exhausted envelope cannot complete")
                }
                _ => {}
            }
            if let Some(id) = owner.pending_fields(1).first() {
                admit(&mut cache, &fixture, *id);
            }
        }
        assert_eq!(
            failed.expect("bounded finalization must reject").reason(),
            if memory_limit {
                "pc_candidate_canonicalization_buffer_limit"
            } else {
                "pc4_compact_union_work_limit"
            }
        );
        assert!(owner.into_reducer_input(&guard).is_err());
    }
    for revoke in 0..3 {
        let (mut owner, mut cache, guard) = make(limits());
        for _ in 0..1000 {
            assert_ne!(
                owner.advance(&cache, nonzero(1), &guard).unwrap(),
                CompactGraphUnionStep::Complete
            );
            if owner.usage().canonicalization_work > 0 {
                break;
            }
            if let Some(id) = owner.pending_fields(1).first() {
                admit(&mut cache, &fixture, *id);
            }
        }
        assert!(
            owner.usage().canonicalization_work > 0,
            "test reached finalization"
        );
        match revoke {
            0 => guard.cancelled.set(true),
            1 => guard.source_current.set(false),
            _ => guard.snapshot_current.set(false),
        }
        assert!(owner.advance(&cache, nonzero(16), &guard).is_err());
        assert!(owner.into_reducer_input(&guard).is_err());
    }
}

// Three disjoint 2x2 holes. Every order of the three O placements is legal;
// the last O clears both rows. Six histories must give ONE canonical layout.
fn three_o_fixture() -> ClearPath {
    let left = 3 | (3 << 10);
    let holes = [left, left << 4, left << 8];
    let initial_board = full_rows(2) ^ (holes[0] | holes[1] | holes[2]);
    let mut fields: Vec<_> = (0u8..8)
        .map(|bits| {
            let cells = (0..3)
                .filter(|i| bits & (1 << i) != 0)
                .fold(initial_board, |cells, i| cells | holes[i]);
            (
                clearra_board64_mask_to_hydra_field_hash_v1(cells).unwrap(),
                bits,
            )
        })
        .collect();
    fields.sort_unstable();
    let mut ids = [0u32; 8];
    for (id, (_, subset)) in fields.iter().enumerate() {
        ids[*subset as usize] = id as u32;
    }
    let mut dataset = RangeDataset {
        field_index: index_header(*b"FHIDIDX1", 8),
        graph_offsets: index_header(*b"GOFFIDX1", 8),
        graph: Vec::new(),
    };
    for (id, (hash, subset)) in fields.into_iter().enumerate() {
        dataset
            .field_index
            .extend_from_slice(&hash.to_le_bytes()[..5]);
        dataset
            .field_index
            .extend_from_slice(&(id as u32).to_le_bytes()[..3]);
        dataset
            .graph_offsets
            .extend_from_slice(&(dataset.graph.len() as u32).to_le_bytes());
        let targets: Vec<_> = (0..3)
            .filter(|i| subset & (1 << i) == 0)
            .map(|i| ids[(subset | (1 << i)) as usize])
            .collect();
        dataset
            .graph
            .extend(hydra_record(hash, [&[], &[], &[], &targets, &[], &[], &[]]));
    }
    dataset
        .graph_offsets
        .extend_from_slice(&(dataset.graph.len() as u32).to_le_bytes());
    ClearPath {
        dataset,
        initial_board,
        ids_by_step: vec![ids[0], ids[1], ids[3], ids[7]],
        original_placements: holes.to_vec(),
    }
}

#[test]
fn pc4_compact_graph_union_merges_diamonds_and_collects_independent_pending_fields() {
    use clearra_pc_graph::request::{
        PcExecutionPolicy, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    };
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let fixture = three_o_fixture();
    let problem = std::sync::Arc::new(
        clearra_problem::ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(2, fixture.initial_board),
                compact_queue("OOO"),
                PieceWindow::new(3),
            )
            .with_exact_pieces(Some(3))
            .with_allow_hold(false)
            .with_rule(clearra_rules::profile::builtin_rules::srs())
            .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1)),
        )
        .unwrap(),
    );
    let mut reference = None;
    for reverse in [false, true] {
        let (mut union, mut cache, guard) = prepare_problem(
            &fixture,
            2,
            Pc4RuleProfile::Srs,
            problem.clone(),
            FixedQueueHoldState::Disabled,
            limits(),
        );
        let mut maximum_pending = 0;
        for _ in 0..10_000 {
            let status = union.advance(&cache, nonzero(8), &guard).unwrap();
            if status == CompactGraphUnionStep::Complete {
                break;
            }
            let mut pending = union.pending_fields(128);
            maximum_pending = maximum_pending.max(pending.len());
            // Allow independent CPU branches to register their demands first.
            if status == CompactGraphUnionStep::Waiting {
                if reverse {
                    pending.reverse();
                }
                admit(
                    &mut cache,
                    &fixture,
                    *pending.first().expect("not complete without data"),
                );
            }
        }
        assert!(
            maximum_pending >= 2,
            "one blocked branch must not freeze other branches"
        );
        let usage = union.usage();
        assert!(
            usage.merged_states >= 3,
            "same partial layout/frame must merge before expansion"
        );
        let actual = union.into_reducer_input(&guard).unwrap();
        let expected = clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity::from_placements(
            fixture.initial_board, fixture.original_placements.iter().map(|mask|
                clearra_core_domain::solution::normalized_tiling_solution::PiecePlacementMask::new(
                    clearra_core_domain::piece::piece_kind::PieceKind::O, *mask))).unwrap();
        assert_eq!(actual.candidates(), &[expected]);
        if let Some(reference) = &reference {
            assert_eq!(actual.candidates(), reference);
        }
        reference = Some(actual.candidates().to_vec());
        println!(
            "pc4_compact_graph_union_diamond reverse={reverse} work={} merged={} peak_pending={}",
            usage.work, usage.merged_states, usage.peak_waiting_fields
        );
    }
}
