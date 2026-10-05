//! SRP rationale: exercise the production native Desktop job boundary using
//! already installed signed assets and the common CLI activation adapter.
//! The CLI owns this integration test: GuiHost must not depend on CLI,
//! including through dev-dependencies. Production dependencies stay unchanged.
//! This is a bounded functional test, not asset qualification, a Tauri window
//! test, a benchmark, or a replacement solver.
#![cfg(feature = "wasm-cpu-runtime")]

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::atomic::AtomicBool,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use clearra_accelerator_product_host::{
    embedded_catalog, CatalogProfileStatus, ProductCatalogKind,
};
use clearra_accelerator_runtime::active_identity;
use clearra_app::{
    register_native_build_probability_host, NativeBuildProbabilityHostRegistration,
    SystemNativeBuildProbabilityAdmissionProvider,
};
use clearra_cli::{activate_native_accelerators_for_request, run_native_accelerator_action};
use clearra_gui_host::DesktopTauriCommandBridge;
use serde::Deserialize;
use serde_json::{json, value::RawValue, Value};

const PROFILES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];
const PRODUCTS: [(&str, ProductCatalogKind); 2] = [
    ("exact-legal-board", ProductCatalogKind::ExactLegalBoard),
    (
        "board-conditioned-reachability",
        ProductCatalogKind::BoardConditionedReachability,
    ),
];
const POLICIES: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

fn guarded_environment() -> String {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    let assets = repository.join("_local/artifacts/v081-compute-data-smoke");
    assert_eq!(
        repository.join("_local/artifacts").canonicalize().unwrap(),
        repository.join("_local/artifacts"),
        "the functional output root must not escape through a link"
    );
    for (name, directory) in [
        ("CLEARRA_LEGAL_BOARD_DIRECTORY", "legal-board"),
        (
            "CLEARRA_CONDITIONED_REACHABILITY_DIRECTORY",
            "conditioned-reachability",
        ),
    ] {
        let configured =
            PathBuf::from(std::env::var(name).expect("explicit installed fixture required"));
        assert!(configured.is_absolute());
        assert_eq!(configured.canonicalize().unwrap(), assets.join(directory));
    }
    let source = std::env::var("CLEARRA_REAL_DESKTOP_SOURCE_COMMIT")
        .expect("explicit source identity required");
    assert_eq!(source.len(), 40);
    assert!(source
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    let journal = repository.join("_local/artifacts/v081-desktop-native-smoke/journals");
    // The production system admission provider is used, with only its durable
    // test journal redirected into the manager-approved functional root.
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
            journal,
            uuid,
        )
        .unwrap(),
    )
    .unwrap();
    source
}

