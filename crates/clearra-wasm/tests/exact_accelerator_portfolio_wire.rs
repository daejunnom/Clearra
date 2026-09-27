//! SRP rationale: bind real minimum portfolios to lazy outer pages and complete
//! native copy artifacts across the browser's production JSON boundary. This
//! is a small functional smoke, not timing, dataset qualification or a benchmark.

use clearra_app::{decode_ctk3_exact, PortfolioEnumerationStop};
use clearra_host_contract::{AppStatus, HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES};
use clearra_wasm::{
    serialize_coverage_portfolio_page, serialize_distributed_final_events,
    CoveragePortfolioPageStore, ProductPageSourceOwner, WasmCommandRuntime, WasmHostCapabilities,
};
use serde_json::{json, Value};

const FIXTURE_LEAF: &str = "v081-product-page-smoke";
const FIXTURE_FILE: &str = "portfolio-wire-smoke.json";
const SLICE_LIMIT: usize = 128;

fn command(legal: bool, conditioned: bool, build: bool) -> String {
    let request = if build {
        // Real mirrored candidates; both are mandatory, so Copy all must
        // retain the two-member portfolio rather than one representative.
        "clearra build pinned-minimals --base-mask 0 --target-mask 0xf --height 4 \
         --queue I --no-hold --objective min-cover --queue-knowledge oracle \
         --required-format ctk3 --required-document "
            .to_owned()
            + &clearra_app::encode_ctk3_compact(&clearra_app::Ctk3Document::new(
                10,
                [0xf_u64, 0x3c0]
                    .into_iter()
                    .map(|mask| {
                        clearra_app::Ctk3Page::new(
                            1,
                            (0..10)
                                .map(|x| {
                                    if mask & (1 << x) == 0 {
                                        clearra_app::Ctk3Color::Empty
                                    } else {
                                        clearra_app::Ctk3Color::Piece(clearra_app::Ctk3Piece::I)
                                    }
                                })
                                .collect(),
                        )
                    })
                    .collect(),
            ))
            .unwrap()
    } else {
        // The two I pieces occupy the same four columns in both rows;
        // three O pieces fill the rest. Starts 0,2,4,6 give four exact ties.
        "clearra pc minimals --lines 2 --board-mask 0 --height 2 --pieces 5 \
         --queue IIOOO --no-hold"
            .to_owned()
    };
    format!(
        "{request} --backend cpu --workers 1 {} {}",
        if legal {
            "--legal-board"
        } else {
            "--no-legal-board"
        },
        if conditioned {
            "--conditioned-reachability"
        } else {
            "--no-conditioned-reachability"
        }
    )
}

fn snapshot(store: &CoveragePortfolioPageStore, outer: usize, kind: &str) -> Value {
    let wire: Value =
        serde_json::from_str(&serialize_coverage_portfolio_page(store, outer, 1).unwrap()).unwrap();
    let artifact = store
        .bounded_solution_set_artifact_payload(outer, kind, HOST_SOLUTION_SET_ARTIFACT_MAX_BYTES)
        .expect("whole selected portfolio must be exportable");
    assert_eq!(artifact.selection_id(), outer.to_string());
    assert_eq!(artifact.selection_kind(), "portfolio-alternative");
    assert_eq!(
        artifact.page_source_identity_sha256(),
        Some(store.source().set_identity_sha256())
    );
    let members = store.member_page(outer, 1).unwrap();
    assert_eq!(artifact.solution_count() as usize, members.members().len());
    let ctk = artifact
        .formats()
        .iter()
        .find(|format| format.format() == "ctk3")
        .unwrap();
    assert!(ctk.available());
    let decoded = decode_ctk3_exact(ctk.document().unwrap()).unwrap();
    assert_eq!(decoded.pages.len(), artifact.solution_count() as usize);
    for format in artifact.formats() {
        assert!(
            format.available(),
            "small real portfolio must support both formats"
        );
    }
    json!({ "wire": wire, "artifact": artifact })
}

