use crate::commands::{
    BuildProbabilityAppCommand, ContinueAppCommand, ConvertAppCommand, CoverAppCommand,
    DamageAppCommand, InspectUnsupportedAppCommand, PathAppCommand, PcAppCommand,
    PercentAppCommand, RulesAppCommand, ScenarioAppCommand, ScoringAppCommand, SetupAppCommand,
    SpinFinderAppCommand, VerifyAppCommand,
};
use crate::{app_context::AppExecutionContext, app_response::AppResponse};
use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_host_contract::{AppCommandKind, BackendPolicy, QueryEnvelope};
use clearra_pc_graph::request::PcCountPolicy;
use clearra_validation::{
    diagnostic::diagnostic_report::DiagnosticReport,
    validators::{
        build_query_validator::validate_build_coverage_query,
        pc_query_validator::{validate_opening_pc_search_query, validate_pc_scenario_query},
        setup_query_validator::validate_setup_search_query,
    },
};

pub trait RunnableAppCommand {
    fn validate(&self) -> DiagnosticReport {
        DiagnosticReport::new()
    }

    fn validation_failed_response(&self, report: DiagnosticReport) -> Option<AppResponse> {
        let _ = report;
        None
    }

    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse;
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppCommand {
    Pc(PcAppCommand),
    Scenario(ScenarioAppCommand),
    Path(PathAppCommand),
    Percent(PercentAppCommand),
    Setup(SetupAppCommand),
    BuildProbability(BuildProbabilityAppCommand),
    Damage(DamageAppCommand),
    SpinFinder(SpinFinderAppCommand),
    Cover(CoverAppCommand),
    Rules(RulesAppCommand),
    Scoring(ScoringAppCommand),
    Convert(ConvertAppCommand),
    Continue(ContinueAppCommand),
    InspectUnsupported(InspectUnsupportedAppCommand),
    Verify(VerifyAppCommand),
    VerifyKicks(VerifyAppCommand),
}

impl AppCommand {
    pub fn kind(&self) -> AppCommandKind {
        match self {
            Self::Pc(_) | Self::Scenario(_) => AppCommandKind::Pc,
            Self::Path(_) => AppCommandKind::Path,
            Self::Percent(_) => AppCommandKind::Percent,
            Self::Setup(_) => AppCommandKind::Setup,
            Self::BuildProbability(_) => AppCommandKind::BuildProbability,
            Self::Damage(_) => AppCommandKind::Damage,
            Self::SpinFinder(_) => AppCommandKind::SpinFinder,
            Self::Cover(_) => AppCommandKind::Cover,
            Self::Rules(_) => AppCommandKind::Rules,
            Self::Scoring(_) => AppCommandKind::Scoring,
            Self::Convert(_) => AppCommandKind::Convert,
            Self::Continue(_) => AppCommandKind::Continue,
            Self::InspectUnsupported(_) => AppCommandKind::InspectUnsupported,
            Self::Verify(command) => {
                if matches!(command.scope(), Some("kicks")) {
                    AppCommandKind::VerifyKicks
                } else {
                    AppCommandKind::Verify
                }
            }
            Self::VerifyKicks(_) => AppCommandKind::VerifyKicks,
        }
    }
}
impl AppCommand {
    pub fn query_envelope(&self) -> QueryEnvelope {
        match self {
            Self::Pc(_) => QueryEnvelope::PcOpening,
            Self::Scenario(_) => QueryEnvelope::PcScenario,
            Self::Path(_) => QueryEnvelope::PathOpening,
            Self::Percent(_) => QueryEnvelope::PercentScenario,
            Self::Setup(_) => QueryEnvelope::SetupSearch,
            Self::BuildProbability(_) => QueryEnvelope::BuildProbability,
            Self::Damage(_) => QueryEnvelope::Damage,
            Self::SpinFinder(_) => QueryEnvelope::SpinFinder,
            Self::Cover(_) => QueryEnvelope::BuildCoverage,
            Self::Rules(_) => QueryEnvelope::Rules,
            Self::Scoring(_) => QueryEnvelope::Scoring,
            Self::Convert(_) => QueryEnvelope::Convert,
            Self::Continue(_) => QueryEnvelope::ContinueToken,
            Self::InspectUnsupported(_) => QueryEnvelope::InspectUnsupported,
            Self::Verify(command) => {
                if matches!(command.scope(), Some("kicks")) {
                    QueryEnvelope::VerifyKicks
                } else {
                    QueryEnvelope::Verify
                }
            }
            Self::VerifyKicks(_) => QueryEnvelope::VerifyKicks,
        }
    }
}
impl AppCommand {
    pub fn backend_policy(&self) -> BackendPolicy {
        match self {
            Self::Pc(command) => BackendPolicy::new(
                command
                    .query()
                    .execution_policy()
                    .requested_backend()
                    .as_str(),
                command.query().execution_policy().allow_backend_fallback(),
            ),
            Self::Scenario(command) => BackendPolicy::new(
                command
                    .query()
                    .execution_policy()
                    .requested_backend()
                    .as_str(),
                command.query().execution_policy().allow_backend_fallback(),
            ),
            Self::Percent(command) => BackendPolicy::new(
                command.requested_backend(),
                command.allow_backend_fallback(),
            ),
            Self::BuildProbability(command) => BackendPolicy::new(
                command
                    .query()
                    .core_query()
                    .execution_policy()
                    .requested_backend()
                    .as_str(),
                command
                    .query()
                    .core_query()
                    .execution_policy()
                    .allow_backend_fallback(),
            ),
            Self::Damage(_) | Self::SpinFinder(_) => BackendPolicy::new("cpu", false),
            Self::Path(command) => BackendPolicy::new(
                command
                    .query()
                    .execution_policy()
                    .requested_backend()
                    .as_str(),
                command.query().execution_policy().allow_backend_fallback(),
            ),
            _ => BackendPolicy::default(),
        }
    }