fn lifecycle(product: &str, action: &str, profile: &str) -> Value {
    serde_json::from_str(
        &run_native_accelerator_action(
            product,
            action,
            profile,
            &AtomicBool::new(false),
            &mut |_, _| panic!("status/check must not download"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn installed() -> Value {
    Value::Array(
        PRODUCTS
            .iter()
            .flat_map(|(product, _)| {
                PROFILES.iter().map(move |profile| {
                    let status = lifecycle(product, "status", profile);
                    assert_eq!(status["installed"], true);
                    assert_eq!(status["qualified"], true);
                    assert_eq!(status["validation"], "ready");
                    json!({ "product": product, "profile": profile,
            "catalog": status["catalog_identity"],
            "generation": status["installed_generation_identity"],
            "bytes": status["installed_payload_bytes"] })
                })
            })
            .collect(),
    )
}

fn arguments(input: &[&str], profile: &str, legal: bool, relation: bool) -> Vec<String> {
    let mut arguments: Vec<String> = ["clearra"]
        .into_iter()
        .chain(input.iter().copied())
        .chain([
            "--backend",
            "cpu",
            "--workers",
            "1",
            "--rule",
            profile,
            if legal {
                "--legal-board"
            } else {
                "--no-legal-board"
            },
            if relation {
                "--conditioned-reachability"
            } else {
                "--no-conditioned-reachability"
            },
        ])
        .map(str::to_owned)
        .collect();
    if input.first() == Some(&"pc") {
        arguments.push("--no-tablebase".to_owned());
    }
    arguments
}

fn envelope(arguments: &[String]) -> String {
    json!({ "app_request_model": "clearra-cli/CommandRequest", "command": "cli",
        "language": "ko", "arguments": arguments })
    .to_string()
}

fn start(
    bridge: &mut DesktopTauriCommandBridge,
    arguments: &[String],
    profile: &str,
    legal: bool,
    relation: bool,
) -> u64 {
    let wire = envelope(arguments);
    // This is the actual order used by Tauri's start_job command. Do not
    // pre-clear the page store or manufacture a registered asset in the test.
    let request = bridge.parse_app_request(&wire).unwrap();
    bridge.ensure_no_running_job().unwrap();
    activate_native_accelerators_for_request(&request).unwrap();
    for ((product, kind), enabled) in PRODUCTS.iter().zip([legal, relation]) {
        for other in PROFILES {
            assert_eq!(
                active_identity(*kind, other).is_some(),
                enabled && other == profile,
                "only the selected qualified profile may remain resident"
            );
        }
        if enabled {
            let catalog = lifecycle(product, "check", profile);
            assert_eq!(catalog["qualified"], true);
            let (generation, signed_catalog) = active_identity(*kind, profile).unwrap();
            let hex = |bytes: [u8; 32]| {
                bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            };
            assert_eq!(hex(generation), catalog["generation_identity"]);
            let verified = embedded_catalog(*kind).unwrap();
            assert_eq!(
                hex(verified.catalog_identity()),
                catalog["catalog_identity"]
            );
            let CatalogProfileStatus::Qualified(asset) = verified.profile(profile).unwrap() else {
                panic!("selected profile must retain signed qualification");
            };
            // Runtime authority identifies the profile's signed statement,
            // whereas lifecycle metadata identifies the entire catalog file.
            assert_eq!(signed_catalog, asset.authority().statement_identity());
        }
    }
    bridge.start_job(&wire).unwrap()
}

fn terminal(bridge: &mut DesktopTauriCommandBridge, job: u64) -> Value {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let events: Vec<Value> =
            serde_json::from_str(&bridge.get_job_events(job).unwrap()).unwrap();
        for event in events {
            assert_eq!(event["schema_version"], 1);
            assert_eq!(event["job_id"], job);
            match event["event"].as_str().unwrap() {
                "completed" | "failed" | "cancelled" => {
                    bridge.ensure_no_running_job().unwrap();
                    return event;
                }
                "started" | "progress" | "diagnostic" => {}
                unknown => panic!("unknown production event: {unknown}"),
            }
        }
        if Instant::now() >= deadline {
            bridge.cancel_job(job).unwrap();
            panic!("functional Desktop request exceeded its bounded deadline");
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn complete(
    bridge: &mut DesktopTauriCommandBridge,
    arguments: &[String],
    profile: &str,
    legal: bool,
    relation: bool,
    source: &str,
) -> Value {
    let job = start(bridge, arguments, profile, legal, relation);
    let event = terminal(bridge, job);
    assert_eq!(event["event"], "completed", "{event}");
    assert_eq!(event["response"]["status"], "success", "{event}");
    assert_eq!(
        event["response"]["runtime_identity"]["source_commit"],
        source
    );
    assert_eq!(
        event["response"]["runtime_identity"]["engine_build_id"],
        source
    );
    assert!(event["search_report"].is_object());
    event["search_report"].clone()
}

fn probability(value: &Value) -> f64 {
    let number = value
        .as_str()
        .map(|text| text.parse::<f64>().unwrap())
        .unwrap_or_else(|| value.as_f64().unwrap());
    assert!(number.is_finite());
    number
}

#[derive(Deserialize)]
struct RawCliProbabilities {
    summary: RawCoverage,
    contract: RawContract,
}
#[derive(Deserialize)]
struct RawCoverage {
    coverage_probability: Box<RawValue>,
}
#[derive(Deserialize)]
struct RawContract {
    artifacts: RawArtifacts,
}
#[derive(Deserialize)]
struct RawArtifacts {
    solution_probabilities: Vec<RawProbability>,
}
#[derive(Deserialize)]
struct RawProbability {
    probability: Box<RawValue>,
}

fn retain_probability_literals(text: &str, value: &mut Value) {
    // serde_json without float_roundtrip can add an ULP while reading an
    // otherwise identical native JSON number. Preserve the real wire token,
    // then use Rust's correctly rounded float parser, just as for the Desktop
    // probability strings. Never loosen equality or change production features.
    let raw: RawCliProbabilities = serde_json::from_str(text).unwrap();
    let literal = |raw: &RawValue| {
        let text = if raw.get().starts_with('"') {
            serde_json::from_str::<String>(raw.get()).unwrap()
        } else {
            raw.get().to_owned()
        };
        let result = Value::String(text);
        probability(&result);
        result
    };
    value["summary"]["coverage_probability"] = literal(&raw.summary.coverage_probability);
    let rows = value["contract"]["artifacts"]["solution_probabilities"]
        .as_array_mut()
        .unwrap();
    assert_eq!(
        rows.len(),
        raw.contract.artifacts.solution_probabilities.len()
    );
    for (row, raw) in rows
        .iter_mut()
        .zip(raw.contract.artifacts.solution_probabilities)
    {
        row["probability"] = literal(&raw.probability);
    }
}

fn assert_meaning(actual: &Value, expected: &Value) {
    assert_eq!(
        actual.as_object().unwrap().len(),
        expected.as_object().unwrap().len()
    );
    for (field, value) in expected.as_object().unwrap() {
        assert_eq!(
            &actual[field], value,
            "exact candidate meaning field: {field}"
        );
    }
}

fn meaning(value: &Value, cli: bool) -> Value {
    let summary = if cli { &value["summary"] } else { value };
    let keys = if cli {
        &value["contract"]["artifacts"]["solution_keys"]
    } else {
        &value["normalized_solution_keys"]
    };
    let keys = keys
        .as_array()
        .expect("complete candidate identities required");
    assert_eq!(
        keys.iter()
            .map(|key| key.as_str().unwrap())
            .collect::<BTreeSet<_>>()
            .len(),
        keys.len()
    );
    assert!(keys
        .windows(2)
        .all(|pair| pair[0].as_str() < pair[1].as_str()));
    for field in [
        "solution_count_calculated",
        "solution_set_materialized",
        "solution_keys_complete",
        "coverage_calculated",
        "probability_calculated",
        "probability_complete",
        "count_complete",
    ] {
        assert_eq!(summary[field], true, "{field}: {summary}");
    }
    assert_eq!(summary["workers_used"], 1);
    let probabilities = if cli {
        &value["contract"]["artifacts"]["solution_probabilities"]
    } else {
        &value["solution_probabilities"]
    };
    let probabilities: Vec<_> = probabilities
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            json!({
        "key": entry["solution_key"], "probability": probability(&entry["probability"]),
        "covered": entry["covered_pattern_count"], "total": entry["pattern_count"],
        "complete": entry["probability_complete"] })
        })
        .collect();
    assert_eq!(probabilities.len(), keys.len());
    json!({ "keys": keys, "hash": summary["normalized_solution_set_hash"],
        "covered": summary["covered_pattern_count"], "total": summary["total_possible_pattern_count"],
        "probability": probability(&summary["coverage_probability"]), "probabilities": probabilities })
}

fn cli_result(arguments: &[String], source: &str) -> Value {
    // Output formatting belongs to the native CLI presenter, not to the
    // Desktop CommandRequest's product argv. Keep the product tokens exact.
    let native_arguments = ["clearra", "--format", "json", "--include-solution-data"]
        .into_iter()
        .map(str::to_owned)
        .chain(arguments.iter().skip(1).cloned());
    let result = clearra_cli::run_with_args(native_arguments);
    assert_eq!(
        result.exit_code(),
        clearra_cli::exit::ExitCode::Success,
        "{}",
        result.stderr()
    );
    let mut value: Value = serde_json::from_str(result.stdout()).unwrap();
    retain_probability_literals(result.stdout(), &mut value);
    assert_eq!(value["runtime_identity"]["source_commit"], source);
    assert_eq!(value["runtime_identity"]["engine_build_id"], source);
    value
}

#[test]
#[ignore = "requires explicitly installed unchanged signed packs and a current-source CPU runtime"]
fn installed_signed_assets_survive_actual_desktop_profile_jobs_pages_and_cancellation() {
    let source = guarded_environment();
    let before = installed();
    let mut bridge = DesktopTauriCommandBridge::default();
    // Check the distinct Build presenter/ingress before the longer profile
    // matrix. Its product has no Tablebase switch; PC explicitly disables TB.
    let build_input = [
        "build-probability",
        "--base-mask",
        "0",
        "--target-mask",
        "15",
        "--height",
        "4",
        "--queue",
        "I",
        "--no-hold",
        "--no-mirror",
        "--result-mode",
        "all-solutions",
        "--solution-probabilities",
    ];
    let baseline = meaning(
        &cli_result(&arguments(&build_input, "srs-plus", false, false), &source),
        true,
    );
    for (legal, relation) in POLICIES {
        let arguments = arguments(&build_input, "srs-plus", legal, relation);
        assert_meaning(
            &meaning(
                &complete(
                    &mut bridge,
                    &arguments,
                    "srs-plus",
                    legal,
                    relation,
                    &source,
                ),
                false,
            ),
            &baseline,
        );
    }
    let eligible = [
        "pc",
        "--lines",
        "4",
        "--height",
        "4",
        "--board-mask",
        "0",
        "--pieces",
        "10",
        "--queue",
        "IIOOOIIOOO",
        "--no-hold",
        "--objective",
        "unique",
        "--count",
        "unique",
        "--solution-probabilities",
    ];
    let initial = [
        "pc",
        "--lines",
        "4",
        "--height",
        "4",
        "--board-mask",
        "0x3c0f03c0f",
        "--pieces",
        "6",
        "--patterns",
        "P7",
        "--hold",
        "empty",
        "--objective",
        "unique",
        "--count",
        "unique",
        "--solution-probabilities",
    ];
    for (index, profile) in PROFILES.iter().enumerate() {
        for (input, expected) in [
            (&eligible[..], 159),
            (&initial[..], [245, 246, 289, 246, 175][index]),
        ] {
            let baseline_arguments = arguments(input, profile, false, false);
            let baseline = meaning(&cli_result(&baseline_arguments, &source), true);
            assert_eq!(baseline["keys"].as_array().unwrap().len(), expected);
            for (legal, relation) in POLICIES {
                let arguments = arguments(input, profile, legal, relation);
                assert_meaning(
                    &meaning(
                        &complete(&mut bridge, &arguments, profile, legal, relation, &source),
                        false,
                    ),
                    &baseline,
                );
            }
        }
        let minimum = arguments(
            &[
                "pc",
                "minimals",
                "--lines",
                "2",
                "--height",
                "2",
                "--board-mask",
                "0",
                "--pieces",
                "5",
                "--queue",
                "IIOOO",
                "--no-hold",
            ],
            profile,
            true,
            true,
        );
        complete(&mut bridge, &minimum, profile, true, true, &source);
        let first: Value =
            serde_json::from_str(&bridge.product_page_get("1", "1").unwrap()).unwrap();
        assert_eq!(first["state"], "page");
        assert_eq!(first["page"]["optimal_cardinality"], "1");
        assert_eq!(first["page"]["known_alternative_count"], "1");
        assert_eq!(first["page"]["enumeration_complete"], false);
        assert!(first["page"]["total_alternative_count"].is_null());
        assert_eq!(first["page"]["members"].as_array().unwrap().len(), 1);
        // A getter cannot manufacture an as-yet unenumerated alternative.
        // Follow the production GUI's explicit bounded lazy-next operation.
        let second: Value = serde_json::from_str(&bridge.product_page_next(4096).unwrap()).unwrap();
        assert_eq!(second["state"], "page");
        assert_eq!(second["page"]["alternative_index"], "2");
        let reread: Value =
            serde_json::from_str(&bridge.product_page_get("2", "1").unwrap()).unwrap();
        assert_eq!(reread, second);
        assert_ne!(first["page"]["members"], second["page"]["members"]);
        assert_eq!(second["page"]["optimal_cardinality"], "1");
        // Keep the lazy page store open while preparing the next request.
        // Production activation happens before start_job clears that store;
        // manually releasing it here would hide a surviving asset lease.
        let after_minimum = arguments(&eligible, profile, false, false);
        complete(&mut bridge, &after_minimum, profile, false, false, &source);
        assert!(bridge.product_page_get("1", "1").is_err());

        // Use a real bounded job, not a fake executor or another profile's
        // relation. Pre-completion cancellation must end in Cancelled, drain
        // the worker, and permit a fresh profile/policy transition.
        let cancelled_arguments = arguments(&initial, profile, true, true);
        let job = start(&mut bridge, &cancelled_arguments, profile, true, true);
        assert!(bridge.ensure_no_running_job().is_err());
        bridge.cancel_job(job).unwrap();
        let cancelled = terminal(&mut bridge, job);
        assert_eq!(cancelled["event"], "cancelled", "{cancelled}");
        assert_eq!(cancelled["scope_released"], true);
        assert!(bridge.product_page_get("1", "1").is_err());
        let restart = arguments(&eligible, profile, false, false);
        assert_eq!(
            meaning(
                &complete(&mut bridge, &restart, profile, false, false, &source),
                false,
            )["keys"]
                .as_array()
                .unwrap()
                .len(),
            159,
        );

        for (product, _) in PRODUCTS {
            let error = run_native_accelerator_action(
                product,
                "download",
                profile,
                &AtomicBool::new(true),
                &mut |_, _| panic!("cancelled download must not use transport"),
            )
            .expect_err("already-cancelled native download must fail before reuse or transport");
            assert!(error.contains("cancelled"), "{error}");
        }
        eprintln!("{profile}: real Desktop policies, lazy minimum pages, cancellation/restart and aborted downloads agree");
    }
    assert_eq!(
        installed(),
        before,
        "functional jobs must not replace signed installed generations"
    );
}
