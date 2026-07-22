use std::{fs, time::Instant};

use serde::Serialize;

use crate::{
    args::ArtifactArgs,
    artifact::{self, ArtifactOutcome},
};

#[derive(Serialize)]
struct ArtifactBatchSummary {
    schema_version: u32,
    process_lifetime_gpu_context_reuse_enabled: bool,
    cases: Vec<ArtifactBatchCase>,
}

#[derive(Serialize)]
struct ArtifactBatchCase {
    scenario: &'static str,
    elapsed_ns: u64,
    output_directory: String,
    solution_count: usize,
    covered_pattern_count: usize,
    pattern_count: usize,
}

pub fn run_and_write(args: ArtifactArgs) -> Result<Vec<ArtifactOutcome>, String> {
    if args.scenarios.len() == 1 {
        return artifact::run_and_write(args).map(|outcome| vec![outcome]);
    }

    let output_root = args.output_directory.clone();
    fs::create_dir_all(&output_root).map_err(|error| {
        format!(
            "failed to create artifact batch directory {}: {error}",
            output_root.display()
        )
    })?;
    let mut outcomes = Vec::with_capacity(args.scenarios.len());
    let mut cases = Vec::with_capacity(args.scenarios.len());
    for scenario in args.scenarios.iter().copied() {
        let mut case_args = args.clone();
        case_args.scenario = scenario;
        case_args.scenarios = vec![scenario];
        case_args.output_directory = output_root.join(scenario.as_str());
        let started_at = Instant::now();
        let outcome = artifact::run_and_write(case_args)?;
        cases.push(ArtifactBatchCase {
            scenario: scenario.as_str(),
            elapsed_ns: u64::try_from(started_at.elapsed().as_nanos()).unwrap_or(u64::MAX),
            output_directory: outcome.output_directory.display().to_string(),
            solution_count: outcome.solution_count,
            covered_pattern_count: outcome.covered_pattern_count,
            pattern_count: outcome.pattern_count,
        });
        outcomes.push(outcome);
    }

    let summary = ArtifactBatchSummary {
        schema_version: 1,
        process_lifetime_gpu_context_reuse_enabled: true,
        cases,
    };
    let bytes = serde_json::to_vec_pretty(&summary)
        .map_err(|error| format!("failed to serialize artifact batch summary: {error}"))?;
    fs::write(output_root.join("batch-summary.json"), bytes)
        .map_err(|error| format!("failed to write artifact batch summary: {error}"))?;
    let mut timings = String::new();
    for case in &summary.cases {
        timings.push_str(case.scenario);
        timings.push('\t');
        timings.push_str(&case.elapsed_ns.to_string());
        timings.push('\n');
    }
    fs::write(output_root.join("batch-times.tsv"), timings)
        .map_err(|error| format!("failed to write artifact batch timings: {error}"))?;
    Ok(outcomes)
}
