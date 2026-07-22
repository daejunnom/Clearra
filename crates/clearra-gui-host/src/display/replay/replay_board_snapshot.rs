use clearra_app::AppResponse;

use super::super::field_value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayBoardSnapshot {
    step_index: usize,
    board_before: String,
    board_after_placement: String,
    board_after_line_clear: String,
    available: bool,
}

impl ReplayBoardSnapshot {
    pub fn from_response(response: &AppResponse, step_index: usize) -> Self {
        let board_before = indexed_field(response, "replay_board_before", step_index)
            .unwrap_or_else(|| "not_available".to_owned());
        let board_after_placement =
            indexed_field(response, "replay_board_after_placement", step_index)
                .unwrap_or_else(|| "not_available".to_owned());
        let board_after_line_clear =
            indexed_field(response, "replay_board_after_line_clear", step_index)
                .unwrap_or_else(|| "not_available".to_owned());
        let available = board_before != "not_available"
            || board_after_placement != "not_available"
            || board_after_line_clear != "not_available";

        Self {
            step_index,
            board_before,
            board_after_placement,
            board_after_line_clear,
            available,
        }
    }
}
impl ReplayBoardSnapshot {
    pub const fn step_index(&self) -> usize {
        self.step_index
    }
}
impl ReplayBoardSnapshot {
    pub fn board_before(&self) -> &str {
        &self.board_before
    }
}
impl ReplayBoardSnapshot {
    pub fn board_after_placement(&self) -> &str {
        &self.board_after_placement
    }
}
impl ReplayBoardSnapshot {
    pub fn board_after_line_clear(&self) -> &str {
        &self.board_after_line_clear
    }
}
impl ReplayBoardSnapshot {
    pub const fn available(&self) -> bool {
        self.available
    }
}

fn indexed_field(response: &AppResponse, prefix: &str, index: usize) -> Option<String> {
    let indexed = format!("{prefix}_{index}");
    field_value(response, &indexed).or_else(|| {
        if index == 0 {
            field_value(response, prefix)
        } else {
            None
        }
    })
}