    pub fn gpu_device_requested(&self) -> Option<String> {
        match self {
            Self::Pc(command) => Some(
                command
                    .query()
                    .execution_policy()
                    .gpu_device()
                    .as_display_string(),
            ),
            Self::Scenario(command) => Some(
                command
                    .query()
                    .execution_policy()
                    .gpu_device()
                    .as_display_string(),
            ),
            Self::Percent(command) => Some(command.gpu_device_display()),
            Self::Path(command) => Some(
                command
                    .query()
                    .execution_policy()
                    .gpu_device()
                    .as_display_string(),
            ),
            Self::BuildProbability(command) => Some(
                command
                    .query()
                    .core_query()
                    .execution_policy()
                    .gpu_device()
                    .as_display_string(),
            ),
            _ => None,
        }
    }
}

impl AppCommand {
    pub fn pattern_coverage_requested(&self) -> bool {
        match self {
            Self::Pc(command) => !matches!(
                command.query().objective().kind(),
                ObjectiveKind::Unique | ObjectiveKind::Tiling
            ),
            Self::Scenario(command) => command.query().count_policy() != PcCountPolicy::CountUnique,
            Self::Path(command) => !matches!(
                command.query().objective().kind(),
                ObjectiveKind::Unique | ObjectiveKind::Tiling
            ),
            Self::BuildProbability(command) => !command.query().aggregation().is_tiling_only(),
            Self::Percent(_) => true,
            Self::Damage(_) | Self::SpinFinder(_) => false,
            _ => false,
        }
    }
}

impl RunnableAppCommand for AppCommand {
    fn validate(&self) -> DiagnosticReport {
        match self {
            Self::Pc(command) => validate_opening_pc_search_query(command.query()),
            Self::Scenario(command) => validate_pc_scenario_query(command.query()),
            Self::Path(command) => validate_opening_pc_search_query(command.query()),
            Self::Percent(command) => command.validate(),
            Self::Setup(command) => validate_setup_search_query(command.query()),
            Self::BuildProbability(_) => DiagnosticReport::new(),
            Self::Damage(_) | Self::SpinFinder(_) => DiagnosticReport::new(),
            Self::Cover(command) => validate_build_coverage_query(command.query()),
            Self::Rules(command) => command.validate(),
            Self::Scoring(command) => command.validate(),
            Self::Convert(command) => command.validate(),
            Self::Continue(command) => command.validate(),
            Self::InspectUnsupported(command) => command.validate(),
            Self::Verify(command) => command.validate(),
            Self::VerifyKicks(command) => command.validate(),
        }
    }

    fn validation_failed_response(&self, report: DiagnosticReport) -> Option<AppResponse> {
        match self {
            Self::Scenario(command) => command.validation_failed_response(report),
            _ => None,
        }
    }

    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        match self {
            Self::Pc(command) => command.run(context),
            Self::Scenario(command) => command.run(context),
            Self::Path(command) => command.run(context),
            Self::Percent(command) => command.run(context),
            Self::Setup(command) => command.run(context),
            Self::BuildProbability(command) => command.run(context),
            Self::Damage(command) => command.run(context),
            Self::SpinFinder(command) => command.run(context),
            Self::Cover(command) => command.run(context),
            Self::Rules(command) => command.run(context),
            Self::Scoring(command) => command.run(context),
            Self::Convert(command) => command.run(context),
            Self::Continue(command) => command.run(context),
            Self::InspectUnsupported(command) => command.run(context),
            Self::Verify(command) => command.run(context),
            Self::VerifyKicks(command) => command.run(context),
        }
    }
}
