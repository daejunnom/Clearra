use super::*;
use crate::codec::{FumenLikeTrace, FumenLikeWriter};
use std::{collections::BTreeSet, fs, hash::Hasher, path::PathBuf};

#[test]
fn external_pc_fumen_decode_preserves_initial_board() {
    let setup =
        decode_source_setup("tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_setup.fumen");

    assert_eq!(setup.initial_board_mask(), 0x0003_00c0_399e_3fdf);
    assert_eq!(setup.visible_height(), 5);
}

#[test]
fn pco_i_hold_setup_fumen_decodes_to_scenario_board() {
    let setup = decode_source_setup(
        "tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_setup.fumen",
    );

    assert_eq!(setup.initial_board_mask(), 0x0000_00e0_f87e_3f87);
    assert_eq!(setup.visible_height(), 4);
}

#[test]
fn external_pc_fumen_decode_preserves_solution_pages() {
    let document = normalize_fixture(
        "tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_expected_any.fumen",
    );

    assert_eq!(document.solution_key_count(), 1);
    assert!(document.pages()[0].is_solution_page());
    assert_eq!(document.pages()[0].final_board_mask(), 0);
}

#[test]
fn external_pc_fumen_normalization_ignores_comments() {
    let left = normalize_trace_pages(vec![
        "kind=normalized-solution\nsolution_index=1\ninitial_board_mask=0x1\nfinal_board_mask=0x0\npiece_sequence=IOTS\noperation_sequence=I:0:0:0|O:0:4:0\nnormalized_shape_key=shape-a\nnormalized_tiling_key=tiling-a".to_owned(),
    ]);
    let right = normalize_trace_pages(vec![
        "this comment line is ignored\nkind=normalized-solution\nsolution_index=1\ninitial_board_mask=0x1\nfinal_board_mask=0x0\npiece_sequence=IOTS\noperation_sequence=I:0:0:0|O:0:4:0\nnormalized_shape_key=shape-a\nnormalized_tiling_key=tiling-a\nanother ignored line".to_owned(),
    ]);

    assert_eq!(left.solution_keys(), right.solution_keys());
}

#[test]
fn normalized_solution_key_ignores_comments() {
    let base = only_solution_key(normalize_trace_pages(vec![solution_page(
        "this ignored header has no equals",
        "",
        "none",
    )]));
    let shifted = only_solution_key(normalize_trace_pages(vec![
        "kind=comment-only\ncomment=page index must not enter solution identity".to_owned(),
        solution_page(
            "free text ignored before fields",
            "another ignored comment line after fields",
            "none",
        ),
    ]));

    assert_eq!(base, shifted);
    assert_eq!(base.piece_sequence(), &["T", "I"]);
    assert_eq!(base.operation_sequence(), &["T:0:4:0", "I:1:2:1"]);
    assert_eq!(base.mirror_policy(), "none");
}

#[test]
fn normalized_solution_key_preserves_hold_decisions() {
    let key = only_solution_key(normalize_trace_pages(vec![format!(
        "{}\nhold_decision_sequence=hold:I|swap:T|none",
        solution_page("", "", "none")
    )]));

    assert_eq!(key.hold_decision_sequence(), &["hold:I", "swap:T", "none"]);
    assert_eq!(key.piece_sequence(), &["T", "I"]);
    assert_eq!(key.mirror_policy(), "none");
}

#[test]
fn normalized_solution_key_preserves_line_clear_events() {
    let key = only_solution_key(normalize_trace_pages(vec![format!(
        "{}\nline_clear_events=clear:0|clear:1",
        solution_page("", "", "right-mirror-disabled")
    )]));

    assert_eq!(key.cleared_line_sequence(), &["clear:0", "clear:1"]);
    assert_eq!(key.final_board_mask(), 0);
    assert_eq!(key.mirror_policy(), "right-mirror-disabled");
}

#[test]
fn external_pc_fumen_normalization_preserves_piece_sequence() {
    let document = normalize_fixture(
        "tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_expected_any.fumen",
    );
    let key: &crate::normalize::NormalizedSolutionKey = document
        .solution_keys()
        .iter()
        .next()
        .expect("representative solution key");

    assert_eq!(key.piece_sequence(), &["O", "I", "J", "T"]);
    assert_eq!(
        key.operation_sequence(),
        &["O:0:0:0", "I:1:2:0", "J:0:6:0", "T:2:4:1"]
    );
}

#[test]
fn normalized_solution_key_preserves_piece_sequence() {
    external_pc_fumen_normalization_preserves_piece_sequence();
}

#[test]
fn external_pc_fumen_solution_key_is_stable() {
    let path = "tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_full_42.fumen";
    let first = normalize_fixture(path);
    let second = normalize_fixture(path);

    assert_eq!(first.solution_keys(), second.solution_keys());
}

