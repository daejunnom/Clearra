mod args;
mod artifact;
mod artifact_batch;
mod gpu_inventory;
mod scenario;
mod sfinder_reference;

fn main() {
    let result = args::ArtifactArgs::parse(std::env::args().skip(1)).and_then(|args| {
        if args.list_gpu_devices {
            gpu_inventory::print_gpu_inventory()?;
            return Ok(None);
        }
        if args.probe_gpu_warmup {
            gpu_inventory::print_gpu_warmup_probe(args.gpu_device)?;
            return Ok(None);
        }
        if args.prewarm_gpu {
            gpu_inventory::prewarm_gpu(args.gpu_device.clone())?;
        }
        artifact_batch::run_and_write(args).map(Some)
    });
    match result {
        Ok(Some(outcomes)) => {
            for outcome in outcomes {
                println!(
                    "artifact complete | output={} | solutions={} | covered_patterns={}/{} | probability={}",
                    outcome.output_directory.display(),
                    outcome.solution_count,
                    outcome.covered_pattern_count,
                    outcome.pattern_count,
                    outcome.coverage_probability
                );
            }
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("clearra-pc-artifact: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod manual_profile {
    use super::*;

    #[test]
    #[ignore = "manual Windows product-pipeline profile"]
    fn empty_four_line_cpu_profile() {
        let output_root = std::env::var_os("CLEARRA_MANUAL_PROFILE_OUTPUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("Clearra").join("manual-empty-4l"));
        let workers = clearra_pc_graph::request::WorkerPolicy::default_worker_limit();
        let args = args::ArtifactArgs::parse([
            "--scenario".to_owned(),
            "empty-4l".to_owned(),
            "--backend".to_owned(),
            "cpu".to_owned(),
            "--workers".to_owned(),
            workers.to_string(),
            "--profile-stages".to_owned(),
            "--skip-fumen".to_owned(),
            "--output-dir".to_owned(),
            output_root.display().to_string(),
        ])
        .expect("manual profile arguments");
        let outcomes = artifact_batch::run_and_write(args).expect("empty 4L product pipeline");
        assert_eq!(outcomes.len(), 1);
    }
}
