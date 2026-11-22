//! Finite, explicit cost probe for the board-independent reverse CSR cut.
//! Timers bracket whole batches, never an edge, pose or template hot loop.
//! This measures template creation and a fixed query workload separately;
//! it does not predict end-to-end see-7 speed or template amortization there.

use std::{collections::BTreeMap, hint::black_box, time::Instant};

use sha2::{Digest, Sha256};

use super::{
    compile_reverse_rotation_sources, state_from_index,
    symbolic_target::{compile_prefix_target_lanes_from_planes, ReverseScratch},
    with_global_kick_prune_mode, ExecutionControl, GlobalKickPruneMode, InstantiatedRealization,
    KickTableProfileId, PieceKind, PrefixPoseWork, ReachabilityTemplate,
};
use crate::reachability_reference;

const PROFILES: [KickTableProfileId; 5] = [
    KickTableProfileId::Srs90,
    KickTableProfileId::SrsPlus,
    KickTableProfileId::SrsX,
    KickTableProfileId::Jstris180,
    KickTableProfileId::NoKick,
];

#[test]
fn improved_global_kick_pruning_reuses_the_exact_csr_decisions() {
    let mut pruned = 0;
    for height in 1..=6 {
        for profile in PROFILES {
            for piece in PieceKind::STANDARD_TETROMINOES {
                let raw = with_global_kick_prune_mode(GlobalKickPruneMode::Off, || {
                    ReachabilityTemplate::compile(10, height, piece, profile)
                });
                let compile = |mode| {
                    compile_reverse_rotation_sources(
                        &raw.state_masks,
                        &raw.rotation_target_offsets,
                        &raw.rotation_targets,
                        mode,
                    )
                };
                let (current_offsets, current_sources, current_work) =
                    compile(GlobalKickPruneMode::Current);
                let (reused_offsets, reused_sources, reused_work) =
                    compile(GlobalKickPruneMode::Improved);
                assert_eq!(current_offsets, reused_offsets);
                assert_eq!(current_sources, reused_sources);
                assert_eq!(
                    raw.reverse_rotation_sources.len(),
                    raw.rotation_targets.len()
                );
                assert_eq!(raw.global_kick_prune_compile_work.predicate_calls, 0);
                assert_eq!(
                    current_work.predicate_calls,
                    2 * raw.rotation_targets.len() as u64
                );
                assert_eq!(
                    reused_work.predicate_calls,
                    raw.rotation_targets.len() as u64
                );
                assert_eq!(
                    current_work.prior_mask_checks,
                    2 * reused_work.prior_mask_checks
                );
                assert_eq!(current_work.keep_bitmap_peak_bytes, 0);
                assert_eq!(
                    reused_work.keep_bitmap_peak_bytes,
                    raw.rotation_targets.len().div_ceil(64) * core::mem::size_of::<u64>()
                );
                pruned += raw.reverse_rotation_sources.len() - current_sources.len();
            }
        }
    }
    assert!(pruned > 0);
}

struct Fixture {
    height: u8,
    piece: PieceKind,
    profile: KickTableProfileId,
    raw: ReachabilityTemplate,
    occupied: [u64; 64],
    lanes: u64,
    targets: Vec<Vec<InstantiatedRealization>>,
    expected: Vec<u64>,
}

