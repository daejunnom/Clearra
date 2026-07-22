use clearra_core_executor::CoreExecutionResult;
use clearra_forward_search::ForwardSearchReport;
use clearra_output::model::RenderField;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppRenderModel {
    Pc(CoreExecutionResult),
    Scenario(CoreExecutionResult),
    ScenarioMessage(AppMessage),
    Setup(CoreExecutionResult),
    BuildProbability(CoreExecutionResult),
    Damage(ForwardSearchReport),
    SpinFinder(ForwardSearchReport),
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
            Self::Damage(result) | Self::SpinFinder(result) => Some(result),
            _ => None,
        }
    }
}
impl AppRenderModel {
    pub fn core_result(&self) -> Option<&CoreExecutionResult> {
        match self {
            Self::Pc(result)
            | Self::Scenario(result)
            | Self::Setup(result)
            | Self::BuildProbability(result)
            | Self::Cover(result)
            | Self::Percent(result) => Some(result),
            _ => None,
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
