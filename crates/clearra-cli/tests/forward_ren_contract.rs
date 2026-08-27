use clearra_cli::{exit::ExitCode, run_with_args};
use serde_json::Value;

#[test]
fn forward_ren_cli_e2e_emits_typed_exact_chain_results() {
    let output = run_with_args([
        "clearra",
        "--format",
        "json",
        "--include-solution-data",
        "ren",
        "--board-mask",
        "0x3f",
        "--height",
        "4",
        "--queue",
        "I",
        "--no-hold",
        "--workers",
        "1",
    ]);

    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    let value: Value = serde_json::from_str(output.stdout()).expect("typed REN JSON");
    assert_eq!(value["kind"], "ren");
    assert_eq!(value["summary"]["complete"], true);
    assert_eq!(value["summary"]["maximum_damage"], Value::Null);
    assert_eq!(value["summary"]["maximum_ren"], 0);
    assert_eq!(value["contract"]["command"]["kind"], "ren");
    assert_eq!(value["contract"]["solution_data"]["requested"], true);
    assert_eq!(value["contract"]["solution_data"]["status"], "complete");
    assert_eq!(
        value["contract"]["artifacts"]["forward"]["outcomes"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "{}",
        output.stdout()
    );
    let outcome = &value["contract"]["artifacts"]["forward"]["outcomes"][0];
    assert_eq!(outcome["id"], 1);
    assert_eq!(outcome["source_queue"], "I");
    assert_eq!(outcome["ren_count"], 0);
    assert_eq!(
        outcome["path"].as_array().map(Vec::len),
        Some(1),
        "{}",
        output.stdout()
    );
    assert_eq!(outcome["path"][0]["cleared_row_mask"], 1);
}
