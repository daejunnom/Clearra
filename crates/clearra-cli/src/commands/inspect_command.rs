use crate::{
    args::inspect_args::InspectArgs,
    output::{CliOutput, CommandRenderer, RenderFormat},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InspectCommand;

impl InspectCommand {
    pub fn run(args: &InspectArgs, format: RenderFormat) -> CliOutput {
        let fields = args
            .subject()
            .map(|subject| vec![("subject", subject.to_owned())])
            .unwrap_or_default();
        CommandRenderer::render_output(
            "inspect",
            crate::output::SummaryRenderContract::render_fields(fields),
            format,
        )
    }
}
