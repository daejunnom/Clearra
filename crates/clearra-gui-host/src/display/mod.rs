pub mod fumen;
pub mod gpu;
mod gui_backend_report_panel;
mod gui_coverage_panel;
mod gui_diagnostic_panel;
mod gui_export_panel;
mod gui_fumen_panel;
mod gui_memory_report_panel;
mod gui_replay_panel;
mod gui_result_model;
mod gui_solution_table;
mod gui_summary_panel;
pub mod render;
pub mod replay;

use clearra_app::{AppResponse, AppStatus};

pub use fumen::{FumenCopyButtonModel, FumenOutputView, FumenPageListView};
pub use gpu::{
    GuiGpuBackendChoiceView, GuiGpuBackpressureView, GuiGpuFallbackReasonView,
    GuiGpuMemoryTicketView, GuiGpuStatusViewModel, GuiGpuTrustStateView,
};
pub use gui_backend_report_panel::GuiBackendReportPanel;
pub use gui_coverage_panel::GuiCoveragePanel;
pub use gui_diagnostic_panel::{GuiDiagnosticEntry, GuiDiagnosticEvidence, GuiDiagnosticPanel};
pub use gui_export_panel::GuiExportPanel;
pub use gui_fumen_panel::GuiFumenPanel;
pub use gui_memory_report_panel::GuiMemoryReportPanel;
pub use gui_replay_panel::{GuiReplayPanel, GuiReplayStep};
pub use gui_result_model::GuiResultModel;
pub use gui_solution_table::{GuiSolutionRow, GuiSolutionTable};
pub use gui_summary_panel::GuiSummaryPanel;
pub use render::{RenderCapabilityView, RenderExportView, RenderPreviewView, SkinSelectorView};
pub use replay::{
    ReplayBoardSnapshot, ReplayLineClearView, ReplayPieceOwnershipView, ReplayStepView,
    ReplayTimelineView,
};

pub(crate) fn response_status(response: &AppResponse) -> &'static str {
    match response.status() {
        AppStatus::Success => "success",
        AppStatus::ValidationFailed => "validation-failed",
        AppStatus::Unsupported => "unsupported",
        AppStatus::ExecutionFailed => "execution-failed",
    }
}

pub(crate) fn result_kind(response: &AppResponse) -> String {
    response
        .render_model()
        .map(|model| model.kind().as_str().to_owned())
        .unwrap_or_else(|| "none".to_owned())
}

pub(crate) fn field_value(response: &AppResponse, key: &str) -> Option<String> {
    let model = response.render_model()?;
    if let Some(core) = model.core_result() {
        if let Some(value) = core.field(key) {
            return Some(value.to_owned());
        }
    }
    model.message().and_then(|message| {
        message
            .fields()
            .iter()
            .find(|field| field.key() == key)
            .map(|field| field.value().as_text())
    })
}

pub(crate) fn first_field(response: &AppResponse, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| field_value(response, key))
}

pub(crate) fn bool_field(response: &AppResponse, key: &str) -> bool {
    field_value(response, key)
        .and_then(|value| value.parse().ok())
        .unwrap_or(false)
}