fn run_case(legal: bool, conditioned: bool, build: bool) -> Value {
    let runtime = WasmCommandRuntime::default()
        .with_host_capabilities(WasmHostCapabilities::new(12, false, false));
    let execution = runtime
        .run_command_text(&command(legal, conditioned, build))
        .unwrap();
    assert_eq!(execution.app_response().status(), AppStatus::Success);
    let owner = execution.product_page_source_owner().unwrap();
    let ProductPageSourceOwner::CoveragePortfolio(set) = owner else {
        panic!("minimum product must retain its canonical page source");
    };
    assert_eq!(set.candidates().len(), if build { 2 } else { 4 });
    assert_eq!(set.optimal_cardinality(), if build { 2 } else { 1 });
    assert_eq!(set.known_alternative_count_decimal(), "1");
    if !build {
        assert!(
            !set.enumeration_complete(),
            "first response must not eagerly enumerate ties"
        );
        assert_eq!(set.total_alternative_count_decimal(), None);
        assert_eq!(set.canonical_page().portfolio().candidate_ids(), &[1]);
    }
    let candidate_keys: Vec<_> = set
        .candidates()
        .iter()
        .map(|candidate| candidate.normalized_key())
        .collect();
    assert!(candidate_keys.windows(2).all(|pair| pair[0] < pair[1]));
    let final_wire: Value =
        serde_json::from_str(&serialize_distributed_final_events(73, &execution).unwrap()).unwrap();
    let final_response = final_wire
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["event"] == "final_response")
        .unwrap()["response"]
        .clone();
    let kind = execution
        .app_response()
        .product_result_payload()
        .unwrap()
        .result_kind();
    let mut store = CoveragePortfolioPageStore::new(set.clone()).unwrap();
    let first = snapshot(&store, 1, kind);
    let mut pages = vec![first.clone()];
    let mut sealed = false;
    for _ in 0..SLICE_LIMIT {
        let advance = store.next_page(1, &mut || false).unwrap();
        if let Some(page) = advance.page() {
            let index: usize = page.alternative_index_decimal().parse().unwrap();
            assert_eq!(index, pages.len() + 1);
            pages.push(snapshot(&store, index, kind));
        }
        match advance.stop() {
            PortfolioEnumerationStop::Sealed => {
                sealed = true;
                break;
            }
            PortfolioEnumerationStop::Cancelled => panic!("not cancelled"),
            PortfolioEnumerationStop::PageFull | PortfolioEnumerationStop::WorkBudgetExhausted => {}
        }
    }
    assert!(
        sealed,
        "small tie fixture must seal within a bounded number of slices"
    );
    assert_eq!(pages.len(), if build { 1 } else { 4 });
    assert!(store.enumeration_complete());
    if !build {
        assert!(
            store.page(1).is_none(),
            "fourth page must evict the first of three retained pages"
        );
    }
    let high_water = store.known_alternative_count_decimal();
    let before_cancel = store.loaded_page_count();
    let cancelled = store
        .load_page_by_alternative_index_slice("1", 1, &mut || true)
        .unwrap();
    if !build {
        assert_eq!(cancelled.state().as_str(), "cancelled");
        assert_eq!(store.loaded_page_count(), before_cancel);
        assert!(store.page(1).is_none());
    }
    let mut reloaded = false;
    for _ in 0..SLICE_LIMIT {
        let advance = store
            .load_page_by_alternative_index_slice("1", 1, &mut || false)
            .unwrap();
        if advance.retained_slot().is_some() {
            reloaded = true;
            break;
        }
        assert_eq!(advance.state().as_str(), "work-budget-exhausted");
    }
    assert!(reloaded, "evicted page replay must terminate");
    assert_eq!(store.known_alternative_count_decimal(), high_water);
    let restored = snapshot(&store, 1, kind);
    assert_eq!(
        restored, first,
        "backtracking must recover the original exact page and copy bytes"
    );
    assert_eq!(store.known_alternative_count_decimal(), high_water);
    json!({ "legal": legal, "conditioned": conditioned, "build": build,
        "candidate_keys": candidate_keys, "final_response": final_response,
        "pages": pages, "restored": restored })
}

fn capture() -> Value {
    let mut cases = Vec::new();
    for build in [false, true] {
        let mut expected: Option<Vec<Value>> = None;
        for (legal, conditioned) in [(false, false), (false, true), (true, false), (true, true)] {
            let case = run_case(legal, conditioned, build);
            let meanings: Vec<_> = case["pages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|page| {
                    json!({ "index": page["wire"]["page"]["alternative_index"],
                    "members": page["wire"]["page"]["members"],
                    "formats": page["artifact"]["formats"] })
                })
                .collect();
            if let Some(expected) = &expected {
                assert_eq!(&meanings, expected);
            } else {
                expected = Some(meanings);
            }
            cases.push(case);
        }
    }
    json!({ "schema_id": "clearra.v081.real-portfolio-wire-smoke.v1", "cases": cases })
}

#[test]
fn real_minimum_ties_preserve_lazy_pages_cancellation_and_whole_selected_copy() {
    let _ = capture();
}

#[test]
#[ignore = "writes only to the explicit managed smoke root for the real UI consumer"]
fn write_real_portfolio_wire_for_ui() {
    let configured = std::path::PathBuf::from(
        std::env::var_os("CLEARRA_REAL_PORTFOLIO_SMOKE_DIR").expect("explicit managed smoke root"),
    );
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    let root = configured.canonicalize().unwrap();
    assert_eq!(
        root,
        workspace
            .join("_local")
            .join("artifacts")
            .join(FIXTURE_LEAF)
    );
    let fixture = capture();
    std::fs::write(
        root.join(FIXTURE_FILE),
        serde_json::to_vec_pretty(&fixture).unwrap(),
    )
    .unwrap();
}
