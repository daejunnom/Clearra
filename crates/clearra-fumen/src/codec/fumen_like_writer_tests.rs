use super::*;
use crate::codec::fumen_like_reader::FumenLikeReader;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{
    BuildVariantOperation, BuildVariantReplayInput, KickEvidenceEvent, ReplayEngine,
    RotationRequest,
};

#[test]
fn writes_external_v115_fumen_prefix() {
    let trace = FumenLikeTrace::new(vec!["kind=pc\nlines=2".to_owned()]);
    let encoded = FumenLikeWriter::write(&trace);

    assert!(encoded.starts_with("v115@"));
    assert!(!encoded.contains("\n---\n"));
}

#[test]
fn roundtrips_page_content_that_contains_the_old_separator() {
    let trace = FumenLikeTrace::new(vec!["kind=pc\n---\nlines=2".to_owned()]);
    let encoded = FumenLikeWriter::write(&trace);
    let decoded = FumenLikeReader::read(&encoded).expect("decoded fumen-like trace");

    assert_eq!(decoded, trace);
}

#[test]
fn roundtrips_multiple_pages() {
    let trace = FumenLikeTrace::new(vec![
        "kind=setup\nstatus=validated".to_owned(),
        "kind=build_coverage\ntemplate=basic".to_owned(),
    ]);
    let encoded = FumenLikeWriter::write(&trace);
    let decoded = FumenLikeReader::read(&encoded).expect("decoded fumen-like trace");

    assert_eq!(decoded, trace);
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

    let encoded = FumenLikeWriter::write_replay_trace(&trace);
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

    let encoded = FumenLikeWriter::write_replay_trace(&trace);
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

    let encoded = FumenLikeWriter::write_replay_trace(&trace);
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
