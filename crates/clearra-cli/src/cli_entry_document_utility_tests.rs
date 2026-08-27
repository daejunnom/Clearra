use clearra_app::{
    decode_ctk3_exact, encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Operation, Ctk3Page,
    Ctk3Piece, Ctk3Rotation,
};
use clearra_fumen::ActualFumenDocumentTransform;

use super::*;
use crate::exit::ExitCode;

fn two_page_ctk3() -> String {
    encode_ctk3_compact(&Ctk3Document::new(
        2,
        vec![
            Ctk3Page::new(1, vec![Ctk3Color::Gray, Ctk3Color::Empty]),
            Ctk3Page::new(1, vec![Ctk3Color::Empty, Ctk3Color::Gray]),
        ],
    ))
    .expect("two-page CTK3")
}

#[test]
fn parity_browses_every_page_without_feasibility_or_pruning_authority() {
    let document = two_page_ctk3();
    let output = run_with_args([
        "clearra",
        "utility",
        "parity",
        "--document",
        document.as_str(),
    ]);

    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    assert!(output.stdout().contains("page: 1/2"));
    assert!(output.stdout().contains("page: 2/2"));
    assert_eq!(
        output.stdout().matches("feasibility_claim: false").count(),
        2
    );
    assert_eq!(
        output.stdout().matches("pruning_authority: none").count(),
        2
    );
    assert_eq!(
        output
            .stdout()
            .matches("pending_garbage_occupied_cell_count:")
            .count(),
        2
    );
}

#[test]
fn fumen_roundtrip_is_lossless_and_json_is_a_typed_document() {
    let document = ActualFumenDocumentTransform::text_to_fumen(&["typed document".to_owned()])
        .expect("canonical Fumen");
    let text = run_with_args([
        "clearra",
        "utility",
        "fumen",
        "roundtrip",
        "--document",
        document.as_str(),
    ]);
    assert_eq!(text.exit_code(), ExitCode::Success, "{}", text.stderr());
    assert_eq!(text.stdout(), document);

    let json = run_with_args([
        "clearra",
        "--format",
        "json",
        "utility",
        "fumen",
        "roundtrip",
        "--document",
        text.stdout(),
    ]);
    assert_eq!(json.exit_code(), ExitCode::Success, "{}", json.stderr());
    let value: serde_json::Value =
        serde_json::from_str(json.stdout()).expect("typed field-document JSON");
    assert_eq!(value["kind"], "field-document.v1");
    assert_eq!(value["payload_kind"], "field-document");
    assert_eq!(value["payload"]["format"], "fumen");
    assert_eq!(value["payload"]["document"], document);
}

#[test]
fn render_returns_only_metadata_and_requires_an_extension_matching_output() {
    let document = two_page_ctk3();
    let output = run_with_args([
        "clearra",
        "utility",
        "render",
        "--document",
        document.as_str(),
        "--artifact-format",
        "png",
        "--page",
        "1",
        "--output",
        "board.png",
    ]);
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    assert!(output.stdout().contains("contract_id: render-artifact.v1"));
    assert!(output.stdout().contains("media_type: image/png"));
    assert!(output.stdout().contains("render_exact: true"));
    assert!(!output.stdout().contains("iVBOR"));

    for args in [
        vec![
            "clearra",
            "utility",
            "render",
            "--document",
            document.as_str(),
            "--artifact-format",
            "png",
        ],
        vec![
            "clearra",
            "utility",
            "render",
            "--document",
            document.as_str(),
            "--artifact-format",
            "png",
            "--output",
            "board.gif",
        ],
    ] {
        let rejected = run_with_args(args);
        assert_eq!(rejected.exit_code(), ExitCode::ValidationFailed);
        assert!(rejected.stdout().is_empty());
    }
}

