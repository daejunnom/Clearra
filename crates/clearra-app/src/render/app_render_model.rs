use std::ops::Deref;

use clearra_core_executor::CoreExecutionResult;
use clearra_forward_search::ForwardSearchReport;
use clearra_output::model::RenderField;
use clearra_spin_structure_search::SpinStructureReport;

use crate::setup_ranked_family_result::SetupRankedFamilySnapshot;

use super::AppResultKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppMessage {
    kind: AppResultKind,
    fields: Vec<RenderField>,
    raw_body: Option<String>,
}

impl AppMessage {
    pub fn new(kind: AppResultKind, fields: Vec<RenderField>) -> Self {
        Self {
            kind,
            fields,
            raw_body: None,
        }
    }
}
impl AppMessage {
    pub fn raw(kind: AppResultKind, body: impl Into<String>) -> Self {
        Self {
            kind,
            fields: Vec::new(),
            raw_body: Some(body.into()),
        }
    }
}
impl AppMessage {
    pub fn kind(&self) -> AppResultKind {
        self.kind
    }
}
impl AppMessage {
    pub fn fields(&self) -> &[RenderField] {
        &self.fields
    }
}
impl AppMessage {
    pub fn raw_body(&self) -> Option<&str> {
        self.raw_body.as_deref()
    }
}

/// Single-owner Setup render payload.
///
/// `core_result` remains the compatibility rendering authority while an
/// optional ranked-family snapshot preserves the validated App contract for
/// `setup.joint`, `setup.build`, and `setup.pc`. The snapshot contains no
/// duplicate solver report or solution paths.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupRenderModel {
    core_result: CoreExecutionResult,
    ranked_family_snapshot: Option<SetupRankedFamilySnapshot>,
}

impl SetupRenderModel {
    pub fn unranked(core_result: CoreExecutionResult) -> Self {
        Self {
            core_result,
            ranked_family_snapshot: None,
        }
    }

    pub fn ranked(
        core_result: CoreExecutionResult,
        ranked_family_snapshot: SetupRankedFamilySnapshot,
    ) -> Self {
        Self {
            core_result,
            ranked_family_snapshot: Some(ranked_family_snapshot),
        }
    }

    pub fn core_result(&self) -> &CoreExecutionResult {
        &self.core_result
    }

    pub fn ranked_family_snapshot(&self) -> Option<&SetupRankedFamilySnapshot> {
        self.ranked_family_snapshot.as_ref()
    }

    fn without_pc_chance_transient_evidence(self) -> Self {
        Self {
            core_result: self.core_result.without_pc_chance_transient_evidence(),
            ranked_family_snapshot: self.ranked_family_snapshot,
        }
    }

    fn without_pc_score_problem_evidence(self) -> Self {
        Self {
            core_result: self.core_result.without_pc_score_problem_evidence(),
            ranked_family_snapshot: self.ranked_family_snapshot,
        }
    }

    fn without_pc_score_transient_evidence(self) -> Self {
        Self {
            core_result: self.core_result.without_pc_score_transient_evidence(),
            ranked_family_snapshot: self.ranked_family_snapshot,
        }
    }
}

impl Deref for SetupRenderModel {
    type Target = CoreExecutionResult;

