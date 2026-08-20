use super::*;
use crate::codec::fumen_like_reader::FumenLikeReader;
use crate::codec::FUMEN_MAX_PAGES;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{
    BuildVariantOperation, BuildVariantReplayInput, KickEvidenceEvent, ReplayEngine,
    RotationRequest,
};

#[test]
fn writes_external_v115_fumen_prefix() {
    let trace = FumenLikeTrace::new(vec!["kind=pc\nlines=2".to_owned()]);
    let encoded = FumenLikeWriter::write(&trace).expect("encoded fumen");

    assert!(encoded.starts_with("v115@"));
    assert!(!encoded.contains("\n---\n"));
}

#[test]
fn roundtrips_page_content_that_contains_the_old_separator() {
    let trace = FumenLikeTrace::new(vec!["kind=pc\n---\nlines=2".to_owned()]);
    let encoded = FumenLikeWriter::write(&trace).expect("encoded fumen");
    let decoded = FumenLikeReader::read(&encoded).expect("decoded fumen-like trace");

    assert_eq!(decoded, trace);
}

#[test]
fn roundtrips_multiple_pages() {
    let trace = FumenLikeTrace::new(vec![
        "kind=setup\nstatus=validated".to_owned(),
        "kind=build_coverage\ntemplate=basic".to_owned(),
    ]);
    let encoded = FumenLikeWriter::write(&trace).expect("encoded fumen");
    let decoded = FumenLikeReader::read(&encoded).expect("decoded fumen-like trace");

    assert_eq!(decoded, trace);
}

#[test]
fn roundtrips_percent_hangul_and_astral_unicode_comments() {
    let trace = FumenLikeTrace::new(vec!["주석 100% 😀 / clearra".to_owned()]);
    let encoded = FumenLikeWriter::try_write(&trace).expect("unicode fumen");
    let decoded = FumenLikeReader::read(&encoded).expect("decoded unicode fumen");

    assert_eq!(
        encoded,
        "v115@vhAAgWyAlvQSBGFEfEDqG6BFb85AQo78A1no2Al/SS?BTGEfEE4k2AFbMzAFbsiDs4DXEyiBAA"
    );
    assert_eq!(decoded, trace);
}

#[test]
fn enforces_the_escaped_comment_length_boundary_without_truncation() {
    for length in [4094usize, 4095] {
        let trace = FumenLikeTrace::new(vec!["A".repeat(length)]);
        let encoded = FumenLikeWriter::try_write(&trace).expect("boundary comment");
        assert_eq!(FumenLikeReader::read(&encoded), Ok(trace));
    }

    let too_long = FumenLikeTrace::new(vec!["A".repeat(4096)]);
    assert_eq!(
        FumenLikeWriter::try_write(&too_long),
        Err(FumenLikeWriteError::CommentTooLong {
            index: 0,
            length: 4096,
        })
    );

    let percent_boundary = FumenLikeTrace::new(vec![format!("{}%", "A".repeat(4092))]);
    let encoded = FumenLikeWriter::try_write(&percent_boundary).expect("percent boundary");
    assert_eq!(FumenLikeReader::read(&encoded), Ok(percent_boundary));

    let split_escape = FumenLikeTrace::new(vec![format!("{}%", "A".repeat(4093))]);
    assert_eq!(
        FumenLikeWriter::try_write(&split_escape),
        Err(FumenLikeWriteError::CommentTooLong {
            index: 0,
            length: 4096,
        })
    );
}

#[test]
fn rejects_page_sets_beyond_the_reader_budget() {
    let trace = FumenLikeTrace::new(vec!["x".to_owned(); FUMEN_MAX_PAGES + 1]);

    assert_eq!(
        FumenLikeWriter::try_write(&trace),
        Err(FumenLikeWriteError::TooManyPages {
            length: FUMEN_MAX_PAGES + 1,
            maximum: FUMEN_MAX_PAGES,
        })
    );
}

#[test]
fn writes_replay_trace_as_fumen_like_comment_pages() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "variant-1",
        layout,
        0,
        vec![BuildVariantOperation::new(
            PieceKind::O,
            RotationState::Zero,
            0,
            0,
        )],
    );
    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");

    let encoded = FumenLikeWriter::write_replay_trace(&trace).expect("encoded replay fumen");
    let decoded = FumenLikeReader::read(&encoded).expect("decoded replay trace");

    assert!(decoded.pages()[0].contains("kind=replay-trace"));
    assert!(decoded.pages()[0].contains("representative=true"));
    assert!(decoded.pages()[0].contains("sample=true"));
    assert!(decoded.pages()[1].contains("kind=replay-step"));
    assert!(decoded
        .pages()
        .iter()
        .any(|page| page.contains("type=score-basis")));
    assert!(decoded
        .pages()
        .iter()
        .any(|page| page.contains("type=board-snapshot")));
    assert!(decoded
        .pages()
        .iter()
        .any(|page| page.contains("type=lock") && page.contains("cleared_cell_owner_count=")));
}

#[test]
fn fumen_like_writer_consumes_replay_trace() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "variant-fumen-adapter",
        layout,
        0,
        vec![BuildVariantOperation::new(
            PieceKind::O,
            RotationState::Zero,
            0,
            0,
        )],
    );
    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");

    let encoded = FumenLikeWriter::write_replay_trace(&trace).expect("encoded replay fumen");
    let decoded = FumenLikeReader::read(&encoded).expect("decoded replay trace");

    assert!(decoded.pages()[0].contains("kind=replay-trace"));
    assert!(decoded
        .pages()
        .iter()
        .any(|page| page.contains("kind=replay-event") && page.contains("type=lock")));
}

#[test]
fn fumen_writer_consumes_replay_trace_events_not_core_candidate() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let kick = KickEvidenceEvent::new(0, 0, 1, RotationRequest::Clockwise, 2, 1, -1);
    let input = BuildVariantReplayInput::new(
        "variant-kick",
        layout,
        0,
        vec![BuildVariantOperation::new(
            PieceKind::I,
            RotationState::Zero,
            0,
            0,
        )],
    )
    .with_kick_evidence(vec![kick]);
    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");

    let encoded = FumenLikeWriter::write_replay_trace(&trace).expect("encoded replay fumen");
    let decoded = FumenLikeReader::read(&encoded).expect("decoded replay trace");

    assert!(decoded
        .pages()
        .iter()
        .any(|page| page.contains("kind=replay-event") && page.contains("type=kick-evidence")));
    assert!(decoded
        .pages()
        .iter()
        .any(|page| page.contains("rotation_request=clockwise")));
}
