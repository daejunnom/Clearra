#[cfg(feature = "wasm-cpu-runtime")]
use crate::error::CliErrorCode;
use crate::{
    args::{ParsedCliCommand, ParsedCliInvocation},
    assemble::CliAppRequestAssembler,
    input::file_input_guard,
    output::{AppResponseRenderer, CliOutput},
};
use clearra_app::{io::AppFilePolicy, AppContext};
#[cfg(feature = "wasm-cpu-runtime")]
use clearra_app::{AppCoreExecutorService, AppServices, AppTablebaseSession};

#[cfg(feature = "wasm-cpu-runtime")]
const PC4_COMPACT_TABLEBASE: &[u8] =
    include_bytes!("../../../apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin");

const TILING_ONLY_WARNING: &str = "WARNING: Tiling-only search skips BuildUp and probability calculation. Results may include solutions that cannot be built.";

pub(crate) fn route_invocation(invocation: ParsedCliInvocation) -> CliOutput {
    let format = invocation
        .output_verbosity()
        .apply_to_format(invocation.format());
    let language = invocation.language();
    let verbose_paths = invocation.verbose_paths();
    file_input_guard::with_verbose_paths(verbose_paths, || {
        let command = invocation.into_command();
        if let ParsedCliCommand::Help(topic) = command {
            return topic.into_output(language);
        }
        let tiling_only = matches!(
            &command,
            ParsedCliCommand::Pc(args)
                if matches!(
                    args.objective()
                        .trim()
                        .to_ascii_lowercase()
                        .replace('_', "-")
                        .as_str(),
                    "tiling" | "tiling-only"
                )
        );
        #[cfg(feature = "wasm-cpu-runtime")]
        let _tablebase_session = match tablebase_session_for_command(&command) {
            Ok(session) => session,
            Err(output) => return output,
        };

        let assembly = match CliAppRequestAssembler::assemble(command, format) {
            Ok(assembly) => assembly,
            Err(output) => return output,
        };
        let render_format = assembly.render_format();
        let default_error = assembly.default_error();
        let request = assembly
            .request()
            .with_language(language)
            .with_file_policy(AppFilePolicy::new(verbose_paths));
        let response = product_app_context()
            .with_language(language)
            .with_file_policy(AppFilePolicy::new(verbose_paths))
            .run(request);
        let output = AppResponseRenderer::render(response, render_format, default_error);
        if tiling_only {
            output.with_surrounding_warning(TILING_ONLY_WARNING)
        } else {
            output
        }
    })
}

#[cfg(feature = "wasm-cpu-runtime")]
fn tablebase_session_for_command(
    command: &ParsedCliCommand,
) -> Result<Option<AppTablebaseSession>, CliOutput> {
    let requested = match command {
        ParsedCliCommand::Pc(args) => args.tablebase_requested() == Some(true),
        ParsedCliCommand::Setup(args) => args.tablebase_requested() == Some(true),
        _ => false,
    };
    install_requested_tablebase(requested, PC4_COMPACT_TABLEBASE)
}

#[cfg(feature = "wasm-cpu-runtime")]
fn install_requested_tablebase(
    requested: bool,
    artifact: &[u8],
) -> Result<Option<AppTablebaseSession>, CliOutput> {
    if !requested {
        return Ok(None);
    }

    AppTablebaseSession::install_pc4_compact(artifact)
        .map(Some)
        .map_err(|error| {
            CliOutput::error(
                CliErrorCode::TablebaseInstallFailed,
                format!("PC4 tablebase installation failed: {}", error.reason()),
            )
        })
}

fn product_app_context() -> AppContext {
    #[cfg(feature = "wasm-cpu-runtime")]
    {
        return AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
    }

    #[cfg(not(feature = "wasm-cpu-runtime"))]
    AppContext::default()
}

#[cfg(all(test, feature = "wasm-cpu-runtime"))]
mod tests {
    use super::install_requested_tablebase;
    use crate::{error::CliErrorCode, exit::ExitCode};

    #[test]
    fn explicit_tablebase_request_fails_closed_when_installation_fails() {
        let output = install_requested_tablebase(true, b"not-a-tablebase")
            .expect_err("an explicit request must not silently fall back");

        assert_eq!(output.exit_code(), ExitCode::InternalError);
        assert!(output
            .stderr()
            .contains(CliErrorCode::TablebaseInstallFailed.as_str()));
        assert!(output.stderr().contains("pc4_tablebase_header_invalid"));
    }

    #[test]
    fn unrequested_tablebase_does_not_touch_the_artifact() {
        assert!(install_requested_tablebase(false, b"not-a-tablebase")
            .expect("disabled tablebase must not be installed")
            .is_none());
    }
}
