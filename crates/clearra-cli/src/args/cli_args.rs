#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliArgs {
    command: CliCommand,
}

impl CliArgs {
    pub fn new(command: CliCommand) -> Self {
        Self { command }
    }
}
impl CliArgs {
    pub fn command(&self) -> CliCommand {
        self.command
    }
}

impl Default for CliArgs {
    fn default() -> Self {
        Self::new(CliCommand::Help)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CliCommand {
    Pc,
    PcScenario,
    Path,
    Percent,
    Cover,
    Setup,
    Continue,
    Rules,
    Scoring,
    Convert,
    Inspect,
    Verify,
    #[default]
    Help,
}
