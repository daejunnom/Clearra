use crate::{error::CliErrorCode, output::CliOutput};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UnsupportedCommand;

impl UnsupportedCommand {
    pub fn run(command: &str) -> CliOutput {
        if command == "inspect" {
            return CliOutput::error(
                CliErrorCode::CliCommandUnsupported,
                "inspect is unsupported; use rules inspect or scoring inspect for profile inspection",
            );
        }

        CliOutput::error(
            CliErrorCode::CliCommandUnsupported,
            format!("command '{command}' is outside the MVP1 executable CLI path"),
        )
    }
}
