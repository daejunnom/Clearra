use clearra_app::AppResponse;

use super::super::field_value;
use super::{ReplayBoardSnapshot, ReplayLineClearView, ReplayPieceOwnershipView, ReplayStepView};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayTimelineView {
    label_i18n_key: &'static str,
    trace_exists: bool,
    retained_trace_count: usize,
    trace_retention_truncated: bool,
    steps: Vec<ReplayStepView>,
    board_snapshots: Vec<ReplayBoardSnapshot>,
    piece_ownership: ReplayPieceOwnershipView,
    line_clears: Vec<ReplayLineClearView>,
}

impl ReplayTimelineView {
    pub fn from_response(response: &AppResponse) -> Self {
        let core = response
            .render_model()
            .and_then(|model| model.core_result());
        let steps: Vec<ReplayStepView> = core
            .map(|result| {
                result
                    .path_steps()
                    .iter()
                    .enumerate()
                    .map(|(index, step)| {
                        ReplayStepView::new(
                            index,
                            step.piece().as_ascii(),
                            step.rotation(),
                            step.x(),
                            step.y(),
                            step.hold(),
                            step.cleared_lines(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_else(|| field_steps(response));
        let board_snapshots = (0..steps.len())
            .map(|index| ReplayBoardSnapshot::from_response(response, index))
            .collect();
        let line_clears = steps
            .iter()
            .map(|step| ReplayLineClearView::new(step.step_index(), step.cleared_lines()))
            .collect();
        let retained_trace_count = core
            .and_then(|result| result.usize_field("retained_trace_count"))
            .unwrap_or(steps.len());
        let sample_trace_available = core
            .and_then(|result| result.bool_field("sample_trace_available"))
            .unwrap_or(false);

        Self {
            label_i18n_key: "ui.result.replay.timeline",
            trace_exists: !steps.is_empty() || sample_trace_available || retained_trace_count > 0,
            retained_trace_count,
            trace_retention_truncated: core
                .and_then(|result| result.bool_field("trace_retention_truncated"))
                .unwrap_or(false),
            steps,
            board_snapshots,
            piece_ownership: ReplayPieceOwnershipView::from_response(response),
            line_clears,
        }
    }
}
impl ReplayTimelineView {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl ReplayTimelineView {
    pub const fn trace_exists(&self) -> bool {
        self.trace_exists
    }
}
impl ReplayTimelineView {
    pub const fn retained_trace_count(&self) -> usize {
        self.retained_trace_count
    }
}
impl ReplayTimelineView {
    pub const fn trace_retention_truncated(&self) -> bool {
        self.trace_retention_truncated
    }
}
impl ReplayTimelineView {
    pub fn steps(&self) -> &[ReplayStepView] {
        &self.steps
    }
}
impl ReplayTimelineView {
    pub fn board_snapshots(&self) -> &[ReplayBoardSnapshot] {
        &self.board_snapshots
    }
}
impl ReplayTimelineView {
    pub const fn piece_ownership(&self) -> &ReplayPieceOwnershipView {
        &self.piece_ownership
    }
}
impl ReplayTimelineView {
    pub fn line_clears(&self) -> &[ReplayLineClearView] {
        &self.line_clears
    }
}

fn field_steps(response: &AppResponse) -> Vec<ReplayStepView> {
    let step_count = field_value(response, "replay_step_count")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    (0..step_count)
        .map(|index| {
            ReplayStepView::new(
                index,
                indexed_field(response, "replay_step_piece", index)
                    .and_then(|value| value.chars().next())
                    .unwrap_or('?'),
                indexed_field(response, "replay_step_rotation", index)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                indexed_field(response, "replay_step_x", index)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                indexed_field(response, "replay_step_y", index)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
                indexed_field(response, "replay_step_hold", index)
                    .unwrap_or_else(|| "none".to_owned()),
                indexed_field(response, "replay_step_cleared_lines", index)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0),
            )
        })
        .collect()
}

fn indexed_field(response: &AppResponse, prefix: &str, index: usize) -> Option<String> {
    field_value(response, &format!("{prefix}_{index}"))
}