fn fixtures() -> Vec<Fixture> {
    let mut fixtures = Vec::new();
    for height in [2_u8, 4, 6] {
        let field_mask = (1_u64 << (10 * height)) - 1;
        let boards = [
            (0, 0),
            (1, 0x4002 & field_mask),
            (7, 0x4802 & field_mask),
            (19, (0x3ff_u64 & !(3 << 4)) << (10 * (height - 2))),
            (47, 0x3f_5022_07e7 & field_mask),
            (
                63,
                (0x1ff_u64 << (10 * (height - 1))) | (0x155_u64 << (10 * height.saturating_sub(3))),
            ),
        ];
        let mut occupied = [0_u64; 64];
        let mut lanes = 0;
        for (lane, mut board) in boards {
            let bit = 1_u64 << lane;
            lanes |= bit;
            while board != 0 {
                occupied[board.trailing_zeros() as usize] |= bit;
                board &= board - 1;
            }
        }
        for profile in PROFILES {
            for piece in PieceKind::STANDARD_TETROMINOES {
                let raw = with_global_kick_prune_mode(GlobalKickPruneMode::Off, || {
                    ReachabilityTemplate::compile(10, height, piece, profile)
                });
                let mut groups = BTreeMap::<u64, Vec<InstantiatedRealization>>::new();
                for (index, &mask) in raw.state_masks.iter().enumerate() {
                    if mask.count_ones() != 4 {
                        continue;
                    }
                    let state = state_from_index(10, raw.ceiling, index);
                    groups
                        .entry(mask)
                        .or_default()
                        .push(InstantiatedRealization {
                            lock_mask: mask,
                            rotation: state.rotation,
                            x: state.x,
                            lock_y: i8::try_from(state.y).unwrap(),
                        });
                }
                let step = groups.len().div_ceil(12).max(1);
                let targets: Vec<_> = groups.into_values().step_by(step).collect();
                let references = boards.map(|(lane, board)| {
                    (
                        lane,
                        reachability_reference::reference_spawn_lock_anchors(
                            10, height, board, piece, profile,
                        )
                        .unwrap(),
                    )
                });
                let expected = targets
                    .iter()
                    .map(|locks| {
                        references.iter().fold(0, |covered, &(lane, anchors)| {
                            let reachable = locks.iter().any(|lock| {
                                let bit = lock.lock_y as usize * 10 + lock.x as usize;
                                anchors[lock.rotation.quarter_turns() as usize] & (1_u64 << bit)
                                    != 0
                            });
                            covered | (u64::from(reachable) << lane)
                        })
                    })
                    .collect();
                fixtures.push(Fixture {
                    height,
                    piece,
                    profile,
                    raw,
                    occupied,
                    lanes,
                    targets,
                    expected,
                });
            }
        }
    }
    fixtures
}