#[test]
fn native_document_file_accepts_canonical_content_and_rejects_plain_grid_text() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "clearra-cli-typed-document-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).expect("temporary directory");
    let canonical = directory.join("canonical.txt");
    let plain = directory.join("plain.txt");
    std::fs::write(&canonical, two_page_ctk3()).expect("canonical fixture");
    std::fs::write(&plain, "..........\nXXXXXXXXXX\n").expect("plain grid fixture");

    let accepted = run_with_args([
        "clearra",
        "utility",
        "parity",
        "--document-file",
        canonical.to_str().expect("UTF-8 path"),
    ]);
    assert_eq!(
        accepted.exit_code(),
        ExitCode::Success,
        "{}",
        accepted.stderr()
    );

    let rejected = run_with_args([
        "clearra",
        "utility",
        "parity",
        "--document-file",
        plain.to_str().expect("UTF-8 path"),
    ]);
    assert_eq!(rejected.exit_code(), ExitCode::ValidationFailed);
    assert!(rejected.stderr().contains("format inference failed"));

    std::fs::remove_file(canonical).expect("canonical cleanup");
    std::fs::remove_file(plain).expect("plain cleanup");
    std::fs::remove_dir(directory).expect("directory cleanup");
}

#[test]
fn to_gray_and_mirror_use_distinct_typed_routes() {
    let mut page = Ctk3Page::new(
        1,
        vec![
            Ctk3Color::Piece(Ctk3Piece::J),
            Ctk3Color::Empty,
            Ctk3Color::Piece(Ctk3Piece::S),
            Ctk3Color::Gray,
        ],
    );
    page.comment = "preserved".to_owned();
    page.garbage = Some(vec![
        Ctk3Color::Piece(Ctk3Piece::L),
        Ctk3Color::Empty,
        Ctk3Color::Piece(Ctk3Piece::Z),
        Ctk3Color::Gray,
    ]);
    page.operation = Some(Ctk3Operation {
        piece: Ctk3Piece::T,
        rotation: Ctk3Rotation::Right,
        x: 1,
        y: 0,
    });
    let source = encode_ctk3_compact(&Ctk3Document::new(4, vec![page])).expect("CTK3");

    let gray = run_with_args([
        "clearra",
        "utility",
        "to-gray",
        "--document",
        source.as_str(),
    ]);
    assert_eq!(gray.exit_code(), ExitCode::Success, "{}", gray.stderr());
    let before = decode_ctk3_exact(&source).expect("source");
    let after = decode_ctk3_exact(gray.stdout()).expect("gray result");
    assert_eq!(after.width, before.width);
    assert_eq!(after.pages[0].operation, before.pages[0].operation);
    assert_eq!(after.pages[0].comment, before.pages[0].comment);
    assert_eq!(after.pages[0].flags, before.pages[0].flags);
    assert_eq!(after.pages[0].height, before.pages[0].height);
    assert!(after.pages[0]
        .cells
        .iter()
        .chain(after.pages[0].garbage.as_ref().expect("garbage"))
        .all(|cell| matches!(cell, Ctk3Color::Empty | Ctk3Color::Gray)));

    let mirrored = run_with_args([
        "clearra",
        "utility",
        "mirror",
        "--document",
        source.as_str(),
    ]);
    assert_eq!(
        mirrored.exit_code(),
        ExitCode::Success,
        "{}",
        mirrored.stderr()
    );
    let restored = run_with_args([
        "clearra",
        "utility",
        "mirror",
        "--document",
        mirrored.stdout(),
    ]);
    assert_eq!(restored.exit_code(), ExitCode::Success);
    assert_eq!(
        decode_ctk3_exact(restored.stdout()),
        decode_ctk3_exact(&source)
    );

    let json = run_with_args([
        "clearra",
        "--format",
        "json",
        "utility",
        "mirror",
        "--document",
        source.as_str(),
    ]);
    let value: serde_json::Value = serde_json::from_str(json.stdout()).expect("typed JSON");
    assert_eq!(value["kind"], "field-document.v1");
    assert_eq!(value["result_kind"], "mirror");
    assert_eq!(value["payload"]["filename"], "clearra-mirror.ctk3");
}
