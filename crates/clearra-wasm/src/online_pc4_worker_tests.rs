//! Synthetic, in-memory tests of the public worker I/O boundary. No upstream
//! downloads, filesystem access, network or ordinary-search fallback.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const COMMAND: &str = "clearra pc --board-mask 0xffbfeffbfe --height 4 --pieces 1 \
    --lines 4 --queue I --no-hold --rule jstris-180 --tablebase";
const PATHS: [&str; 3] = [
    "field_hash_to_id.v1.bin",
    "graph_offsets.u32.bin",
    "graph.bin",
];

struct Fixture {
    generation: Value,
    files: [Vec<u8>; 3],
}

impl Fixture {
    fn new() -> Self {
        // Hydra reverses bits within each ten-cell row; it is not column-major.
        let initial_hash =
            clearra_pc4_tablebase::clearra_board64_mask_to_hydra_field_hash_v1(0xff_bfef_fbfe)
                .unwrap();
        assert_eq!(initial_hash, 0x7f_dff7_fdff);
        let hashes = [0_u64, initial_hash, 0xff_ffff_ffff];
        let header = |magic: &[u8; 8]| {
            let mut bytes = magic.to_vec();
            bytes.extend_from_slice(&1_u32.to_le_bytes());
            bytes.extend_from_slice(&3_u32.to_le_bytes());
            bytes
        };
        let mut fields = header(b"FHIDIDX1");
        let mut graph = Vec::new();
        let mut offsets = header(b"GOFFIDX1");
        for (id, hash) in hashes.into_iter().enumerate() {
            fields.extend_from_slice(&hash.to_le_bytes()[..5]);
            fields.extend_from_slice(&(id as u32).to_le_bytes()[..3]);
            offsets.extend_from_slice(&(graph.len() as u32).to_le_bytes());
            graph.extend_from_slice(&hash.to_be_bytes()[3..]);
            // The middle field has a single vertical-I completion. Piece order
            // is IJLOSTZ; records for empty/terminal fields have no outgoing edge.
            if id == 1 {
                graph.extend_from_slice(&[1, 2, 0, 0]);
            } else {
                graph.push(0);
            }
            graph.extend_from_slice(&[0; 6]);
        }
        offsets.extend_from_slice(&(graph.len() as u32).to_le_bytes());
        let files = [fields, offsets, graph];
        let artifact = |i: usize| {
            json!({ "path": PATHS[i], "byte_length": files[i].len(),
            "content_identity": format!("sha256:{:x}", Sha256::digest(&files[i])) })
        };
        let profiles = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"].map(|profile| {
            if profile != "jstris-180" { return json!({ "profile": profile, "status": "unavailable" }); }
            json!({ "profile": profile, "status": "ready", "upstream_complete": true,
                "reader_contract": "hydra-jstris-180-complete-graph-v1", "field_count": 3,
                "terminal_id": 2, "target_width": 3, "target_lines": [4],
                "artifacts": { "fields": artifact(0), "offsets": artifact(1), "graph": artifact(2) },
                "evidence": [{ "id": 0, "hash": 0, "start": 0, "end": 12 },
                    { "id": 2, "hash": hashes[2], "start": 27, "end": 39 }] })
        });
        Self {
            files,
            generation: json!({ "schema": "clearra.pc4.host-generation.v1",
            "repository": "muse918/tetris-4lpc-mdp-vstar-policy", "revision": "a".repeat(40), "profiles": profiles }),
        }
    }

    fn start(&self) -> (WasmWorkerJobRuntime, WasmWorkerJobId) {
        let mut runtime = WasmWorkerJobRuntime::default();
        runtime
            .configure_online_pc4(&self.generation.to_string())
            .unwrap();
        let id = runtime.start_job(COMMAND).unwrap();
        (runtime, id)
    }

    fn pending(runtime: &mut WasmWorkerJobRuntime, id: WasmWorkerJobId) -> Value {
        for _ in 0..100 {
            let state = runtime.advance_job(id, 256).unwrap();
            let range: Value = serde_json::from_str(&runtime.online_pc4_pending_json(id)).unwrap();
            if !range.is_null() {
                return range;
            }
            assert!(
                !state.is_terminal(),
                "{state:?}: {:?}",
                runtime.drain_events(id)
            );
        }
        panic!("the I/O boundary never requested bytes");
    }

    fn response(&self, range: &Value, local: bool) -> Value {
        let i = PATHS
            .iter()
            .position(|path| range["artifact"]["path"] == *path)
            .unwrap();
        let start = range["offset"].as_u64().unwrap() as usize;
        let length = range["length"].as_u64().unwrap() as usize;
        let mut response = json!({ "lookup_session": range["lookup_session"], "request_id": range["request_id"],
            "bytes": self.files[i][start..start + length] });
        if local {
            response["source"] = json!("verified-local-file");
        } else {
            response["status"] = json!(206);
            response["content_range"] = json!(format!(
                "bytes {}-{}/{}",
                start,
                start + length - 1,
                self.files[i].len()
            ));
        }
        response
    }
}

