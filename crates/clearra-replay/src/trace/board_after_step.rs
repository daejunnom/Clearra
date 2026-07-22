use crate::board::board64_state::Board64State;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardAfterStep {
    after_placement: Board64State,
    after_line_clear: Board64State,
}

impl BoardAfterStep {
    pub fn new(after_placement: Board64State, after_line_clear: Board64State) -> Self {
        Self {
            after_placement,
            after_line_clear,
        }
    }
}
impl BoardAfterStep {
    pub fn after_placement(self) -> Board64State {
        self.after_placement
    }
}
impl BoardAfterStep {
    pub fn after_line_clear(self) -> Board64State {
        self.after_line_clear
    }
}
