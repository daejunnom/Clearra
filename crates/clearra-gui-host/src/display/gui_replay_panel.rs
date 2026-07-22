use clearra_app::AppResponse;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiReplayStep {
    piece: char,
    rotation: u8,
    x: i32,
    y: i32,
    hold: String,
    cleared_lines: u8,
}

impl GuiReplayStep {
    pub const fn piece(&self) -> char {
        self.piece
    }
}
impl GuiReplayStep {
    pub const fn rotation(&self) -> u8 {
        self.rotation
    }
}
impl GuiReplayStep {
    pub const fn x(&self) -> i32 {
        self.x
    }
}
impl GuiReplayStep {
    pub const fn y(&self) -> i32 {
        self.y
    }
}
impl GuiReplayStep {
    pub fn hold(&self) -> &str {
        &self.hold
    }
}
impl GuiReplayStep {
    pub const fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiReplayPanel {
    label_i18n_key: &'static str,
    trace_available: bool,
    retained_trace_count: usize,
    trace_retention_truncated: bool,
    steps: Vec<GuiReplayStep>,
}

impl GuiReplayPanel {
    pub fn from_response(response: &AppResponse) -> Self {
        let core = response
            .render_model()
            .and_then(|model| model.core_result());
        let steps: Vec<GuiReplayStep> = core
            .map(|result| {
                result
                    .path_steps()
                    .iter()
                    .map(|step| GuiReplayStep {
                        piece: step.piece().as_ascii(),
                        rotation: step.rotation(),
                        x: step.x(),
                        y: step.y(),
                        hold: step.hold().to_owned(),
                        cleared_lines: step.cleared_lines(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            label_i18n_key: "ui.result.replay",
            trace_available: !steps.is_empty()
                || core
                    .and_then(|result| result.bool_field("sample_trace_available"))
                    .unwrap_or(false),
            retained_trace_count: core
                .and_then(|result| result.usize_field("retained_trace_count"))
                .unwrap_or(steps.len()),
            trace_retention_truncated: core
                .and_then(|result| result.bool_field("trace_retention_truncated"))
                .unwrap_or(false),
            steps,
        }
    }
}
impl GuiReplayPanel {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiReplayPanel {
    pub const fn trace_available(&self) -> bool {
        self.trace_available
    }
}
impl GuiReplayPanel {
    pub const fn retained_trace_count(&self) -> usize {
        self.retained_trace_count
    }
}
impl GuiReplayPanel {
    pub const fn trace_retention_truncated(&self) -> bool {
        self.trace_retention_truncated
    }
}
impl GuiReplayPanel {
    pub fn steps(&self) -> &[GuiReplayStep] {
        &self.steps
    }
}
