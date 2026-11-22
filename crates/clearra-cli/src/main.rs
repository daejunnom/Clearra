const CLI_PRODUCT_STACK_BYTES: usize = 16 * 1024 * 1024;

fn run_cli_product() -> i32 {
    #[cfg(feature = "local-search-ab")]
    clearra_app::configure_local_product_search_benchmark();
    #[cfg(feature = "wasm-cpu-runtime")]
    let _native_build_probability_registration =
        clearra_app::register_system_native_build_probability_host();
    #[cfg(feature = "stage-profiling")]
    let stage_profile = diagnostic_stage_profile();
    let exit_code = clearra_cli::run();
    #[cfg(feature = "stage-profiling")]
    if let Some(session) = stage_profile {
        let stages: Vec<_> = session
            .finish()
            .into_iter()
            .filter(|stage| stage.invocation_count != 0 || stage.work_item_count != 0)
            .map(|stage| {
                serde_json::json!({
                    "name": stage.name,
                    "duration_ns": stage.duration_ns,
                    "invocation_count": stage.invocation_count,
                    "work_item_count": stage.work_item_count,
                })
            })
            .collect();
        eprintln!(
            "clearra_native_cpu_stages={}",
            serde_json::json!({ "schema_version": 1, "stages": stages })
        );
    }
    exit_code
}

#[cfg(feature = "stage-profiling")]
fn diagnostic_stage_profile() -> Option<clearra_app::ExecutorSearchProfileSession> {
    let requested = std::env::var("CLEARRA_NATIVE_CPU_STAGE_PROBE").as_deref() == Ok("1");
    let supervised = std::env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() == Ok("1")
        && std::env::var("CLEARRA_RUNTIME_PROFILE").as_deref() == Ok("benchmark-search");
    if requested && supervised {
        Some(clearra_app::ExecutorSearchProfileSession::start().expect("first CLI stage capture"))
    } else {
        None
    }
}

fn report_cli_product_error(code: clearra_cli::error::CliErrorCode, message: &str) -> i32 {
    clearra_cli::output::CliOutputDispatcher::dispatch(&clearra_cli::output::CliOutput::error(
        code, message,
    ))
}

fn main() {
    let exit_code = match std::thread::Builder::new()
        .name("clearra-cli-product".to_owned())
        .stack_size(CLI_PRODUCT_STACK_BYTES)
        .spawn(run_cli_product)
    {
        Ok(handle) => match handle.join() {
            Ok(exit_code) => exit_code,
            Err(payload) => std::panic::resume_unwind(payload),
        },
        Err(error) => report_cli_product_error(
            clearra_cli::error::CliErrorCode::CliProductThreadUnavailable,
            &format!("unable to start CLI product execution: {error}"),
        ),
    };
    std::process::exit(exit_code);
}
