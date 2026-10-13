//! Actual native CLI and Desktop App jobs for few-piece 7/8/12/24L inputs.
//! This is not an IPC simulation, a Tauri window test, asset qualification,
//! or performance acceptance. CI reuses its current-source CPU build.
#![cfg(feature = "wasm-cpu-runtime")]

use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use clearra_app::{
    register_native_build_probability_host, NativeBuildProbabilityHostRegistration,
    SystemNativeBuildProbabilityAdmissionProvider,
};
use clearra_cli::{activate_native_accelerators_for_request, exit::ExitCode, run_with_args};
use clearra_gui_host::DesktopTauriCommandBridge;
use serde_json::{json, Value};

const PROFILES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];
const INPUTS: &str =
    include_str!("../../../tests/fixtures/contracts/extended_pc_surface_input.v1.tsv");
const PRODUCTS: [(&str, &str); 8] = [
    ("", "pc-scenario"),
    ("chance", "pc-probability.v2"),
    ("failed-queue", "pc-failed-queue.v2"),
    ("minimals", "pc-minimum-cover.v2"),
    ("score", "pc-score-summary.v2"),
    ("score-minimals", "pc-score-portfolio.v2"),
    ("score-finder", "pc-fixed-score-witness.v2"),
    ("path", "pc-path-family.v2"),
];

