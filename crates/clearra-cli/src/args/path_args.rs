use super::pc_args::PcArgs;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathArgs {
    pc: PcArgs,
}

impl PathArgs {
    pub fn new(pc: PcArgs) -> Self {
        Self { pc }
    }
}
impl PathArgs {
    pub fn pc(&self) -> &PcArgs {
        &self.pc
    }
}

impl Default for PathArgs {
    fn default() -> Self {
        Self::new(PcArgs::default())
    }
}
