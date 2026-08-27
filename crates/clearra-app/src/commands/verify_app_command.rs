use clearra_build_coverage::{
    domain::slot_domain::SlotDomain,
    query::{build_coverage_limits::BuildCoverageLimits, build_coverage_query::BuildCoverageQuery},
    template::{
        build_slot::{BuildSlot, BuildSlotId},
        build_template::BuildTemplate,
    },
};
use clearra_core_domain::{
    board::cell::CellCoord, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
};
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput};
use clearra_problem::{SetupLimits, SetupSearchQuery};
use clearra_rules::kicks::KickContractReport;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{string_field, CoverAppCommand, PcAppCommand, SetupAppCommand},
    render::{AppMessage, AppRenderModel, AppResultKind},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VerifyAppCommand {
    scope: Option<String>,
}

impl VerifyAppCommand {
    pub fn new(scope: impl Into<String>) -> Self {
        let scope = scope.into();
        Self {
            scope: (!scope.is_empty()).then_some(scope),
        }
    }
}
impl VerifyAppCommand {
    pub fn with_scope(scope: Option<String>) -> Self {
        Self { scope }
    }
}
impl VerifyAppCommand {
    pub fn kicks() -> Self {
        Self::new("kicks")
    }
}
impl VerifyAppCommand {
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }
}

impl RunnableAppCommand for VerifyAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        match self.scope.as_deref() {
            Some("pc") => scoped_verify("pc", PcAppCommand::new(default_pc_query()).run(context)),
            Some("setup") => scoped_verify(
                "setup",
                SetupAppCommand::new(default_setup_query()).run(context),
            ),
            Some("cover") | Some("build") => scoped_verify(
                "build",
                CoverAppCommand::new(default_cover_query()).run(context),
            ),
            Some("kicks") => verify_kicks(),
            Some(target) => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::VerifyTargetUnknown,
                    format!("unknown verify target '{target}'"),
                ),
            ),
            None => run_default_verify(context),
        }
    }
}

fn scoped_verify(scope: &str, probe: AppResponse) -> AppResponse {
    if probe.status() != AppStatus::Success {
        return probe;
    }
    let Some(result_kind) = probe.render_model().map(AppRenderModel::kind) else {
        return AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(
                AppErrorCode::ExecutionFailed,
                format!("verify probe '{scope}' returned no result"),
            ),
        );
    };
    verify_success(
        AppResultKind::Verify,
        vec![
            ("scope".to_owned(), scope.to_owned()),
            ("status".to_owned(), "verified".to_owned()),
            (
                "probe_result_kind".to_owned(),
                result_kind.as_str().to_owned(),
            ),
            ("probes_attempted".to_owned(), "1".to_owned()),
            ("probes_passed".to_owned(), "1".to_owned()),
            ("probes_failed".to_owned(), "0".to_owned()),
        ],
    )
}

fn run_default_verify(context: &AppExecutionContext<'_>) -> AppResponse {
    let kicks = KickContractReport::verify_builtin_contracts();
    if kicks.verification_failure_count() > 0 {
        return kick_failure(&kicks);
    }
    let outputs = [
        PcAppCommand::new(default_pc_query()).run(context),
        SetupAppCommand::new(default_setup_query()).run(context),
        CoverAppCommand::new(default_cover_query()).run(context),
    ];
    if let Some(failed) = outputs
        .into_iter()
        .find(|output| output.status() != AppStatus::Success)
    {
        return failed;
    }

    let mut fields = vec![
        ("pc".to_owned(), "ok".to_owned()),
        ("setup".to_owned(), "ok".to_owned()),
        ("build_coverage".to_owned(), "ok".to_owned()),
        ("kicks".to_owned(), "ok".to_owned()),
    ];
    fields.extend(kick_contract_summary_fields(&kicks));
    verify_success(AppResultKind::Verify, fields)
}

fn verify_kicks() -> AppResponse {
    let report = KickContractReport::verify_builtin_contracts();
    if report.verification_failure_count() > 0 {
        return kick_failure(&report);
    }
    verify_success(AppResultKind::VerifyKicks, kick_contract_fields(&report))
}

fn kick_failure(report: &KickContractReport) -> AppResponse {
    AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(
            AppErrorCode::VerifyKicksFailed,
            format!(
                "built-in kick verification failed with {} failure(s)",
                report.verification_failure_count()
            ),
        ),
    )
}

fn verify_success(kind: AppResultKind, fields: Vec<(String, String)>) -> AppResponse {
    AppResponse::success(AppRenderModel::Verify(AppMessage::new(
        kind,
        fields
            .into_iter()
            .map(|(key, value)| string_field(key, value))
            .collect(),
    )))
}