fn sample(mode: GlobalKickPruneMode, fixtures: &[Fixture]) -> serde_json::Value {
    // Each batch owns each template once. Destruction and the independent
    // reference are outside the creation/query timers.
    let start = Instant::now();
    let templates: Vec<_> = with_global_kick_prune_mode(mode, || {
        fixtures
            .iter()
            .map(|fixture| {
                ReachabilityTemplate::compile(10, fixture.height, fixture.piece, fixture.profile)
            })
            .collect()
    });
    let template_generation_ns = start.elapsed().as_nanos();

    // Isolate the changed compilation stage using already-built immutable
    // forward input. Do not add this to full template creation a second time.
    let start = Instant::now();
    let reverse_indexes: Vec<_> = fixtures
        .iter()
        .map(|fixture| {
            compile_reverse_rotation_sources(
                black_box(&fixture.raw.state_masks),
                black_box(&fixture.raw.rotation_target_offsets),
                black_box(&fixture.raw.rotation_targets),
                black_box(mode),
            )
        })
        .collect();
    let reverse_csr_generation_ns = start.elapsed().as_nanos();
    for (template, (offsets, sources, _)) in templates.iter().zip(&reverse_indexes) {
        assert_eq!(&template.reverse_rotation_offsets, offsets);
        assert_eq!(&template.reverse_rotation_sources, sources);
    }

    let control = ExecutionControl::default();
    let mut work = PrefixPoseWork::default();
    let mut scratch = ReverseScratch::default();
    let query_count: usize = fixtures.iter().map(|fixture| fixture.targets.len()).sum();
    let mut answers = Vec::with_capacity(query_count);
    // One worker scratch is reused across all queries. Its first allocation
    // is included; result-vector capacity and oracle construction are not.
    let start = Instant::now();
    for (fixture, template) in fixtures.iter().zip(&templates) {
        for locks in &fixture.targets {
            answers.push(
                compile_prefix_target_lanes_from_planes(
                    black_box(template),
                    &fixture.occupied,
                    fixture.lanes,
                    locks,
                    &control,
                    &mut work,
                    &mut scratch,
                )
                .unwrap()
                .expect("finite admitted target query"),
            );
        }
    }
    let query_ns = start.elapsed().as_nanos();
    let expected: Vec<_> = fixtures
        .iter()
        .flat_map(|fixture| &fixture.expected)
        .copied()
        .collect();
    assert_eq!(answers, expected, "mode={mode:?}");
    let raw_edges: usize = templates.iter().map(|t| t.rotation_targets.len()).sum();
    let retained_edges: usize = templates
        .iter()
        .map(|t| t.reverse_rotation_sources.len())
        .sum();
    let predicate_calls: u64 = templates
        .iter()
        .map(|t| t.global_kick_prune_compile_work.predicate_calls)
        .sum();
    let prior_mask_checks: u64 = templates
        .iter()
        .map(|t| t.global_kick_prune_compile_work.prior_mask_checks)
        .sum();
    let keep_bitmap_peak_bytes = templates
        .iter()
        .map(|t| t.global_kick_prune_compile_work.keep_bitmap_peak_bytes)
        .max()
        .unwrap();
    serde_json::json!({
        "mode": match mode {
            GlobalKickPruneMode::Off => "0",
            GlobalKickPruneMode::Current => "1",
            GlobalKickPruneMode::Improved => "2",
        },
        "fixture_count": fixtures.len(),
        "query_count": query_count,
        "template_generation_ns": template_generation_ns,
        "reverse_csr_generation_ns": reverse_csr_generation_ns,
        "query_ns": query_ns,
        "template_plus_queries_ns": template_generation_ns + query_ns,
        "raw_edges": raw_edges,
        "retained_edges": retained_edges,
        "compile_predicate_calls": predicate_calls,
        "compile_prior_mask_checks": prior_mask_checks,
        "keep_bitmap_peak_bytes": keep_bitmap_peak_bytes,
        "query_reverse_candidate_checks": work.reverse_rotation_candidate_checks,
        "query_prior_guard_checks": work.reverse_rotation_prior_checks,
        "query_ordered_kick_terms": work.ordered_kick_terms,
        "query_pose_domains": work.pose_domains,
        "query_pose_work": work.reached_pose_lanes,
        "query_worklist_pops": work.worklist_pops,
        "peak_query_logical_bytes": work.peak_logical_bytes,
        "independent_reference_equal": true,
    })
}

fn sample_metric_summary(samples: &[&serde_json::Value], metric: &str) -> serde_json::Value {
    let values: Vec<u64> = samples
        .iter()
        .map(|sample| {
            sample[metric]
                .as_u64()
                .expect("finite integer sample metric")
        })
        .collect();
    let sum: u128 = values.iter().copied().map(u128::from).sum();
    serde_json::json!({
        "sample_count": values.len(),
        "mean": sum as f64 / values.len() as f64,
        "minimum": values.iter().min().expect("nonempty mode sample group"),
        "maximum": values.iter().max().expect("nonempty mode sample group"),
    })
}