fn guarded_environment() -> String {
    let source = std::env::var("CLEARRA_REAL_DESKTOP_SOURCE_COMMIT")
        .expect("explicit current-source Desktop identity required");
    assert_eq!(source.len(), 40);
    assert!(source
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    assert_eq!(
        repository.join("_local/artifacts").canonicalize().unwrap(),
        repository.join("_local/artifacts"),
        "functional output root must not be a link"
    );
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let uuid = format!(
        "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
        std::process::id(),
        (nonce >> 48) & 0xffff,
        (nonce >> 36) & 0xfff,
        (nonce >> 24) & 0xfff,
        nonce & 0xffffffffffff
    );
    register_native_build_probability_host(
        NativeBuildProbabilityHostRegistration::new(
            SystemNativeBuildProbabilityAdmissionProvider,
            repository.join("_local/artifacts/v081-desktop-native-smoke/full-height-journals"),
            uuid,
        )
        .unwrap(),
    )
    .unwrap();
    source
}

fn arguments(input: &[&str], profile: &str, product: &str, b2b: bool) -> Vec<String> {
    let mut args = vec!["clearra".to_owned(), "pc".to_owned()];
    if !product.is_empty() {
        args.push(product.to_owned());
    }
    args.extend(
        [
            "--lines",
            input[1],
            "--height",
            input[1],
            "--board-mask",
            &format!("0x{}", input[2]),
            "--pieces",
            input[3],
            "--queue",
            input[4],
            "--no-hold",
            "--rule",
            profile,
            "--workers",
            "1",
            "--no-legal-board",
            "--no-conditioned-reachability",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    // --board-mask/--height/--pieces select the shared CLI grammar, which
    // already lowers --queue to a fixed sequence for the ordinary PC product.
    // Finder excludes TB semantics; all three score products own CPU execution.
    // Keep the request valid before either native CLI or Desktop lowering.
    if product != "score-finder" {
        args.push("--no-tablebase".to_owned());
    }
    if !matches!(product, "score" | "score-minimals" | "score-finder") {
        args.extend(["--backend".to_owned(), "cpu".to_owned()]);
    }
    if product == "failed-queue" {
        args.extend(["--failed-count".to_owned(), "1".to_owned()]);
    }
    if b2b {
        args.extend([
            "--preserve-b2b".to_owned(),
            "--spin-profile".to_owned(),
            "t-spins".to_owned(),
        ]);
    }
    args
}

fn envelope(args: &[String]) -> String {
    json!({ "app_request_model": "clearra-cli/CommandRequest", "command": "cli",
        "language": "ko", "arguments": args })
    .to_string()
}

fn runtime(value: &Value, source: &str) {
    assert_eq!(value["runtime_identity"]["source_commit"], source);
    assert_eq!(value["runtime_identity"]["engine_build_id"], source);
}

fn cli(args: &[String], source: &str, kind: &str) -> Value {
    let native = ["clearra", "--format", "json", "--include-solution-data"]
        .into_iter()
        .map(str::to_owned)
        .chain(args.iter().skip(1).cloned());
    let output = run_with_args(native);
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());
    let value: Value = serde_json::from_str(output.stdout()).unwrap();
    runtime(&value, source);
    assert_eq!(value["kind"], kind);
    value
}

fn job(bridge: &mut DesktopTauriCommandBridge, args: &[String], source: &str, kind: &str) -> Value {
    let wire = envelope(args);
    let request = bridge.parse_app_request(&wire).unwrap();
    bridge.ensure_no_running_job().unwrap();
    activate_native_accelerators_for_request(&request).unwrap();
    let id = bridge.start_job(&wire).unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let events: Vec<Value> = serde_json::from_str(&bridge.get_job_events(id).unwrap()).unwrap();
        for event in events {
            assert_eq!(event["schema_version"], 1);
            assert_eq!(event["job_id"], id);
            match event["event"].as_str().unwrap() {
                "completed" => {
                    assert_eq!(event["response"]["status"], "success", "{event}");
                    assert_eq!(event["response"]["result"]["kind"], kind, "{event}");
                    runtime(&event["response"], source);
                    assert!(event["search_report"].is_object(),
                        "actual Desktop CPU worker must provide its owned report; enable clearra-gui-host/wasm-cpu-runtime");
                    // Sending the terminal event precedes the OS thread's final
                    // return. Wait for that owned thread, never reactivate a
                    // process-wide profile registry while it is still alive.
                    while bridge.ensure_no_running_job().is_err() {
                        assert!(
                            Instant::now() < deadline,
                            "terminal Desktop worker did not drain"
                        );
                        thread::sleep(Duration::from_millis(1));
                    }
                    return event;
                }
                "failed" | "cancelled" => panic!("actual full-height Desktop job failed: {event}"),
                "started" | "progress" | "diagnostic" => {}
                unknown => panic!("unknown production Desktop event: {unknown}"),
            }
        }
        if Instant::now() >= deadline {
            bridge.cancel_job(id).unwrap();
            panic!("few-piece Desktop fixture exceeded its finite deadline");
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn full_key(value: &Value, input: &[&str]) {
    let prefix = format!("ctk2|height={}|initial={}|placements=", input[1], input[2]);
    assert!(
        value.as_str().is_some_and(|key| key.starts_with(&prefix)),
        "full-height physical identity must survive without CTK1 truncation: {value}"
    );
}

fn compare_score_winner(host: &Value, native: &Value, family: &Value) {
    // CLI exposes family metadata on each winner; Desktop keeps that same
    // metadata once on the family. Compare every authority, not raw shapes.
    for field in [
        "pattern_id",
        "candidate_id",
        "normalized_solution_key",
        "score",
        "informational_attack",
    ] {
        assert_eq!(
            host[field], native[field],
            "exact native/Desktop score winner field {field}"
        );
    }
    assert_eq!(native["contract"], family["winner_contract"]);
    assert_eq!(
        native["informational_attack_basis"],
        family["informational_attack_basis"]
    );
}

fn compare_payload(event: &Value, native: &Value, input: &[&str], product: &str, b2b: bool) {
    let report = &event["search_report"];
    let summary = &native["summary"];
    let public = &event["response"]["product_result_payload"];
    let payload = &public["content"]["payload"];
    match product {
        "minimals" | "score-minimals" => {
            assert_eq!(public["result_kind"], native["kind"]);
            for field in [
                "set_identity_sha256",
                "candidate_map_sha256",
                "alternative_index",
                "optimal_cardinality",
                "known_alternative_count",
                "total_alternative_count",
                "enumeration_complete",
                "member_page_number",
                "total_member_pages",
                "members",
            ] {
                assert_eq!(
                    payload[field], summary[field],
                    "exact native/Desktop portfolio field {field}"
                );
            }
            let empty = b2b && input[1] == "7";
            assert_eq!(
                payload["optimal_cardinality"],
                if empty { "0" } else { "1" }
            );
            assert_eq!(
                payload["members"].as_array().unwrap().len(),
                usize::from(!empty)
            );
            if !empty {
                full_key(&payload["members"][0]["normalized_solution_key"], input);
            }
            if product == "minimals" {
                assert_eq!(
                    payload["canonical_selection"],
                    "smallest-canonical-candidate-id"
                );
                assert_eq!(payload["canonical_witness"], summary["canonical_witness"]);
                if empty {
                    assert!(payload["canonical_witness"].is_null());
                    assert!(payload.get("unsatisfiable").is_none());
                }
            }
        }
        "path" => {
            assert_eq!(public["result_kind"], "pc-path-family.v2");
            for field in [
                "complete",
                "witness_count",
                "canonical_selection",
                "canonical_witness",
                "witnesses",
            ] {
                assert_eq!(
                    payload[field], summary[field],
                    "exact native/Desktop replay field {field}"
                );
            }
            if b2b && input[1] == "7" {
                assert_eq!(payload["witness_count"], "0");
                assert!(payload["canonical_witness"].is_null());
            } else {
                let witness = &payload["canonical_witness"];
                assert!(witness["normalized_trace_key"]
                    .as_str()
                    .unwrap()
                    .starts_with("trk2:"));
                let steps = witness["steps"].as_array().unwrap();
                assert_eq!(steps.len(), input[3].parse::<usize>().unwrap());
                assert_eq!(steps[0]["board_before_mask"], format!("0x{}", input[2]));
                assert_eq!(
                    steps.last().unwrap()["board_after_line_clear_mask"],
                    format!("0x{}", "0".repeat(64))
                );
            }
        }
        "score" => {
            assert_eq!(public["result_kind"], "pc-score-summary.v2");
            assert_eq!(payload["fields"], summary["score_solution_fields"]);
            assert_eq!(payload["complete"], true);
            assert_eq!(payload["fields"].as_array().unwrap().len(), 1);
            full_key(&payload["fields"][0]["normalized_field_key"], input);
            assert_eq!(payload["overall_score"], summary["score_overall_score"]);
        }
        "score-finder" => {
            assert_eq!(public["result_kind"], "pc-fixed-score-witness.v2");
            assert_eq!(
                payload["canonical_selection"],
                summary["score_pattern_canonical_selection"]
            );
            compare_score_winner(
                &payload["canonical_winner"],
                &summary["score_pattern_canonical_winner"],
                payload,
            );
            let host_winners = payload["winners"].as_array().unwrap();
            let native_winners = summary["score_pattern_winners"].as_array().unwrap();
            assert_eq!(host_winners.len(), native_winners.len());
            for (host, native) in host_winners.iter().zip(native_winners) {
                compare_score_winner(host, native, payload);
            }
            full_key(
                &payload["canonical_winner"]["normalized_solution_key"],
                input,
            );
        }
        "chance" | "failed-queue" => {
            assert_eq!(report["covered_pattern_count"], 1);
            assert_eq!(report["coverage_probability"], "1");
            assert_eq!(report["probability_complete"], true);
            if product == "chance" {
                assert_eq!(summary["covered_pattern_count"], 1);
                assert_eq!(summary["coverage_probability"], 1.0);
                assert_eq!(summary["probability_complete"], true);
                assert!(summary.get("probability").is_none());
            } else {
                assert_eq!(summary["failed_pattern_count"], 0);
                assert_eq!(summary["failed_queue_probability"], 0.0);
            }
        }
        "" => {
            assert_eq!(report["solution_keys_complete"], true);
            assert_eq!(
                report["normalized_solution_keys"],
                native["contract"]["artifacts"]["solution_keys"]
            );
            assert_eq!(
                report["normalized_solution_keys"].as_array().unwrap().len(),
                1
            );
            full_key(&report["normalized_solution_keys"][0], input);
        }
        _ => panic!("unreviewed full-height Desktop product"),
    }
}

#[test]
#[ignore = "requires current-source CPU/GuiHost build and an explicit source identity; no asset generation"]
fn actual_desktop_jobs_preserve_all_full_height_cli_products_and_exact_empty_minimum() {
    let source = guarded_environment();
    let mut bridge = DesktopTauriCommandBridge::default();
    let inputs: Vec<_> = INPUTS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .collect();
    assert_eq!(inputs.len(), 4);
    let mut cases = 0;
    for profile in PROFILES {
        for input in &inputs {
            for (product, kind, b2b) in PRODUCTS
                .into_iter()
                .map(|(product, kind)| (product, kind, false))
                .chain([
                    ("minimals", "pc-minimum-cover.v2", true),
                    ("path", "pc-path-family.v2", true),
                ])
            {
                let args = arguments(input, profile, product, b2b);
                let native = cli(&args, &source, kind);
                let event = job(&mut bridge, &args, &source, kind);
                compare_payload(&event, &native, input, product, b2b);
                if product == "minimals" || product == "score-minimals" {
                    let page: Value =
                        serde_json::from_str(&bridge.product_page_get("1", "1").unwrap()).unwrap();
                    assert_eq!(page["state"], "page");
                    assert_eq!(
                        page["page"]["optimal_cardinality"],
                        native["summary"]["optimal_cardinality"]
                    );
                    assert_eq!(page["page"]["members"], native["summary"]["members"]);
                    let repeat: Value =
                        serde_json::from_str(&bridge.product_page_get("1", "1").unwrap()).unwrap();
                    assert_eq!(repeat, page, "page reread must not create a new portfolio");
                }
                // Check the actual synchronous Tauri command seam separately;
                // do not confuse a successful mock IPC response with this job.
                if input[1] == "7" && product == "minimals" && b2b {
                    let wire = envelope(&args);
                    let request = bridge.parse_app_request(&wire).unwrap();
                    bridge.ensure_no_running_job().unwrap();
                    activate_native_accelerators_for_request(&request).unwrap();
                    let sync: Value =
                        serde_json::from_str(&bridge.run_request(&wire).unwrap()).unwrap();
                    runtime(&sync, &source);
                    assert_eq!(sync["status"], "success");
                    assert_eq!(
                        sync["product_result_payload"],
                        event["response"]["product_result_payload"]
                    );
                }
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 200);
    eprintln!("source={source}: 200 actual full-height CLI/Desktop product pairs; release_authority=false");
}
