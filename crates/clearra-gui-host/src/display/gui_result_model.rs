use clearra_app::AppResponse;

use crate::{
    FumenOutputView, GuiBackendReportPanel, GuiCoveragePanel, GuiDiagnosticPanel, GuiExportPanel,
    GuiFumenPanel, GuiGpuStatusViewModel, GuiMemoryReportPanel, GuiReplayPanel, GuiSolutionTable,
    GuiSummaryPanel, RenderExportView, RenderPreviewView, ReplayTimelineView,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiResultModel {
    schema_version: u32,
    summary_panel: GuiSummaryPanel,
    solution_table: GuiSolutionTable,
    coverage_panel: GuiCoveragePanel,
    backend_report_panel: GuiBackendReportPanel,
    gpu_status_view_model: GuiGpuStatusViewModel,
    memory_report_panel: GuiMemoryReportPanel,
    diagnostic_panel: GuiDiagnosticPanel,
    replay_panel: GuiReplayPanel,
    replay_timeline_view: ReplayTimelineView,
    fumen_panel: GuiFumenPanel,
    fumen_output_view: FumenOutputView,
    render_preview_view: RenderPreviewView,
    render_export_view: RenderExportView,
    export_panel: GuiExportPanel,
    json_contract_keys_localized: bool,
}

impl GuiResultModel {
    pub fn from_response(response: &AppResponse) -> Self {
        Self {
            schema_version: clearra_output::json::JSON_SCHEMA_VERSION,
            summary_panel: GuiSummaryPanel::from_response(response),
            solution_table: GuiSolutionTable::from_response(response),
            coverage_panel: GuiCoveragePanel::from_response(response),
            backend_report_panel: GuiBackendReportPanel::from_response(response),
            gpu_status_view_model: GuiGpuStatusViewModel::from_response(response),
            memory_report_panel: GuiMemoryReportPanel::from_response(response),
            diagnostic_panel: GuiDiagnosticPanel::from_response(response),
            replay_panel: GuiReplayPanel::from_response(response),
            replay_timeline_view: ReplayTimelineView::from_response(response),
            fumen_panel: GuiFumenPanel::from_response(response),
            fumen_output_view: FumenOutputView::from_response(response),
            render_preview_view: RenderPreviewView::from_response(response),
            render_export_view: RenderExportView::from_response(response),
            export_panel: GuiExportPanel::from_response(response),
            json_contract_keys_localized: false,
        }
    }
}
impl GuiResultModel {
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
impl GuiResultModel {
    pub const fn summary_panel(&self) -> &GuiSummaryPanel {
        &self.summary_panel
    }
}
impl GuiResultModel {
    pub const fn solution_table(&self) -> &GuiSolutionTable {
        &self.solution_table
    }
}
impl GuiResultModel {
    pub const fn coverage_panel(&self) -> &GuiCoveragePanel {
        &self.coverage_panel
    }
}
impl GuiResultModel {
    pub const fn backend_report_panel(&self) -> &GuiBackendReportPanel {
        &self.backend_report_panel
    }
}
impl GuiResultModel {
    pub const fn gpu_status_view_model(&self) -> &GuiGpuStatusViewModel {
        &self.gpu_status_view_model
    }
}
impl GuiResultModel {
    pub const fn memory_report_panel(&self) -> &GuiMemoryReportPanel {
        &self.memory_report_panel
    }
}
impl GuiResultModel {
    pub const fn diagnostic_panel(&self) -> &GuiDiagnosticPanel {
        &self.diagnostic_panel
    }
}
impl GuiResultModel {
    pub const fn replay_panel(&self) -> &GuiReplayPanel {
        &self.replay_panel
    }
}
impl GuiResultModel {
    pub const fn replay_timeline_view(&self) -> &ReplayTimelineView {
        &self.replay_timeline_view
    }
}
impl GuiResultModel {
    pub const fn fumen_panel(&self) -> &GuiFumenPanel {
        &self.fumen_panel
    }
}
impl GuiResultModel {
    pub const fn fumen_output_view(&self) -> &FumenOutputView {
        &self.fumen_output_view
    }
}
impl GuiResultModel {
    pub const fn render_preview_view(&self) -> &RenderPreviewView {
        &self.render_preview_view
    }
}
impl GuiResultModel {
    pub const fn render_export_view(&self) -> &RenderExportView {
        &self.render_export_view
    }
}
impl GuiResultModel {
    pub const fn export_panel(&self) -> &GuiExportPanel {
        &self.export_panel
    }
}
impl GuiResultModel {
    pub const fn json_contract_keys_localized(&self) -> bool {
        self.json_contract_keys_localized
    }
}