#[test]
fn online_pc4_worker_local_and_http_adapters_complete_the_same_one_piece_search() {
    let fixture = Fixture::new();
    let mut responses = Vec::new();
    for local in [true, false] {
        let (mut runtime, id) = fixture.start();
        let mut requests = 0;
        let mut completed = false;
        for _ in 0..2_000 {
            let state = runtime.advance_job(id, 256).unwrap();
            let range: Value = serde_json::from_str(&runtime.online_pc4_pending_json(id)).unwrap();
            if !range.is_null() {
                let batch = range["batch"]
                    .as_array()
                    .expect("independent demand envelope");
                assert!(!batch.is_empty() && batch.len() <= 8);
                for field in [
                    "lookup_session",
                    "request_id",
                    "profile",
                    "offset",
                    "length",
                    "artifact",
                ] {
                    assert_eq!(
                        range[field], batch[0][field],
                        "scalar host remains compatible"
                    );
                }
                assert!(range["can_advance"].is_boolean());
                requests += 1;
                runtime
                    .online_pc4_admit_json(id, &fixture.response(&range, local).to_string())
                    .unwrap();
            }
            if state.is_terminal() {
                let events = runtime.drain_events(id);
                assert_eq!(state, WasmWorkerAdvanceStatus::Completed, "{events:?}");
                let (response, report) = events
                    .into_iter()
                    .find_map(|event| match event {
                        WasmWorkerJobEvent::FinalResponse {
                            response,
                            search_report,
                            ..
                        } => Some((response, search_report)),
                        _ => None,
                    })
                    .expect("terminal product response");
                assert_eq!(response.status(), AppStatus::Success);
                let report = report.expect("GUI-visible solution report");
                assert!(report.solution_count_calculated);
                assert_eq!(report.unique_solution_count, 1);
                responses.push(response);
                completed = true;
                break;
            }
        }
        assert!(
            completed && requests > 0,
            "must complete through actual host admission, not fallback"
        );
    }
    assert_eq!(
        responses[0], responses[1],
        "transport must not alter the solution set"
    );
}

#[test]
fn online_pc4_worker_local_source_rejects_http_headers_stale_ids_and_late_cancelled_bytes() {
    let fixture = Fixture::new();
    let (mut runtime, id) = fixture.start();
    let range = Fixture::pending(&mut runtime, id);
    let valid = fixture.response(&range, true);
    for (field, value, reason) in [
        ("status", json!(206), "pc4_local_response_not_http"),
        (
            "content_range",
            json!("bytes 0-15/40"),
            "pc4_local_response_not_http",
        ),
        (
            "source",
            json!("another-profile"),
            "pc4_online_source_invalid",
        ),
        (
            "request_id",
            json!(u64::MAX),
            "pc4_online_response_id_mismatch",
        ),
        (
            "lookup_session",
            json!(u64::MAX),
            "pc4_online_response_id_mismatch",
        ),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        assert_eq!(
            runtime
                .online_pc4_admit_json(id, &invalid.to_string())
                .unwrap_err()
                .code(),
            reason
        );
        assert_eq!(
            serde_json::from_str::<Value>(&runtime.online_pc4_pending_json(id)).unwrap(),
            range
        );
    }
    assert_eq!(
        runtime.configure_online_pc4("null").unwrap_err().code(),
        "pc4_online_job_active"
    );
    runtime.cancel_job(id).unwrap();
    assert_eq!(runtime.online_pc4_pending_json(id), "null");
    assert_eq!(
        runtime
            .online_pc4_admit_json(id, &valid.to_string())
            .unwrap_err()
            .code(),
        "pc4_online_job_missing"
    );
    runtime.configure_online_pc4("null").unwrap();
}
