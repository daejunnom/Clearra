use std::{
    fs,
    path::{Path, PathBuf},
};

use clearra_output::fumen_like::{FumenLikeReader, FumenLikeWriter};

#[test]
fn fixture_fumens_roundtrip_through_clearra_trace_contract() {
    let fixture_dir = workspace_root()
        .join("tests")
        .join("fixtures")
        .join("fumens");
    let mut fixture_count = 0;

    for entry in fs::read_dir(&fixture_dir).expect("fumen fixture directory") {
        let path = entry.expect("fixture entry").path();
        if !is_fumen_fixture(&path) {
            continue;
        }

        fixture_count += 1;
        let encoded = fs::read_to_string(&path).expect("fumen fixture");
        let trace = FumenLikeReader::read(encoded.trim()).expect("fixture must decode");
        let reencoded = FumenLikeWriter::write(&trace).expect("reencoded fixture");
        let decoded_again = FumenLikeReader::read(&reencoded).expect("roundtrip decode");

        assert_eq!(decoded_again, trace, "fixture roundtrip failed: {path:?}");
        assert_expected_trace(&path, &trace.pages().join("\n===PAGE===\n"));
    }

    assert!(
        fixture_count > 0,
        "tests/fixtures/fumens must contain at least one .fumen fixture"
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn is_fumen_fixture(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("fumen")
}

fn assert_expected_trace(fumen_path: &Path, actual: &str) {
    let expected_path = fumen_path.with_extension("trace");
    if !expected_path.exists() {
        return;
    }

    let expected = fs::read_to_string(&expected_path)
        .expect("expected trace fixture")
        .replace("\r\n", "\n");
    assert_eq!(
        actual,
        expected.trim_end_matches('\n'),
        "fixture trace payload changed: {fumen_path:?}"
    );
}
