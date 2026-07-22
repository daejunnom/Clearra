use super::pc_scenario_args::PcScenarioArgs;

impl PcScenarioArgs {
    pub fn with_field(mut self, field: Option<String>) -> Self {
        self.field = field;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_board_width(mut self, board_width: Option<u16>) -> Self {
        self.board_width = board_width;
        self
    }
}
impl PcScenarioArgs {
    pub fn with_visible_height(mut self, visible_height: Option<u16>) -> Self {
        self.visible_height = visible_height;
        self
    }
}
impl PcScenarioArgs {
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }
}
impl PcScenarioArgs {
    pub fn board_width(&self) -> Option<u16> {
        self.board_width
    }
}
impl PcScenarioArgs {
    pub fn visible_height(&self) -> Option<u16> {
        self.visible_height
    }
}