/// Summarize only after every timed arm has finished. Binary hashing,
/// grouping and statistics must never enter a measured batch.
fn sample_summary(samples: &[serde_json::Value]) -> serde_json::Value {
    let mut modes = BTreeMap::new();
    for (mode, label) in [("0", "off"), ("1", "current"), ("2", "improved")] {
        let group: Vec<_> = samples
            .iter()
            .filter(|sample| sample["mode"] == mode)
            .collect();
        let first = group.first().expect("all three modes were measured");
        assert!(group.iter().all(|sample| {
            sample["fixture_count"] == first["fixture_count"]
                && sample["query_count"] == first["query_count"]
                && sample["independent_reference_equal"] == true
        }));
        let metrics = |names: &[&str]| {
            names
                .iter()
                .map(|&name| (name.to_owned(), sample_metric_summary(&group, name)))
                .collect::<BTreeMap<_, _>>()
        };
        modes.insert(
            mode,
            serde_json::json!({
                "label": label,
                "sample_count": group.len(),
                "fixture_count": first["fixture_count"],
                "query_count": first["query_count"],
                "timing_ns": metrics(&[
                    "template_generation_ns", "reverse_csr_generation_ns",
                    "query_ns", "template_plus_queries_ns",
                ]),
                "compile_work": metrics(&["compile_predicate_calls", "compile_prior_mask_checks"]),
                "query_work": metrics(&[
                    "query_reverse_candidate_checks", "query_prior_guard_checks",
                    "query_ordered_kick_terms", "query_pose_domains", "query_pose_work",
                    "query_worklist_pops",
                ]),
                "template_edges": metrics(&["raw_edges", "retained_edges"]),
                "memory_bytes": metrics(&["keep_bitmap_peak_bytes", "peak_query_logical_bytes"]),
                "independent_reference_equal": true,
            }),
        );
    }
    let binary = std::env::current_exe().expect("current benchmark test binary");
    let binary_sha = format!(
        "{:x}",
        Sha256::digest(std::fs::read(binary).expect("read benchmark binary after timing"))
    );
    serde_json::json!({
        "schema": "clearra.global-kick-pruning-cost-summary.v1",
        "benchmark": "global_kick_pruning_generation_and_query_cost",
        "binary_sha256": binary_sha,
        "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
        "measured_sample_count": samples.len(),
        "warmup_samples_excluded": 3,
        "abba_rounds_per_pair": 2,
        "pairs": ["off_current", "current_improved"],
        "modes": modes,
        "timing_scope": "Whole batches. template_plus_queries_ns sums template creation and queries; reverse CSR is a separate diagnostic already included in template creation and must not be added again. Warmup, binary hashing and summary statistics are excluded.",
        "counter_scope": "Per batch: compile counts and raw/retained edges sum each owned template once; query counts sum actual work. keep_bitmap_peak_bytes is the largest transient bitmap for one template, and peak_query_logical_bytes is the largest sampled logical query capacity, not RSS.",
    })
}

#[test]
#[ignore = "finite native A/B cost probe; run explicitly in release with --ignored --nocapture"]
fn global_kick_pruning_generation_and_query_cost() {
    let fixtures = fixtures();
    for mode in [
        GlobalKickPruneMode::Off,
        GlobalKickPruneMode::Current,
        GlobalKickPruneMode::Improved,
    ] {
        black_box(sample(mode, &fixtures));
    }
    let mut measured_samples = Vec::with_capacity(16);
    // Two finite ABBA rounds per single changed axis. All three modes use
    // the same 105 templates and the same independently checked target set.
    for round in 0..2 {
        for (pair, baseline, treatment) in [
            (
                "off_current",
                GlobalKickPruneMode::Off,
                GlobalKickPruneMode::Current,
            ),
            (
                "current_improved",
                GlobalKickPruneMode::Current,
                GlobalKickPruneMode::Improved,
            ),
        ] {
            for (arm, mode) in [baseline, treatment, treatment, baseline]
                .into_iter()
                .enumerate()
            {
                let mut result = sample(mode, &fixtures);
                result["benchmark"] = "global_kick_pruning_generation_and_query_cost".into();
                result["pair"] = pair.into();
                result["round"] = round.into();
                result["arm"] = arm.into();
                eprintln!("{result}");
                measured_samples.push(result);
            }
        }
    }
    eprintln!("{}", sample_summary(&measured_samples));
}
