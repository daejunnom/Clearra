use super::super::{
    RecoveryBuildFields, RecoveryBuildParallelCoordinator, RecoveryBuildParallelWorker,
};
use super::*;
use crate::CrossStageEarlyLimit;
use clearra_core_domain::board::standard_pc_board::Board256Mask as Mask;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;
fn mask(n: u64) -> Mask {
    Mask::from_words([n, 0, 0, 0])
}
fn query() -> RecoveryBuildQuery {
    RecoveryBuildQuery {
        chain_stages: Vec::new(),
        all_solutions: true,
        minimum_solutions: false,
        required_solution_keys: Vec::new(),
        minimum_source_identity: None,
        fields: RecoveryBuildFields {
            height: 8,
            initial: mask(0),
            middle: mask(0xf),
            result: mask(0xc030),
        },
        first_supply: "[IO]".into(),
        second_supply: "[IO]".into(),
        early_limit: CrossStageEarlyLimit::Auto,
        allow_piece_exchange: true,
        hold_enabled: true,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    }
}
fn compare(q: RecoveryBuildQuery) -> RecoveryBuildPopulation {
    let control = ExecutionControl::default();
    let mut plain = q.clone();
    plain.all_solutions = false;
    let expected = plain.search(&control).unwrap();
    let actual = q.search(&control).unwrap();
    assert_eq!(
        [
            actual.normal_count,
            actual.recovery_count,
            actual.no_path_count
        ],
        [
            expected.normal_count,
            expected.recovery_count,
            expected.no_path_count
        ],
        "{q:?}"
    );
    for (a, b) in [
        (actual.normal_probability, expected.normal_probability),
        (actual.recovery_probability, expected.recovery_probability),
    ] {
        assert!((a - b).abs() < 1e-12);
    }
    assert!(actual.solutions_complete);
    assert_eq!(
        actual
            .solutions
            .iter()
            .map(|s| &s.key)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        actual.solutions.len()
    );
    for s in &actual.solutions {
        assert!(
            s.covered_count > 0 && s.covered_count <= actual.normal_count + actual.recovery_count
        );
        assert!(s.probability > 0.0 && s.probability <= 1.0);
    }
    actual
}
#[test]
fn recovery_build_catalog_complements_and_hold_match_complete_source_union() {
    for hold in [false, true] {
        for exchange in [false, true] {
            for early in [0, 1, 2] {
                let mut q = query();
                q.hold_enabled = hold;
                q.allow_piece_exchange = exchange;
                q.early_limit = CrossStageEarlyLimit::AtMost(early);
                compare(q);
            }
        }
    }
    let mut q = query();
    q.fields.middle = mask(0x3c0f);
    q.fields.result = mask(0x3c0);
    q.first_supply = "[IO][IO]".into();
    q.second_supply = "I".into();
    assert_eq!(
        compare(q).solutions.len(),
        4,
        "4x2 has two I/O tilings on each side of the symmetric initial field"
    );
}
#[test]
fn recovery_build_catalog_mirrored_target_is_actually_verified() {
    let mut q = query();
    q.fields.initial = mask(0x3f0);
    q.fields.result = mask(0x1007);
    q.first_supply = "I".into();
    q.second_supply = "J".into();
    q.hold_enabled = false;
    let actual = compare(q);
    assert_eq!(actual.normal_count, 1);
    assert!(!actual.solutions.is_empty());
    assert!(actual
        .solutions
        .iter()
        .any(|s| s.key.starts_with("recovery-tiling.v1:1|")));
}
#[test]
fn recovery_build_catalog_first_and_second_p7_are_compact_and_distinct() {
    let mut q = query();
    q.first_supply = "P7".into();
    q.second_supply = "P7".into();
    let p = PreparedPopulation::new(q).unwrap();
    let mut d = Diagram::default();
    let s = Source::compile_all(&mut d, &p.first, &p.second, &ExecutionControl::default()).unwrap();
    assert_eq!(s.first_counts, Some([1; 7]));
    assert_eq!(s.second_counts, Some([1; 7]));
    assert_eq!(d.count(s.universe, 0, 14).unwrap(), 25_401_600);
    assert!(d.node_count() < 400, "nodes={}", d.node_count());
    let packet = d.export([s.universe, s.second]).unwrap();
    let mut other = Diagram::default();
    let roots = other.import(&packet, 14).unwrap();
    assert_eq!(other.count(roots[0], 0, 14).unwrap(), 25_401_600);
    let mut bad = packet;
    bad.nodes[0].1[0] = 999_999;
    assert!(Diagram::default().import(&bad, 14).is_err());
}
#[test]
fn recovery_build_catalog_parallel_reversed_results_keep_all_solutions() {
    let mut q = query();
    q.fields.middle = mask(0x3c0f);
    q.fields.result = mask(0x3c0);
    q.first_supply = "[IO][IO]".into();
    q.second_supply = "I".into();
    let control = ExecutionControl::default();
    let expected = q.search(&control).unwrap();
    let mut c = RecoveryBuildParallelCoordinator::new(q, 11).unwrap();
    let init = c.worker_initialization();
    let mut w = RecoveryBuildParallelWorker::new(&init).unwrap();
    let mut replies = Vec::new();
    for _ in 0..10000 {
        let (kind, bytes) = c.produce(32, &control).unwrap();
        if kind == Produce::Batch {
            assert!(w.consume(&bytes, &control).unwrap().is_none());
            let result = loop {
                if let Some(r) = w.advance(&control).unwrap() {
                    break r;
                }
            };
            replies.push(result);
        } else if c.catalog.as_ref().unwrap().producer.done {
            break;
        }
    }
    assert!(replies.len() >= 2);
    for result in replies.into_iter().rev() {
        c.absorb(&result, &control).unwrap();
        assert!(c.absorb(&result, &control).is_err());
    }
    assert_eq!(c.produce(32, &control).unwrap().0, Produce::Completed);
    let actual = c.finish(&control).unwrap();
    assert_eq!(actual, expected);
}
#[test]
#[ignore = "finite full original P7/P7 catalog benchmark under the resource supervisor"]
fn recovery_build_catalog_original_fixture_benchmark() {
    let mut q = query();
    q.fields.height = 10;
    q.fields.initial = mask(0xc0383f3fc7);
    q.fields.middle = mask(0x3ff3fc7c0c038);
    q.fields.result = mask(0x30483f07f3f8f);
    q.first_supply = "P7".into();
    q.second_supply = "P7".into();
    q.preserve_b2b = true;
    q.early_limit = std::env::var("CLEARRA_EARLY_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(CrossStageEarlyLimit::Auto, CrossStageEarlyLimit::AtMost);
    let start = std::time::Instant::now();
    let result = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.evaluated, 25_401_600);
    assert!(result.solutions_complete);
    assert!(!result.solutions.is_empty());
    eprintln!(
        "catalog_fixture elapsed_ms={} normal={} recovery={} no_path={} solutions={} states={}",
        start.elapsed().as_millis(),
        result.normal_count,
        result.recovery_count,
        result.no_path_count,
        result.solutions.len(),
        result.states
    );
}
