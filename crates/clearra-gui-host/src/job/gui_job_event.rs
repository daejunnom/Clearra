use crate::{GuiJobId, GuiJobProgress};
use clearra_app::ProductPageSourceOwner;
use clearra_host_contract::AppResponse;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GuiJobEvent {
    Started {
        job_id: GuiJobId,
    },
    Progress {
        job_id: GuiJobId,
        progress: GuiJobProgress,
    },
    Diagnostic {
        job_id: GuiJobId,
        code: String,
        severity: String,
    },
    Completed {
        job_id: GuiJobId,
        response: AppResponse,
        search_report_json: Option<String>,
        product_page_source_owner: Option<ProductPageSourceOwner>,
    },
    Failed {
        job_id: GuiJobId,
        code: String,
    },
    Cancelled {
        job_id: GuiJobId,
    },
}

impl GuiJobEvent {
    pub const fn job_id(&self) -> GuiJobId {
        match self {
            Self::Started { job_id }
            | Self::Progress { job_id, .. }
            | Self::Diagnostic { job_id, .. }
            | Self::Completed { job_id, .. }
            | Self::Failed { job_id, .. }
            | Self::Cancelled { job_id } => *job_id,
        }
    }

    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }
}