#[test]
fn tsar_cannon_full_42_fumen_has_42_unique_solution_keys() {
    let document =
        normalize_fixture("tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_full_42.fumen");

    assert_eq!(document.pages().len(), 44);
    assert_eq!(document.solution_key_count(), 42);
    assert!(document
        .solution_keys()
        .iter()
        .all(|key| key.initial_board_mask() == 0x0000_0000_00f3_c3f0));
    assert!(document
        .solution_keys()
        .iter()
        .all(|key| key.final_board_mask() == 0));
}

#[test]
fn tsar_cannon_full_42_fumen_decodes_to_42_unique_solution_keys() {
    tsar_cannon_full_42_fumen_has_42_unique_solution_keys();
}

#[test]
fn normalized_solution_key_hash_is_shared_between_rust_and_worker_e2e() {
    let document =
        normalize_fixture("tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_full_42.fumen");
    let actual_hash = normalized_solution_set_hash(&document);
    let report = read_fixture_text(
        "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.normalize.json",
    );

    assert_eq!(actual_hash, "wes1:548277ae9ac32701");
    assert!(report.contains("\"solution_set_hash\": \"wes1:548277ae9ac32701\""));
    assert!(report.contains(
        "\"solution_set_hash_algorithm\": \"worker-e2e-normalized-solution-key-fnv64-v1\""
    ));
}

fn normalize_fixture(path: &str) -> crate::normalize::NormalizedFumenDocument {
    let text = read_fixture_text(path);
    FumenNormalizer::normalize(&text)
        .unwrap_or_else(|error| panic!("failed to normalize {path}: {error:?}"))
}

fn read_fixture_text(path: &str) -> String {
    let full_path = workspace_root().join(path);
    fs::read_to_string(&full_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", full_path.display()))
}

fn decode_source_setup(path: &str) -> crate::codec::SourceFumenSetup {
    crate::codec::SourceFumenSetup::decode(&read_fixture_text(path))
        .unwrap_or_else(|error| panic!("failed to decode {path}: {error:?}"))
}

fn normalize_trace_pages(pages: Vec<String>) -> crate::normalize::NormalizedFumenDocument {
    let encoded = FumenLikeWriter::write(&FumenLikeTrace::new(pages)).expect("synthetic fumen");
    FumenNormalizer::normalize(&encoded).expect("normalized synthetic trace")
}

fn only_solution_key(
    document: crate::normalize::NormalizedFumenDocument,
) -> crate::normalize::NormalizedSolutionKey {
    let mut keys = document.solution_keys().iter();
    let key = keys.next().expect("solution key").clone();
    assert!(keys.next().is_none(), "expected exactly one solution key");
    key
}

fn solution_page(prefix_comment: &str, suffix_comment: &str, mirror_policy: &str) -> String {
    let mirror_policy_field = format!("mirror_policy={mirror_policy}");
    [
        prefix_comment,
        "kind=normalized-solution",
        "initial_board_mask=0x000000000000000f",
        "final_board_mask=0x0",
        "piece_sequence=TI",
        "operation_sequence=T:0:4:0|I:1:2:1",
        mirror_policy_field.as_str(),
        suffix_comment,
    ]
    .into_iter()
    .filter(|line| !line.is_empty())
    .collect::<Vec<_>>()
    .join("\n")
}

fn normalized_solution_set_hash(document: &crate::normalize::NormalizedFumenDocument) -> String {
    let keys = document
        .solution_keys()
        .iter()
        .map(solution_key_signature)
        .collect::<BTreeSet<_>>();
    stable_hash(keys)
}

fn solution_key_signature(key: &crate::normalize::NormalizedSolutionKey) -> String {
    [
        format!("0x{:016x}", key.initial_board_mask()),
        format!("0x{:016x}", key.final_board_mask()),
        key.piece_sequence().join(""),
        key.hold_decision_sequence().join("|"),
        key.operation_sequence().join("|"),
        key.cleared_line_sequence().join("|"),
        key.normalized_shape_key().to_owned(),
        key.normalized_tiling_key().to_owned(),
    ]
    .join("|")
}

fn stable_hash<I>(parts: I) -> String
where
    I: IntoIterator<Item = String>,
{
    let mut hasher = StableFnv64::default();
    for part in parts {
        hasher.write(part.as_bytes());
    }
    format!("wes1:{:016x}", hasher.finish())
}

#[derive(Default)]
struct StableFnv64(u64);

impl Hasher for StableFnv64 {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        if self.0 == 0 {
            self.0 = 0xcbf2_9ce4_8422_2325;
        }
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}