    fn deref(&self) -> &Self::Target {
        &self.core_result
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppRenderModel {
    Pc(CoreExecutionResult),
    Scenario(CoreExecutionResult),
    ScenarioMessage(AppMessage),
    Setup(SetupRenderModel),
    BuildProbability(CoreExecutionResult),
    Damage(ForwardSearchReport),
    SpinFinder(ForwardSearchReport),
    Ren(ForwardSearchReport),
    SpinStructure(SpinStructureReport),
    Cover(CoreExecutionResult),
    CoverMessage(AppMessage),
    Percent(CoreExecutionResult),
    Path(AppMessage),
    Rules(AppMessage),
    Scoring(AppMessage),
    Convert(AppMessage),
    Continue(AppMessage),
    Verify(AppMessage),
}

impl AppRenderModel {
    pub fn kind(&self) -> AppResultKind {
        match self {
            Self::Pc(_) => AppResultKind::Pc,
            Self::Scenario(_) => AppResultKind::Scenario,
            Self::ScenarioMessage(message) => message.kind(),
            Self::Setup(_) => AppResultKind::Setup,
            Self::BuildProbability(_) => AppResultKind::BuildProbability,
            Self::Damage(_) => AppResultKind::Damage,
            Self::SpinFinder(_) => AppResultKind::SpinFinder,
            Self::Ren(_) => AppResultKind::Ren,
            Self::SpinStructure(_) => AppResultKind::SpinStructure,
            Self::Cover(_) => AppResultKind::Cover,
            Self::Percent(_) => AppResultKind::Percent,
            Self::CoverMessage(message) => message.kind(),
            Self::Path(message)
            | Self::Rules(message)
            | Self::Scoring(message)
            | Self::Convert(message)
            | Self::Continue(message)
            | Self::Verify(message) => message.kind(),
        }
    }
}
impl AppRenderModel {
    pub fn forward_search_result(&self) -> Option<&ForwardSearchReport> {
        match self {
            Self::Damage(result) | Self::SpinFinder(result) | Self::Ren(result) => Some(result),
            _ => None,
        }
    }
}
impl AppRenderModel {
    pub fn spin_structure_result(&self) -> Option<&SpinStructureReport> {
        match self {
            Self::SpinStructure(result) => Some(result),
            _ => None,
        }
    }

    pub fn setup_ranked_family_snapshot(&self) -> Option<&SetupRankedFamilySnapshot> {
        match self {
            Self::Setup(result) => result.ranked_family_snapshot(),
            _ => None,
        }
    }
}
impl AppRenderModel {
    pub fn core_result(&self) -> Option<&CoreExecutionResult> {
        match self {
            Self::Pc(result)
            | Self::Scenario(result)
            | Self::BuildProbability(result)
            | Self::Cover(result)
            | Self::Percent(result) => Some(result),
            Self::Setup(result) => Some(result.core_result()),
            _ => None,
        }
    }
}
impl AppRenderModel {
    /// Removes product-private chance authority from every Core-backed render
    /// family without changing its public result kind or summary fields.
    pub(crate) fn without_pc_chance_transient_evidence(self) -> Self {
        match self {
            Self::Pc(result) => Self::Pc(result.without_pc_chance_transient_evidence()),
            Self::Scenario(result) => Self::Scenario(result.without_pc_chance_transient_evidence()),
            Self::Setup(result) => Self::Setup(result.without_pc_chance_transient_evidence()),
            Self::BuildProbability(result) => {
                Self::BuildProbability(result.without_pc_chance_transient_evidence())
            }
            Self::Cover(result) => Self::Cover(result.without_pc_chance_transient_evidence()),
            Self::Percent(result) => Self::Percent(result.without_pc_chance_transient_evidence()),
            other => other,
        }
    }

    /// Removes the producer-owned executed-problem snapshot from every
    /// Core-backed result. This always runs at the App boundary, including for
    /// generic score output that retains its established replay surface.
    pub(crate) fn without_pc_score_problem_evidence(self) -> Self {
        match self {
            Self::Pc(result) => Self::Pc(result.without_pc_score_problem_evidence()),
            Self::Scenario(result) => Self::Scenario(result.without_pc_score_problem_evidence()),
            Self::Setup(result) => Self::Setup(result.without_pc_score_problem_evidence()),
            Self::BuildProbability(result) => {
                Self::BuildProbability(result.without_pc_score_problem_evidence())
            }
            Self::Cover(result) => Self::Cover(result.without_pc_score_problem_evidence()),
            Self::Percent(result) => Self::Percent(result.without_pc_score_problem_evidence()),
            other => other,
        }
    }

    /// Removes producer-owned score execution material only for a validated
    /// typed score response. Generic `pc --score` responses do not call this
    /// boundary and retain their established result surface.
    pub(crate) fn without_pc_score_transient_evidence(self) -> Self {
        match self {
            Self::Pc(result) => Self::Pc(result.without_pc_score_transient_evidence()),
            Self::Scenario(result) => Self::Scenario(result.without_pc_score_transient_evidence()),
            Self::Setup(result) => Self::Setup(result.without_pc_score_transient_evidence()),
            Self::BuildProbability(result) => {
                Self::BuildProbability(result.without_pc_score_transient_evidence())
            }
            Self::Cover(result) => Self::Cover(result.without_pc_score_transient_evidence()),
            Self::Percent(result) => Self::Percent(result.without_pc_score_transient_evidence()),
            other => other,
        }
    }
}
impl AppRenderModel {
    pub fn message(&self) -> Option<&AppMessage> {
        match self {
            Self::CoverMessage(message)
            | Self::ScenarioMessage(message)
            | Self::Path(message)
            | Self::Rules(message)
            | Self::Scoring(message)
            | Self::Convert(message)
            | Self::Continue(message)
            | Self::Verify(message) => Some(message),
            _ => None,
        }
    }
}
