//! Independent offline exact-family evidence for one immutable PC4 generation.
//!
//! This owner deliberately does not read graph adjacency. It binds Clearra's
//! ordinary exact CPU solver result to the same repository/revision/profile
//! whose graph is being qualified, so a later differential proof can compare
//! the tablebase materializer against an independently produced family.

use super::{read_json, validate_receipt_identity, with_identity, write_json_atomic, Dataset};
use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    pc::pc_target::PcTarget,
    solution::normalized_tiling_solution::normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
};
use clearra_core_executor::WasmCpuSearchBackend;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::jstris_180;
use serde_json::json;
use std::{path::Path, time::Instant};

pub(crate) const SCHEMA: &str = "clearra.pc4.offline-exact-result-family.v1";
pub(crate) const INPUT_IDENTITY: &str = "empty-board-4l-jstris-180-standard-7-bag-hold-empty-v1";

pub(crate) struct ExactFamily {
    pub(crate) identities: Vec<StandardBoard64TilingIdentity>,
    pub(crate) normalized_hash: String,
    pub(crate) normalized_hash_algorithm: String,
}

impl ExactFamily {
    pub(crate) fn count(&self) -> usize {
        self.identities.len()
    }
}

pub(crate) fn prove(
    dataset: &Dataset,
    workers: usize,
    expected_count: usize,
    output: &Path,
) -> Result<(), String> {
    if dataset.profile != "jstris-180" || dataset.kick_profile.as_str() != "jstris-180" {
        return Err("offline family proof currently requires jstris-180".to_owned());
    }
    if workers == 0 || workers > 64 {
        return Err("offline family proof worker count outside 1..=64".to_owned());
    }
    if expected_count == 0 {
        return Err("offline family proof expected count must be nonzero".to_owned());
    }

    if output.exists() {
        let existing = read_json(output, 16 * 1024 * 1024)?;
        validate_receipt_identity(&existing)?;
        if existing["schema"] != SCHEMA
            || existing["repository"].as_str() != Some(&dataset.repository)
            || existing["revision"].as_str() != Some(&dataset.revision)
            || existing["profile"].as_str() != Some(&dataset.profile)
            || existing["artifacts"] != dataset.public_artifacts()
            || existing["input_identity"] != INPUT_IDENTITY
            || existing["expected_unique_solution_count"].as_u64()
                != u64::try_from(expected_count).ok()
        {
            return Err("existing offline family proof does not match current inputs".to_owned());
        }
        println!(
            "pc4_offline_family_proof=already-complete solutions={} receipt={}",
            existing["unique_solution_count"].as_u64().unwrap_or(0),
            existing["receipt_identity"].as_str().unwrap_or("invalid")
        );
        return Ok(());
    }

    let started = Instant::now();
    let exact = execute(workers)?;
    let elapsed_ms = started.elapsed().as_millis();
    let count = exact.count();
    if count != expected_count {
        return Err(format!(
            "offline exact family count {count} differs from expected {expected_count}"
        ));
    }

    let core = json!({
        "schema": SCHEMA,
        "authority": "non-target-qualification-evidence",
        "qualification_status": "offline-exact-family-complete",
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "kick_profile": dataset.kick_profile.as_str(),
        "reader_contract": dataset.reader_contract,
        "artifacts": dataset.public_artifacts(),
        "input_identity": INPUT_IDENTITY,
        "initial_board_mask": 0,
        "target_lines": 4,
        "queue": "P7P4",
        "hold": "enabled-empty",
        "objective": "unique",
        "expected_unique_solution_count": expected_count,
        "unique_solution_count": count,
        "normalized_solution_set_hash_algorithm": exact.normalized_hash_algorithm,
        "normalized_solution_set_hash": exact.normalized_hash,
        "identity_order": "strict-canonical-ascending",
        "count_complete": true,
        "probability_complete": true,
        "resource_truncated": false,
        "offline_exact_parity_identity": serde_json::Value::Null,
    });
    let receipt = with_identity(core)?;
    write_json_atomic(output, &receipt)?;
    println!(
        "pc4_offline_family_proof=passed solutions={} hash={} workers={} elapsed_ms={} receipt={}",
        count,
        receipt["normalized_solution_set_hash"]
            .as_str()
            .unwrap_or("invalid"),
        workers,
        elapsed_ms,
        receipt["receipt_identity"].as_str().unwrap_or("invalid")
    );
    Ok(())
}

pub(crate) fn execute(workers: usize) -> Result<ExactFamily, String> {
    if workers == 0 || workers > 64 {
        return Err("offline family proof worker count outside 1..=64".to_owned());
    }
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(workers)
        .with_cpu_warmup(true);
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_queue(PcQueueInput::standard_7_bag())
        .with_hold_policy(PcHoldPolicy::EnabledEmpty)
        .with_objective(ObjectivePolicy::unique())
        .with_rule(jstris_180())
        .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_opening_pc(&query)
        .map_err(|error| format!("offline family problem compile failed: {error:?}"))?;
    let control = ExecutionControl::new(ExecutionCancellationToken::new());
    let result = WasmCpuSearchBackend::execute_with_control(&problem, &control)
        .map_err(|error| format!("offline exact family search failed: {error:?}"))?;

    let count = result
        .usize_field("normalized_unique_solution_count")
        .ok_or("offline result lacks normalized unique solution count")?;
    if result.field("count_complete") != Some("true")
        || result.field("probability_complete") != Some("true")
        || result.field("resource_truncated") != Some("false")
    {
        return Err("offline exact family result is incomplete".to_owned());
    }
    let identities = if result.normalized_solution_identities().len() == count {
        result.normalized_solution_identities().to_vec()
    } else {
        result
            .tiling_solution_page_store()
            .ok_or("offline result lacks exact family storage")?
            .page_identities(0, count)
            .map_err(str::to_owned)?
    };
    if identities.len() != count || identities.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("offline exact family identities are not one strict canonical set".to_owned());
    }
    let independently_hashed =
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(&identities);
    let reported_hash = result
        .field("normalized_solution_set_hash")
        .ok_or("offline result lacks normalized solution set hash")?;
    if reported_hash != independently_hashed
        || result.field("actual_normalized_solution_set_hash") != Some(reported_hash)
    {
        return Err("offline exact family normalized hash mismatch".to_owned());
    }

    Ok(ExactFamily {
        identities,
        normalized_hash: reported_hash.to_owned(),
        normalized_hash_algorithm: result
            .field("normalized_solution_set_hash_algorithm")
            .ok_or("offline result lacks normalized hash algorithm")?
            .to_owned(),
    })
}
