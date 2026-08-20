use super::*;
use crate::codec::fumen_like_writer::FumenLikeWriter;
use std::{fs, path::PathBuf};

#[test]
fn rejects_obsolete_separator_payload_without_fumen_version() {
    assert_eq!(
        FumenLikeReader::read("kind=pc\n---\nlines=2"),
        Err(FumenLikeReadError::UnsupportedVersion)
    );
}

#[test]
fn rejects_invalid_base64_characters() {
    assert_eq!(
        FumenLikeReader::read("v115@!!!!"),
        Err(FumenLikeReadError::InvalidCharacter {
            index: 0,
            value: '!'
        })
    );
}

#[test]
fn rejects_truncated_v115_data() {
    assert_eq!(
        FumenLikeReader::read("v115@vhA"),
        Err(FumenLikeReadError::UnexpectedEnd)
    );
}

#[test]
fn unescapes_javascript_utf16_pairs_and_rejects_unpaired_surrogates() {
    assert_eq!(
        unescape_comment("%uD55C%uAE00%20%uD83D%uDE00"),
        Ok("한글 😀".to_owned())
    );
    assert_eq!(
        unescape_comment("100%25%20ready"),
        Ok("100% ready".to_owned())
    );
    assert_eq!(
        unescape_comment("%uD83D"),
        Err(FumenLikeReadError::InvalidEscape)
    );
    assert_eq!(
        unescape_comment("%uDE00"),
        Err(FumenLikeReadError::InvalidEscape)
    );
}

#[test]
fn rejects_oversized_input_before_base64_materialization() {
    let input = format!("v115@{}", "A".repeat(FUMEN_MAX_INPUT_BYTES));

    assert_eq!(
        FumenLikeReader::read(&input),
        Err(FumenLikeReadError::InputTooLong {
            length: input.len(),
            maximum: FUMEN_MAX_INPUT_BYTES,
        })
    );
}

#[test]
fn rejects_documents_beyond_the_page_budget() {
    let single = FumenLikeWriter::write(&FumenLikeTrace::new(vec!["x".to_owned()]))
        .expect("single-page fumen");
    let page_data = single
        .strip_prefix("v115@")
        .expect("writer version")
        .replace('?', "");
    let encoded = format!("v115@{}", page_data.repeat(FUMEN_MAX_PAGES + 1));

    assert_eq!(
        FumenLikeReader::read(&encoded),
        Err(FumenLikeReadError::TooManyPages {
            maximum: FUMEN_MAX_PAGES,
        })
    );
}

#[test]
fn rejects_external_fumen_without_clearra_payload_pages() {
    assert_eq!(
        FumenLikeReader::read("v115@vhAAgH"),
        Err(FumenLikeReadError::InvalidTrace(
            FumenLikeTraceError::EmptyPage { index: 0 }
        ))
    );
}

#[test]
fn reads_v115_from_url_and_ignores_query_parameters() {
    let trace = FumenLikeTrace::new(vec!["source=url".to_owned()]);
    let encoded = FumenLikeWriter::write(&trace).expect("URL fumen");
    let decoded = FumenLikeReader::read(&format!("https://harddrop.com/fumen/?{encoded}&foo=bar"))
        .expect("decoded");

    assert_eq!(decoded, trace);
}

#[test]
fn pco_external_pc_fumen_files_decode_as_contract_payloads() {
    let setup = read_source_setup(
        "tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_setup.fumen",
    );
    let representative = read_external_pc_trace_pages(
        "tests/fixtures/fumens/external-pc/pco_i_hold_6p_second_bag_pc_expected_any.fumen",
    );

    assert_eq!(setup.initial_board_mask(), 0x0000_00e0_f87e_3f87);
    assert_eq!(setup.visible_height(), 4);
    assert!(representative
        .iter()
        .any(|page| page.contains("representative_only=true")));
    assert!(representative.iter().any(|page| page
        .contains("accepted_solution_label_match_policy=at-least-one-source-label-family")));
    assert!(representative
        .iter()
        .any(|page| page.contains("final_board_empty=true")));
}

#[test]
fn fumen_like_reader_outputs_occupancy_field() {
    let trace = FumenLikeTrace::new(vec![
        "kind=scenario\ninitial_board_mask=0x00000000000003f0".to_owned()
    ]);
    let encoded = FumenLikeWriter::write(&trace).expect("occupancy fumen");

    let field = FumenLikeReader::read_occupancy_field(&encoded, 10, 4).expect("field");

    assert_eq!(field.width, 10);
    assert_eq!(field.height, 4);
    assert_eq!(field.mask, 0x03f0);
}

#[test]
fn tsar_external_pc_fumen_contract_pins_full_42_metadata() {
    let setup =
        read_source_setup("tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_setup.fumen");
    let full_set = read_external_pc_trace_pages(
        "tests/fixtures/fumens/external-pc/tsar_cannon_after_2bag_full_42.fumen",
    );

    assert_eq!(setup.initial_board_mask(), 0x0003_00c0_399e_3fdf);
    assert_eq!(setup.visible_height(), 5);
    assert!(full_set
        .iter()
        .any(|page| page.contains("expected_unique_solution_count=42")));
    assert!(full_set
        .iter()
        .any(|page| page.contains("unique_solution_count_basis=normalized-fumen-solution-set")));
    assert!(full_set
        .iter()
        .any(|page| page.contains("pc_probability_source_percent=98.69")));
    assert!(full_set
        .iter()
        .any(|page| page.contains("tsd_pc_probability_source_percent=73.2")));
    assert!(full_set
        .iter()
        .any(|page| page.contains("minimal_solve_set_is_metadata_only=true")));
}

fn read_external_pc_trace_pages(path: &str) -> Vec<String> {
    let full_path = workspace_root().join(path);
    let text = fs::read_to_string(&full_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", full_path.display()));
    FumenLikeReader::read(&text)
        .unwrap_or_else(|error| panic!("failed to decode {}: {error:?}", full_path.display()))
        .pages()
        .to_vec()
}

fn read_source_setup(path: &str) -> crate::codec::SourceFumenSetup {
    let full_path = workspace_root().join(path);
    let text = fs::read_to_string(&full_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", full_path.display()));
    crate::codec::SourceFumenSetup::decode(&text)
        .unwrap_or_else(|error| panic!("failed to decode {}: {error:?}", full_path.display()))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}