fn default_pc_query() -> OpeningPcSearchQuery {
    OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(PcHoldPolicy::Disabled)
}

fn default_setup_query() -> SetupSearchQuery {
    SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::I, PieceKind::I, PieceKind::O])
        .with_queue_based_pieces(vec![PieceKind::T, PieceKind::S, PieceKind::Z, PieceKind::J])
        .with_max_setup_pieces(1)
        .with_limits(SetupLimits::new(1, 1, 1, 1, 288, 1).expect("non-zero verify limits"))
}

fn default_cover_query() -> BuildCoverageQuery {
    let slot_id = BuildSlotId::new(0);
    BuildCoverageQuery::new(
        BuildTemplate::new(
            "cli-default",
            vec![BuildSlot::new(
                slot_id,
                vec![CellCoord::new_unchecked(0, 0)],
            )],
        ),
        vec![SlotDomain::new(slot_id, vec![PieceKind::I])],
        Vec::new(),
        1,
        BuildCoverageLimits::default(),
    )
}

fn kick_contract_summary_fields(report: &KickContractReport) -> Vec<(String, String)> {
    vec![
        (
            "kick_verification_cases".to_owned(),
            report.verification_case_count().to_string(),
        ),
        (
            "kick_verification_failures".to_owned(),
            report.verification_failure_count().to_string(),
        ),
    ]
}

fn kick_contract_fields(report: &KickContractReport) -> Vec<(String, String)> {
    let mut fields = vec![
        ("status".to_owned(), "verified".to_owned()),
        (
            "srs_jlstz_transitions".to_owned(),
            report.srs_jlstz_transition_count().to_string(),
        ),
        (
            "srs_i_transitions".to_owned(),
            report.srs_i_transition_count().to_string(),
        ),
        (
            "srs_profile_id".to_owned(),
            report.srs_profile_id().to_owned(),
        ),
        ("srs_o_model".to_owned(), report.o_piece_model().to_owned()),
        (
            "no_kick_transitions".to_owned(),
            report.no_kick_transition_count().to_string(),
        ),
        (
            "no_kick_profile_id".to_owned(),
            report.no_kick_profile_id().to_owned(),
        ),
        (
            "srs_plus_profile_id".to_owned(),
            report.srs_plus_profile_id().to_owned(),
        ),
        (
            "srs_plus_effective_kick_model".to_owned(),
            report.srs_plus_effective_kick_model().to_owned(),
        ),
        (
            "srs_plus_180_transitions".to_owned(),
            report.srs_plus_180_transition_count().to_string(),
        ),
        (
            "jstris_profile_id".to_owned(),
            report.jstris_profile_id().to_owned(),
        ),
        (
            "jstris_180_transitions".to_owned(),
            report.jstris_180_transition_count().to_string(),
        ),
        (
            "kick_profile_registry_count".to_owned(),
            report.profile_registry_count().to_string(),
        ),
        (
            "kick_verification_cases".to_owned(),
            report.verification_case_count().to_string(),
        ),
        (
            "kick_verification_failures".to_owned(),
            report.verification_failure_count().to_string(),
        ),
    ];
    if let Some(reason) = report.srs_plus_extension_reason() {
        fields.push(("srs_plus_extension_reason".to_owned(), reason.to_owned()));
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_probe_success_is_wrapped_in_the_verify_result_contract() {
        let probe = AppResponse::success(AppRenderModel::Path(AppMessage::new(
            AppResultKind::Path,
            Vec::new(),
        )));

        let response = scoped_verify("pc", probe);

        assert_eq!(response.status(), AppStatus::Success);
        let model = response.render_model().expect("verify render model");
        assert_eq!(model.kind(), AppResultKind::Verify);
        let fields = model.message().expect("verify message").fields();
        let field = |key: &str| {
            fields
                .iter()
                .find(|field| field.key() == key)
                .map(|field| field.value().as_text())
        };
        assert_eq!(field("scope").as_deref(), Some("pc"));
        assert_eq!(field("status").as_deref(), Some("verified"));
        assert_eq!(field("probe_result_kind").as_deref(), Some("path"));
        assert_eq!(field("probes_attempted").as_deref(), Some("1"));
        assert_eq!(field("probes_passed").as_deref(), Some("1"));
        assert_eq!(field("probes_failed").as_deref(), Some("0"));
    }

    #[test]
    fn scoped_probe_failure_preserves_the_underlying_failure() {
        let probe = AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::NoSolution, "fixture failure"),
        );

        let response = scoped_verify("pc", probe);

        assert_eq!(response.status(), AppStatus::ExecutionFailed);
        assert_eq!(
            response.error().map(AppError::code),
            Some(AppErrorCode::NoSolution)
        );
        assert!(response.render_model().is_none());
    }
}
