//! Offline validation of the real adapter's P7P4 diagnostic at an observed CPU width.
//! Consumes the saved private product-runner report; never starts a solver.
//! cts1/ctks1 are separate canonical-key hash envelopes. Artifact SHA-256 is
//! an encoded-file digest and cannot be compared with either FNV64 value.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;

const COUNT: u64 = 456_923;
const PATTERNS: u64 = 4_233_600;
const OUTPUT_CAP: u64 = 4 * 1024 * 1024;
const ARTIFACT_CAP: u64 = 24 * 1024 * 1024;
const CTS: &str = "cts1:98ebe8726537b29f";
const CTKS: &str = "ctks1:98ebe8726537b29f";
const ARTIFACT_BYTES: u64 = 8_046_188;
const ARTIFACT_SHA256: &str = "44a22fd221b291fdbc7758e6d935c8dda8c7d2aea2cc1d575ee05aa4f966d602";
const KEY_ALGORITHM: &str = "clearra-normalized-tiling-key-v1";
const HASH_ALGORITHM: &str = "clearra-normalized-tiling-set-fnv64-v1";

fn check(errors: &mut Vec<String>, condition: bool, message: impl Into<String>) {
    if !condition {
        errors.push(message.into());
    }
}
fn integer(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        let text = value.as_str()?;
        if text.is_empty()
            || text.len() > 1 && text.starts_with('0')
            || !text.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        text.parse().ok()
    })
}
fn flag(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| match value.as_str()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}
fn exposed(value: &Value) -> bool {
    !value.is_null()
        && !matches!(
            value.as_str(),
            Some("unknown" | "not-calculated" | "unavailable" | "none" | "")
        )
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn require_integer(errors: &mut Vec<String>, value: &Value, expected: u64, name: &str) {
    check(
        errors,
        integer(value) == Some(expected),
        format!("{name}: expected {expected}, got {value}"),
    );
}
fn require_flag(errors: &mut Vec<String>, value: &Value, expected: bool, name: &str) {
    check(
        errors,
        flag(value) == Some(expected),
        format!("{name}: expected {expected}, got {value}"),
    );
}
fn require_string(errors: &mut Vec<String>, value: &Value, expected: &str, name: &str) {
    check(
        errors,
        value.as_str() == Some(expected),
        format!("{name}: expected {expected:?}, got {value}"),
    );
}
fn hash_check(errors: &mut Vec<String>, summary: &Value, name: &str) -> Value {
    let actual = &summary[name];
    if !exposed(actual) {
        return json!({"field":name,"status":"not-exposed","value":actual});
    }
    let (expected, semantics) = match actual.as_str() {
        Some(value) if value.starts_with("cts1:") => {
            (CTS, "canonical normalized tiling set; exact cts1 golden")
        }
        Some(value) if value.starts_with("ctks1:") => {
            require_string(
                errors,
                &summary["normalized_solution_key_algorithm"],
                KEY_ALGORITHM,
                "normalized_solution_key_algorithm for ctks1 comparison",
            );
            (
                CTKS,
                "sorted canonical key strings; separate ctks1 envelope, same FNV64 key+NUL input",
            )
        }
        _ => {
            errors.push(format!("{name}: unsupported exposed hash {actual}"));
            return json!({"field":name,"status":"unsupported","value":actual});
        }
    };
    require_string(errors, actual, expected, name);
    json!({"field":name,"status":if actual == expected {"matched"} else {"mismatch"},
        "value":actual,"expected":expected,"semantics":semantics})
}

fn inspect_observation(
    errors: &mut Vec<String>,
    outer: &Value,
    returned_stderr_bytes: usize,
) -> Value {
    let observation = &outer["observation"];
    let initial_errors = errors.len();
    let node_version = &outer["nodeVersion"];
    require_string(errors, node_version, "22.23.2", "observation Node version");
    check(
        errors,
        observation.is_object(),
        "the diagnostic requires the real child observation object",
    );
    if !observation.is_object() {
        return json!({"status":"invalid","nodeVersion":node_version,"pipeBytes":null});
    }
    let moment = |name: &str| {
        observation[name]
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
    };
    for name in ["spawnMs", "firstStdoutMs", "closeMs", "wallTimeMs"] {
        check(
            errors,
            moment(name).is_some(),
            format!("observation.{name} must be a finite nonnegative monotonic duration"),
        );
    }
    let (spawn, first, close, wall) = (
        moment("spawnMs"),
        moment("firstStdoutMs"),
        moment("closeMs"),
        moment("wallTimeMs"),
    );
    if let (Some(spawn), Some(first), Some(close), Some(wall)) = (spawn, first, close, wall) {
        check(
            errors,
            spawn <= first && first <= close && close <= wall,
            "observation timestamps must satisfy spawn <= first stdout <= close <= wall",
        );
    }
    let pipe_stdout = observation["stdoutBytes"].as_u64();
    let pipe_stderr = observation["stderrBytes"].as_u64();
    check(
        errors,
        pipe_stdout.is_some_and(|bytes| bytes > 0),
        "observation.stdoutBytes must be a positive integer for this successful JSON result",
    );
    check(
        errors,
        pipe_stderr.is_some(),
        "observation.stderrBytes must be a nonnegative integer",
    );
    let pipe_total = pipe_stdout
        .zip(pipe_stderr)
        .and_then(|(left, right)| left.checked_add(right));
    check(
        errors,
        pipe_total.is_some_and(|bytes| bytes <= OUTPUT_CAP),
        "observed child stdout+stderr must remain within the 4 MiB success policy",
    );
    check(
        errors,
        pipe_stderr == Some(returned_stderr_bytes as u64),
        "observed child stderr bytes must match the unchanged returned CLI stderr",
    );
    require_integer(errors, &observation["exitCode"], 0, "observation.exitCode");
    check(
        errors,
        observation["exitCode"] == outer["exitCode"],
        "observed exit code must match the runner result",
    );
    check(
        errors,
        observation.get("signal").is_some_and(Value::is_null)
            && observation["signal"] == outer["signal"],
        "observed signal must be null and match the runner result",
    );
    let delta = |later: Option<f64>, earlier: Option<f64>| {
        later
            .zip(earlier)
            .and_then(|(later, earlier)| (later >= earlier).then_some(later - earlier))
    };
    json!({"status":if errors.len() == initial_errors {"validated"} else {"invalid"},
        "nodeVersion":node_version,"clock":"process.hrtime.bigint",
        "scope":"same baseline observation wrapper around execute(request), including cleanup; no timing equality asserted against Date.now stage durations",
        "pipeBytes":{"stdout":pipe_stdout,"stderr":pipe_stderr,"combined":pipe_total},
        "spawnToFirstStdoutMs":delta(first, spawn),"spawnToCloseMs":delta(close, spawn),
        "afterCloseMs":delta(wall, close),
        "stdoutComparison":"raw pipe stdout and returned stdout are different surfaces; no equality required"})
}

pub(super) fn validate(path: &str, source: &str, workers: u64) -> Result<Value, String> {
    if !hex(source, 40) {
        return Err("SOURCE_SHA must be an exact lowercase 40-digit commit".into());
    }
    if workers == 0 {
        return Err("diagnostic worker count must be positive".into());
    }
    let metadata = fs::metadata(path).map_err(|error| format!("report metadata: {error}"))?;
    if !metadata.is_file() || metadata.len() > 32 * 1024 * 1024 {
        return Err("expected one bounded JSON report, at most 32 MiB".into());
    }
    let bytes = fs::read(path).map_err(|error| format!("read report: {error}"))?;
    validate_bytes(&bytes, source, workers)
}

fn validate_bytes(bytes: &[u8], source: &str, workers: u64) -> Result<Value, String> {
    let outer: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("outer JSON: {error}"))?;
    let mut errors = Vec::new();
    require_string(
        &mut errors,
        &outer["schema"],
        "clearra.local-discord-runner.v1",
        "schema",
    );
    require_string(
        &mut errors,
        &outer["adapterSourceCommit"],
        source,
        "adapterSourceCommit",
    );
    require_flag(
        &mut errors,
        &outer["diagnosticOnly"],
        true,
        "diagnosticOnly",
    );
    require_integer(&mut errors, &outer["exitCode"], 0, "exitCode");
    check(
        &mut errors,
        outer.get("signal").is_some_and(Value::is_null),
        "signal must be present and null",
    );
    check(
        &mut errors,
        outer.get("failureCode").is_some_and(Value::is_null),
        "failureCode must be present and null",
    );
    check(
        &mut errors,
        outer["error"].is_null(),
        format!("runner error: {}", outer["error"]),
    );
    require_string(
        &mut errors,
        &outer["terminationReason"],
        "exit",
        "terminationReason",
    );
    let expected_args = json!([
        "pc",
        "--board-mask",
        "0",
        "--height",
        "4",
        "--pieces",
        "10",
        "--lines",
        "4",
        "--count",
        "unique",
        "--patterns",
        "P7P4",
        "--rule",
        "srs-plus",
        "--hold",
        "empty",
        "--queue-knowledge",
        "oracle"
    ]);
    check(
        &mut errors,
        outer["arguments"] == expected_args,
        "arguments differ from the exact P7P4 fixture",
    );
    let policy = &outer["policy"];
    for (name, expected) in [
        ("workers", workers),
        ("expectedVcpus", workers),
        ("maxOutputBytes", OUTPUT_CAP),
        ("maxArtifactBytes", ARTIFACT_CAP),
        ("deadlineMs", 14 * 60_000),
    ] {
        require_integer(
            &mut errors,
            &policy[name],
            expected,
            &format!("policy.{name}"),
        );
    }
    let stdout = outer["stdout"].as_str();
    let stderr = outer["stderr"].as_str();
    check(
        &mut errors,
        stdout.is_some(),
        "stdout must be a JSON string",
    );
    check(&mut errors, stderr.is_some(), "stderr must be a string");
    let stdout = stdout.unwrap_or("");
    let stderr = stderr.unwrap_or("");
    let inner: Value = match serde_json::from_str(stdout) {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("stdout JSON: {error}"));
            Value::Null
        }
    };
    for name in ["source_commit", "engine_build_id"] {
        require_string(
            &mut errors,
            &inner["runtime_identity"][name],
            source,
            &format!("runtime_identity.{name}"),
        );
    }
    require_integer(
        &mut errors,
        &inner["schema_version"],
        2,
        "stdout.schema_version",
    );
    require_string(&mut errors, &inner["kind"], "pc-scenario", "stdout.kind");
    check(
        &mut errors,
        inner["error"].is_null(),
        format!("CLI error: {}", inner["error"]),
    );
    let summary = &inner["summary"];
    for name in ["unique_solution_count", "normalized_unique_solution_count"] {
        require_integer(&mut errors, &summary[name], COUNT, name);
    }
    for name in [
        "actual_normalized_unique_solution_count",
        "solution_keys_materialized_count",
    ] {
        if exposed(&summary[name]) {
            require_integer(&mut errors, &summary[name], COUNT, name);
        }
    }
    for name in [
        "count_complete",
        "solution_count_calculated",
        "solution_set_materialized",
        "solution_keys_complete",
    ] {
        require_flag(&mut errors, &summary[name], true, name);
    }
    require_flag(
        &mut errors,
        &inner["resource_report"]["truncated"],
        false,
        "resource_report.truncated",
    );
    for name in ["resource_truncated", "backend_fallback_used"] {
        if exposed(&summary[name]) {
            require_flag(&mut errors, &summary[name], false, name);
        }
    }
    require_integer(
        &mut errors,
        &summary["workers_used"],
        workers,
        "workers_used",
    );
    if exposed(&summary["parallel_active_workers"]) {
        require_integer(
            &mut errors,
            &summary["parallel_active_workers"],
            workers,
            "parallel_active_workers",
        );
    }
    if exposed(&summary["cpu_parallel_execution"]) {
        require_flag(
            &mut errors,
            &summary["cpu_parallel_execution"],
            workers > 1,
            "cpu_parallel_execution",
        );
    }
    for (name, expected) in [
        ("rule_profile", "srs-plus"),
        ("queue_knowledge", "oracle"),
        ("normalized_solution_key_algorithm", KEY_ALGORITHM),
        ("normalized_solution_set_hash_algorithm", HASH_ALGORITHM),
    ] {
        if exposed(&summary[name]) {
            require_string(&mut errors, &summary[name], expected, name);
        }
    }
    let mut pattern_evidence = Vec::new();
    for name in [
        "total_possible_pattern_count",
        "coverage_pattern_count",
        "materialized_pattern_count",
    ] {
        if exposed(&summary[name]) {
            require_integer(&mut errors, &summary[name], PATTERNS, name);
            pattern_evidence.push(json!({"field":name,"value":summary[name]}));
        }
    }
    check(
        &mut errors,
        !pattern_evidence.is_empty(),
        "no public source-universe count was exposed",
    );
    let mut hash_evidence = Vec::new();
    for name in [
        "normalized_solution_set_hash",
        "actual_normalized_solution_set_hash",
    ] {
        hash_evidence.push(hash_check(&mut errors, summary, name));
    }
    if exposed(&summary["normalized_solution_set_hash"])
        && exposed(&summary["actual_normalized_solution_set_hash"])
    {
        check(
            &mut errors,
            summary["normalized_solution_set_hash"]
                == summary["actual_normalized_solution_set_hash"],
            "declared and actual normalized-set hash envelopes must agree exactly",
        );
    }
    let audit_hash = "solution_set_audit_normalized_output_identity_hash";
    if exposed(&summary[audit_hash])
        && flag(&summary["solution_set_audit_normalized_complete"]) == Some(true)
        && integer(&summary["solution_set_audit_normalized_output_count"]) == Some(COUNT)
    {
        hash_evidence.push(hash_check(&mut errors, summary, audit_hash));
    } else {
        hash_evidence.push(json!({"field":audit_hash,"status":"not-compared",
            "value":summary[audit_hash],"reason":"complete full-family audit authority not exposed"}));
    }
    check(
        &mut errors,
        hash_evidence
            .iter()
            .any(|entry| entry["status"] == "matched"),
        "at least one exposed complete canonical-set hash must match the P7P4 golden",
    );
    // Native file output intentionally does not put the complete key array in stdout.
    let solution_data = &inner["contract"]["solution_data"];
    check(
        &mut errors,
        solution_data["requested"].as_bool() == Some(false),
        "contract.solution_data.requested must be the typed boolean false",
    );
    require_string(
        &mut errors,
        &solution_data["status"],
        "not-requested",
        "contract.solution_data.status",
    );
    check(
        &mut errors,
        solution_data.get("reason").is_some_and(Value::is_null),
        "contract.solution_data.reason must be present and null",
    );
    for (name, expected) in [("solution_data_requested", false)] {
        if exposed(&summary[name]) {
            require_flag(&mut errors, &summary[name], expected, name);
        }
    }
    if exposed(&summary["solution_data_status"]) {
        require_string(
            &mut errors,
            &summary["solution_data_status"],
            "not-requested",
            "solution_data_status",
        );
    }
    check(
        &mut errors,
        inner["contract"].get("artifacts").is_none(),
        "native file output must not duplicate inline contract.artifacts",
    );
    check(
        &mut errors,
        inner.get("solution_artifact").is_none(),
        "the product runner must consume and remove its private CLI artifact receipt",
    );
    let artifact = &outer["artifact"];
    require_string(
        &mut errors,
        &artifact["contract"],
        "clearra.discord-solution-artifact.v1",
        "artifact.contract",
    );
    require_string(
        &mut errors,
        &artifact["filename"],
        "clearra-result.ctk3",
        "artifact.filename",
    );
    require_integer(
        &mut errors,
        &artifact["solutionCount"],
        COUNT,
        "artifact.solutionCount",
    );
    require_flag(
        &mut errors,
        &artifact["complete"],
        true,
        "artifact.complete",
    );
    check(
        &mut errors,
        integer(&artifact["byteLength"]).is_some_and(|count| count > 0 && count <= ARTIFACT_CAP),
        "artifact.byteLength must be positive and within 24 MiB",
    );
    require_integer(
        &mut errors,
        &artifact["byteLength"],
        ARTIFACT_BYTES,
        "artifact.byteLength golden",
    );
    require_string(
        &mut errors,
        &artifact["sha256"],
        ARTIFACT_SHA256,
        "artifact.sha256 exact encoded-file golden",
    );
    let stages = &outer["stageDurationsMs"];
    for stage in ["prepare", "execute", "artifact", "projection"] {
        check(
            &mut errors,
            stages[stage].as_u64().is_some(),
            format!("stageDurationsMs.{stage} must be a nonnegative integer"),
        );
    }
    let stage_total = stages.as_object().and_then(|map| {
        map.values()
            .try_fold(0u64, |total, value| total.checked_add(value.as_u64()?))
    });
    check(
        &mut errors,
        stage_total.is_some(),
        "stageDurationsMs must contain only bounded nonnegative integers",
    );
    let returned_bytes = stdout.len() + stderr.len();
    check(
        &mut errors,
        returned_bytes as u64 <= OUTPUT_CAP,
        "returned stdout+stderr exceed 4 MiB",
    );
    let observation_evidence = inspect_observation(&mut errors, &outer, stderr.len());
    Ok(json!({
        "schema":"clearra.local-discord-p7p4-validation.v1",
        "valid":errors.is_empty(), "errors":errors, "expectedSource":source,
        "inputReportSha256":format!("{:x}",Sha256::digest(&bytes)),
        "runtimeIdentity":inner["runtime_identity"], "arguments":outer["arguments"], "policy":policy,
        "golden":{"solutionCount":COUNT,"sourcePatterns":PATTERNS,"canonicalSetHash":CTS,
            "equivalentSortedKeyStringEnvelope":CTKS,"artifactBytes":ARTIFACT_BYTES,
            "artifactSha256":ARTIFACT_SHA256,"coverageParityQualified":false},
        "count":summary["normalized_unique_solution_count"],"countComplete":summary["count_complete"],
        "workersUsed":summary["workers_used"],"activeWorkers":summary["parallel_active_workers"],
        "patternEvidence":pattern_evidence,"hashEvidence":hash_evidence,
        "coverage":{"probabilityComplete":summary["probability_complete"],
            "coveredPatternCount":summary["covered_pattern_count"],"coverageProbability":summary["coverage_probability"],
            "comparison":"reported only; CountUnique golden does not qualify probability/bitmap parity"},
        "solutionData":{"contract":solution_data,
            "optionalSummaryRequested":summary["solution_data_requested"],
            "optionalSummaryStatus":summary["solution_data_status"]},
        "artifact":artifact,
        "artifactValidation":"production runner checked file identity, CRC32 and CTK3 page count and hashed the exact artifact bytes; this metadata-only validator requires the recorded byte length and SHA-256 to match the complete baseline golden",
        "stageDurationsMs":stages,"stageDurationSumMs":stage_total,
        "observation":outer["observation"],"observationValidation":observation_evidence,
        "bytes":{"savedReport":bytes.len(),"returnedStdoutUtf8":stdout.len(),"returnedStderrUtf8":stderr.len(),
            "returnedCombinedUtf8":returned_bytes,"artifact":artifact["byteLength"],
            "originalChildWireBytes":observation_evidence["pipeBytes"],
            "note":"runner consumes its private artifact receipt and reserializes stdout; returned bytes are not the original child pipe byte count"},
        "returnedStdoutSha256":format!("{:x}",Sha256::digest(stdout.as_bytes())),
        "scope":"offline diagnostic on the recorded CI CPU width; not Cloud Run 8-vCPU performance or deployment evidence; no reconstructed bitmap or independent artifact-content rehash"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SOURCE: &str = "1111111111111111111111111111111111111111";

    fn successful_report() -> Value {
        let stdout = serde_json::to_string(&json!({
            "runtime_identity":{"source_commit":TEST_SOURCE,"engine_build_id":TEST_SOURCE},
            "schema_version":2,"kind":"pc-scenario",
            "summary":{
                "unique_solution_count":COUNT,"normalized_unique_solution_count":COUNT,
                "count_complete":true,"solution_count_calculated":true,
                "solution_set_materialized":true,"solution_keys_complete":true,
                "workers_used":2,"coverage_pattern_count":PATTERNS,
                "normalized_solution_set_hash":CTS,
                "normalized_solution_key_algorithm":KEY_ALGORITHM,
                "normalized_solution_set_hash_algorithm":HASH_ALGORITHM
            },
            "resource_report":{"truncated":false},
            "contract":{"solution_data":{"requested":false,"status":"not-requested","reason":null}}
        }))
        .unwrap();
        json!({
            "schema":"clearra.local-discord-runner.v1","adapterSourceCommit":TEST_SOURCE,
            "diagnosticOnly":true,"nodeVersion":"22.23.2",
            "exitCode":0,"signal":null,"failureCode":null,"terminationReason":"exit",
            "arguments":["pc","--board-mask","0","--height","4","--pieces","10",
                "--lines","4","--count","unique","--patterns","P7P4","--rule","srs-plus",
                "--hold","empty","--queue-knowledge","oracle"],
            "policy":{"workers":2,"expectedVcpus":2,"maxOutputBytes":OUTPUT_CAP,
                "maxArtifactBytes":ARTIFACT_CAP,"deadlineMs":14*60_000},
            "observation":{"spawnMs":1,"firstStdoutMs":10,"closeMs":11,"wallTimeMs":14,
                "stdoutBytes":stdout.len(),"stderrBytes":0,"exitCode":0,"signal":null},
            "stdout":stdout,"stderr":"",
            "artifact":{"contract":"clearra.discord-solution-artifact.v1",
                "filename":"clearra-result.ctk3","solutionCount":COUNT,"complete":true,
                "byteLength":ARTIFACT_BYTES,"sha256":ARTIFACT_SHA256},
            "stageDurationsMs":{"prepare":1,"execute":10,"artifact":2,"projection":1}
        })
    }

    fn validate_report(report: &Value) -> Value {
        validate_bytes(&serde_json::to_vec(report).unwrap(), TEST_SOURCE, 2).unwrap()
    }

    #[test]
    fn complete_observed_report_matches_both_set_and_encoded_artifact_golden() {
        let validation = validate_report(&successful_report());
        assert_eq!(validation["valid"], true, "{}", validation["errors"]);
        assert_eq!(validation["observationValidation"]["status"], "validated");
        assert_eq!(validation["golden"]["artifactSha256"], ARTIFACT_SHA256);
        assert_eq!(validation["golden"]["coverageParityQualified"], false);
    }

    #[test]
    fn diagnostic_identity_and_observation_are_mandatory() {
        for field in ["adapterSourceCommit", "nodeVersion", "observation"] {
            let mut report = successful_report();
            report.as_object_mut().unwrap().remove(field);
            assert_eq!(validate_report(&report)["valid"], false, "{field}");
        }
        let mut report = successful_report();
        report["nodeVersion"] = json!("24.0.0");
        assert_eq!(validate_report(&report)["valid"], false);
        report.as_object_mut().unwrap().remove("nodeVersion");
        report["baseline"] = json!({"nodeVersion":"22.23.2"});
        assert_eq!(validate_report(&report)["valid"], false);
    }

    #[test]
    fn missing_or_inconsistent_child_timing_output_and_exit_are_rejected() {
        for field in [
            "spawnMs",
            "firstStdoutMs",
            "closeMs",
            "wallTimeMs",
            "stdoutBytes",
            "stderrBytes",
            "exitCode",
            "signal",
        ] {
            let mut report = successful_report();
            report["observation"].as_object_mut().unwrap().remove(field);
            assert_eq!(validate_report(&report)["valid"], false, "missing {field}");
        }
        for (field, invalid) in [
            ("spawnMs", json!(-1)),
            ("firstStdoutMs", json!(12)),
            ("wallTimeMs", json!(10)),
            ("stdoutBytes", json!(OUTPUT_CAP + 1)),
            ("stderrBytes", json!(1)),
            ("exitCode", json!(1)),
            ("signal", json!("SIGTERM")),
        ] {
            let mut report = successful_report();
            report["observation"][field] = invalid;
            assert_eq!(validate_report(&report)["valid"], false, "invalid {field}");
        }
    }

    #[test]
    fn correct_counts_cannot_replace_canonical_or_encoded_artifact_hashes() {
        for (field, invalid) in [
            ("sha256", json!("0".repeat(64))),
            ("byteLength", json!(ARTIFACT_BYTES - 1)),
        ] {
            let mut report = successful_report();
            report["artifact"][field] = invalid;
            assert_eq!(validate_report(&report)["valid"], false, "{field}");
        }
        for invalid in [Value::Null, json!("cts1:0000000000000000")] {
            let mut report = successful_report();
            let mut stdout: Value =
                serde_json::from_str(report["stdout"].as_str().unwrap()).unwrap();
            stdout["summary"]["normalized_solution_set_hash"] = invalid;
            report["stdout"] = json!(serde_json::to_string(&stdout).unwrap());
            assert_eq!(validate_report(&report)["valid"], false);
        }
    }
}
