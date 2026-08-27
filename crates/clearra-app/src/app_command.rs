use crate::commands::{
    BuildProbabilityAppCommand, BuildV2AppCommand, ContinueAppCommand, ConvertAppCommand,
    CoverAppCommand, DamageAppCommand, FieldDocumentTransformAppCommand, FumenAppCommand,
    InspectUnsupportedAppCommand, OperationSequenceAppCommand, ParityAppCommand, PathAppCommand,
    PcAppCommand, PercentAppCommand, RenAppCommand, RenderAppCommand, RulesAppCommand,
    ScenarioAppCommand, ScoringAppCommand, SequenceDependenciesAppCommand, SetupAppCommand,
    SetupScoreAppCommand, SpinFinderAppCommand, SpinStructureAppCommand, VerifyAppCommand,
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

pub(crate) trait RunnableAppCommand {
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
    SetupScore(SetupScoreAppCommand),
    BuildProbability(BuildProbabilityAppCommand),
    BuildV2(BuildV2AppCommand),
    Damage(DamageAppCommand),
    SpinFinder(SpinFinderAppCommand),
    Ren(RenAppCommand),
    SpinStructure(SpinStructureAppCommand),
    Cover(CoverAppCommand),
    Rules(RulesAppCommand),
    Scoring(ScoringAppCommand),
    Convert(ConvertAppCommand),
    Continue(ContinueAppCommand),
    InspectUnsupported(InspectUnsupportedAppCommand),
    Verify(VerifyAppCommand),
    VerifyKicks(VerifyAppCommand),
    UtilitySequence(OperationSequenceAppCommand),
    UtilitySequenceDependencies(SequenceDependenciesAppCommand),
    UtilityParity(ParityAppCommand),
    UtilityFumen(FumenAppCommand),
    UtilityRender(RenderAppCommand),
    UtilityToGray(FieldDocumentTransformAppCommand),
    UtilityMirror(FieldDocumentTransformAppCommand),
}

impl AppCommand {
    pub fn kind(&self) -> AppCommandKind {
        match self {
            Self::Pc(_) | Self::Scenario(_) => AppCommandKind::Pc,
            Self::Path(_) => AppCommandKind::Path,
            Self::Percent(_) => AppCommandKind::Percent,
            Self::Setup(_) | Self::SetupScore(_) => AppCommandKind::Setup,
            Self::BuildProbability(_) => AppCommandKind::BuildProbability,
            Self::BuildV2(_) => AppCommandKind::BuildProbability,
            Self::Damage(_) => AppCommandKind::Damage,
            Self::SpinFinder(_) => AppCommandKind::SpinFinder,
            Self::Ren(_) => AppCommandKind::Ren,
            Self::SpinStructure(_) => AppCommandKind::SpinStructure,
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
            Self::UtilitySequence(_) => AppCommandKind::UtilitySequence,
            Self::UtilitySequenceDependencies(_) => AppCommandKind::UtilitySequenceDependencies,
            Self::UtilityParity(_) => AppCommandKind::UtilityParity,
            Self::UtilityFumen(_) => AppCommandKind::UtilityFumen,
            Self::UtilityRender(_) => AppCommandKind::UtilityRender,
            Self::UtilityToGray(_) => AppCommandKind::UtilityToGray,
            Self::UtilityMirror(_) => AppCommandKind::UtilityMirror,
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
            Self::Setup(_) | Self::SetupScore(_) => QueryEnvelope::SetupSearch,
            Self::BuildProbability(_) => QueryEnvelope::BuildProbability,
            Self::BuildV2(_) => QueryEnvelope::BuildCoverage,
            Self::Damage(_) => QueryEnvelope::Damage,
            Self::SpinFinder(_) => QueryEnvelope::SpinFinder,
            Self::Ren(_) => QueryEnvelope::Ren,
            Self::SpinStructure(_) => QueryEnvelope::SpinStructure,
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
            Self::UtilitySequence(_) => QueryEnvelope::UtilitySequence,
            Self::UtilitySequenceDependencies(_) => QueryEnvelope::UtilitySequenceDependencies,
            Self::UtilityParity(_) => QueryEnvelope::UtilityParity,
            Self::UtilityFumen(_) => QueryEnvelope::UtilityFumen,
            Self::UtilityRender(_) => QueryEnvelope::UtilityRender,
            Self::UtilityToGray(_) => QueryEnvelope::UtilityToGray,
            Self::UtilityMirror(_) => QueryEnvelope::UtilityMirror,
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
            Self::BuildV2(_) | Self::SetupScore(_) => BackendPolicy::new("cpu", false),
            Self::Damage(_) | Self::SpinFinder(_) | Self::Ren(_) | Self::SpinStructure(_) => {
                BackendPolicy::new("cpu", false)
            }
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
            Self::BuildV2(_) => None,
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
            Self::BuildV2(_) | Self::SetupScore(_) => true,
            Self::Percent(_) => true,
            Self::Damage(_) | Self::SpinFinder(_) | Self::Ren(_) | Self::SpinStructure(_) => false,
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
            Self::SetupScore(_) => DiagnosticReport::new(),
            Self::BuildProbability(_) => DiagnosticReport::new(),
            Self::BuildV2(_) => DiagnosticReport::new(),
            Self::Damage(_) | Self::SpinFinder(_) | Self::Ren(_) | Self::SpinStructure(_) => {
                DiagnosticReport::new()
            }
            Self::Cover(command) => validate_build_coverage_query(command.query()),
            Self::Rules(command) => command.validate(),
            Self::Scoring(command) => command.validate(),
            Self::Convert(command) => command.validate(),
            Self::Continue(command) => command.validate(),
            Self::InspectUnsupported(command) => command.validate(),
            Self::Verify(command) => command.validate(),
            Self::VerifyKicks(command) => command.validate(),
            Self::UtilitySequence(_) => DiagnosticReport::new(),
            Self::UtilitySequenceDependencies(_) => DiagnosticReport::new(),
            Self::UtilityParity(_)
            | Self::UtilityFumen(_)
            | Self::UtilityRender(_)
            | Self::UtilityToGray(_)
            | Self::UtilityMirror(_) => DiagnosticReport::new(),
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
            Self::SetupScore(command) => command.run(context),
            Self::BuildProbability(command) => command.run(context),
            Self::BuildV2(command) => command.run(context),
            Self::Damage(command) => command.run(context),
            Self::SpinFinder(command) => command.run(context),
            Self::Ren(command) => command.run(context),
            Self::SpinStructure(command) => command.run(context),
            Self::Cover(command) => command.run(context),
            Self::Rules(command) => command.run(context),
            Self::Scoring(command) => command.run(context),
            Self::Convert(command) => command.run(context),
            Self::Continue(command) => command.run(context),
            Self::InspectUnsupported(command) => command.run(context),
            Self::Verify(command) => command.run(context),
            Self::VerifyKicks(command) => command.run(context),
            Self::UtilitySequence(command) => command.run(context),
            Self::UtilitySequenceDependencies(command) => command.run(context),
            Self::UtilityParity(command) => command.run(context),
            Self::UtilityFumen(command) => command.run(context),
            Self::UtilityRender(command) => command.run(context),
            Self::UtilityToGray(command) => command.run(context),
            Self::UtilityMirror(command) => command.run(context),
        }
    }
}
