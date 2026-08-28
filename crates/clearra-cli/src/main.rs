const CLI_PRODUCT_STACK_BYTES: usize = 16 * 1024 * 1024;

fn run_cli_product() -> i32 {
    #[cfg(feature = "wasm-cpu-runtime")]
    let _native_build_probability_registration =
        clearra_app::register_system_native_build_probability_host();
    clearra_cli::run()
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
