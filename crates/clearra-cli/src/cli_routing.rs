use crate::{
    args::{ParsedCliCommand, ParsedCliInvocation},
    assemble::CliAppRequestAssembler,
    input::file_input_guard,
    output::{AppResponseRenderer, CliOutput},
};
use clearra_app::{io::AppFilePolicy, AppContext};

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
        let response = AppContext::default()
            .with_language(language)
            .with_file_policy(AppFilePolicy::new(verbose_paths))
            .run(request);
        AppResponseRenderer::render(response, render_format, default_error)
    })
}
