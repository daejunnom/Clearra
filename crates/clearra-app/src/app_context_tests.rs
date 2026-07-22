use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_core_executor::{native_core_runtime_available, CoreExecutionResult};
use clearra_i18n::LanguageId;
use clearra_output::model::RenderFieldValue;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PieceWindow, RequestedSearchBackend,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::{
    commands::{PcAppCommand, RulesAppCommand, ScenarioAppCommand},
    io::AppFilePolicy,
    AppCommand, AppCommandKind, AppContext, AppErrorCode, AppMessage, AppOutputPolicy,
    AppRenderModel, AppRequest, AppResponse, AppServices, AppStatus, QueryEnvelope,
};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

fn field_text(message: &AppMessage, key: &str) -> Option<String> {
    message
        .fields()
        .iter()
        .find(|field| field.key() == key)
        .map(|field| match field.value() {
            RenderFieldValue::String(value) | RenderFieldValue::Number(value) => value.clone(),
            RenderFieldValue::Bool(value) => value.to_string(),
            value => value.as_text(),
        })
}

fn resource_response_from_fields(fields: Vec<(String, String)>) -> AppResponse {
    AppResponse::success(AppRenderModel::Scenario(CoreExecutionResult::new(
        fields,
        Vec::new(),
    )))
}

fn field(key: &str, value: &str) -> (String, String) {
    (key.to_owned(), value.to_owned())
}

mod case_app_pc_request_runs_without_cli_parser {
    use super::*;

    #[test]
    fn app_pc_request_runs_without_cli_parser() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])),
        );

        let response =
            AppContext::default().run(AppRequest::new(AppCommand::Pc(PcAppCommand::new(query))));

        if !native_core_runtime_available() {
            assert_eq!(response.status(), AppStatus::Unsupported);
            assert!(response.result().is_none());
            assert!(response.render_model().is_none());
            assert_eq!(
                response.error().map(|error| error.code()),
                Some(AppErrorCode::NativeCoreUnavailable)
            );
            assert_eq!(response.backend_report().backend_selected(), "none");
            assert!(!response.backend_report().fallback_used());
            assert!(!response.resource_report().solver_executed());
            assert!(!response.resource_report().probability_complete());
            assert!(response
                .diagnostics()
                .validation()
                .contains_code(DiagnosticCode::ENativeCoreUnavailable));
            return;
        }

        assert_eq!(response.status(), AppStatus::Success);
        let Some(AppRenderModel::Pc(result)) = response.render_model() else {
            panic!("pc render model");
        };
        assert_eq!(result.field("route"), Some("search-problem-core-executor"));
        assert_eq!(result.field("problem_layer"), Some("clearra-problem"));
        assert_eq!(
            result.field("executor_layer"),
            Some("clearra-core-executor")
        );
    }
}

mod case_app_scenario_request_runs_without_cli_parser {
    use super::*;

    #[test]
    fn app_scenario_request_runs_without_cli_parser() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_retained_trace_limit(1);

        let response = AppContext::default().run(AppRequest::new(AppCommand::Scenario(
            ScenarioAppCommand::new(query),
        )));

        if !native_core_runtime_available() {
            assert_eq!(response.status(), AppStatus::Unsupported);
            assert!(response.result().is_none());
            assert!(response.render_model().is_none());
            assert_eq!(response.backend_report().backend_selected(), "none");
            assert!(!response.backend_report().fallback_used());
            assert!(!response.resource_report().solver_executed());
            assert!(!response.resource_report().probability_complete());
            return;
        }

        assert_eq!(response.status(), AppStatus::Success);
        let Some(AppRenderModel::Scenario(result)) = response.render_model() else {
            panic!("scenario render model");
        };
        assert_eq!(result.field("solution_found"), Some("true"));
        assert_eq!(
            result.field("total_solution_count"),
            Some(if native_core_runtime_available() {
                "2"
            } else {
                "1"
            })
        );
    }
}

mod case_app_response_contains_diagnostics_and_render_model {
    use super::*;

    #[test]
    fn app_response_contains_diagnostics_and_render_model() {
        let valid_query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
        let valid = AppContext::default().run(AppRequest::new(AppCommand::Pc(PcAppCommand::new(
            valid_query,
        ))));
        if native_core_runtime_available() {
            assert_eq!(valid.status(), AppStatus::Success);
            assert!(valid.render_model().is_some());
            assert!(!valid.diagnostics().has_errors());
            assert!(!valid.resource_report().truncated());
        } else {
            assert_eq!(valid.status(), AppStatus::Unsupported);
            assert!(valid.result().is_none());
            assert!(valid.render_model().is_none());
            assert!(valid.error().is_some());
        }

        let invalid =
            AppContext::default().run(AppRequest::new(AppCommand::Pc(PcAppCommand::new(
                OpeningPcSearchQuery::new(PcTarget::new(8).expect("8L target")),
            ))));
        assert_eq!(invalid.status(), AppStatus::ValidationFailed);
        assert!(invalid.render_model().is_none());
        assert!(invalid.diagnostics().has_errors());
    }
}

mod case_app_request_runs_validation_before_execution {
    use super::*;

    #[test]
    fn app_request_runs_validation_before_execution() {
        let request = AppRequest::new(AppCommand::Pc(PcAppCommand::new(
            OpeningPcSearchQuery::new(PcTarget::new(8).expect("8L target")),
        )));

        let report = AppContext::default().validate_request(&request);
        assert!(report.has_errors());

        let response = AppContext::default().run(request);
        assert_eq!(response.status(), AppStatus::ValidationFailed);
        assert!(response.render_model().is_none());
    }
}

mod case_app_request_contract_exposes_command_query_and_policy {
    use super::*;

    #[test]
    fn app_request_contract_exposes_command_query_and_policy() {
        let request = AppRequest::new(AppCommand::Pc(PcAppCommand::new(
            OpeningPcSearchQuery::new(PcTarget::two_lines()),
        )));

        assert_eq!(request.command_kind(), AppCommandKind::Pc);
        assert_eq!(request.query(), &QueryEnvelope::PcOpening);
        assert_eq!(request.backend_policy().backend_requested(), "auto");
        assert!(request.backend_policy().allow_backend_fallback());
        assert!(request.output_policy().include_render_model());
        assert_eq!(request.output_policy().contract().format(), "text");
        assert!(request.diagnostics_policy().include_diagnostics());
        assert!(!request.diagnostics_policy().fail_on_warnings());
        assert_eq!(request.locale_policy().language(), None);
        assert_eq!(request.resource_budget().workers(), 1);
    }
}

mod case_app_response_contract_contains_command_result_and_reports {
    use super::*;

    #[test]
    fn app_response_contract_contains_command_result_and_reports() {
        let response = AppContext::default().run(AppRequest::new(AppCommand::Rules(
            RulesAppCommand::new("list"),
        )));

        assert_eq!(response.status(), AppStatus::Success);
        assert_eq!(response.command(), Some(AppCommandKind::Rules));
        assert_eq!(response.result().map(|result| result.kind()), Some("rules"));
        assert_eq!(response.backend_report().backend_selected(), "none");
        assert!(!response.backend_report().fallback_used());
        assert!(response.resource_report().solver_executed());
        assert_eq!(
            response.capability_report().app_request_boundary(),
            "clearra-app/AppRequest"
        );
        assert!(response.continuation().is_none());
    }
}

mod case_verify_kicks_uses_distinct_app_command_kind {
    use super::*;

    #[test]
    fn verify_kicks_uses_distinct_app_command_kind() {
        let request = AppRequest::new(AppCommand::VerifyKicks(crate::VerifyAppCommand::kicks()));

        assert_eq!(request.command_kind(), AppCommandKind::VerifyKicks);
        assert_eq!(request.query(), &QueryEnvelope::VerifyKicks);

        let response = AppContext::default().run(request);
        assert_eq!(response.command(), Some(AppCommandKind::VerifyKicks));
        assert_eq!(response.status(), AppStatus::Success);
    }
}

mod case_diagnostic_error_prevents_execution {
    use super::*;

    #[test]
    fn diagnostic_error_prevents_execution() {
        let response =
            AppContext::default().run(AppRequest::new(AppCommand::Pc(PcAppCommand::new(
                OpeningPcSearchQuery::new(PcTarget::new(8).expect("8L target")),
            ))));

        assert_eq!(response.status(), AppStatus::ValidationFailed);
        assert_eq!(response.command(), Some(AppCommandKind::Pc));
        assert!(response.result().is_none());
        assert!(!response.resource_report().solver_executed());
        assert!(response.diagnostics().has_errors());
        assert!(response.render_model().is_none());
    }
}

mod case_warning_allows_execution {
    use super::*;

    #[test]
    fn warning_allows_execution() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_backend(RequestedSearchBackend::Gpu)
                .with_allow_backend_fallback(true),
        );

        let response = AppContext::default().run(AppRequest::new(AppCommand::Scenario(
            ScenarioAppCommand::new(query),
        )));

        assert_eq!(
            response.status(),
            if native_core_runtime_available() {
                AppStatus::Success
            } else {
                AppStatus::Unsupported
            }
        );
        if native_core_runtime_available() {
            assert!(
                !response.diagnostics().has_errors(),
                "unexpected diagnostics: {:#?}",
                response.diagnostics()
            );
        } else {
            assert!(response
                .diagnostics()
                .validation()
                .contains_code(DiagnosticCode::ENativeCoreUnavailable));
        }
        assert!(!response.diagnostics().validation().is_empty());
        if native_core_runtime_available() {
            assert!(response.render_model().is_some());
        } else {
            assert!(response.result().is_none());
            assert!(response.render_model().is_none());
        }
    }
}

mod case_app_context_runs_real_rules_command_through_app_boundary {
    use super::*;

    #[test]
    fn app_context_runs_real_rules_command_through_app_boundary() {
        let response = AppContext::default()
            .with_language(LanguageId::Ko)
            .with_file_policy(AppFilePolicy::new(true))
            .run(AppRequest::new(AppCommand::Rules(RulesAppCommand::new(
                "list",
            ))));

        let Some(AppRenderModel::Rules(message)) = response.render_model() else {
            panic!("rules render model");
        };
        assert_eq!(field_text(message, "action"), Some("list".to_owned()));
        assert!(field_text(message, "profile_count").is_some());
    }
}

mod case_app_services_exposes_real_di_slots {
    use super::*;

    #[test]
    fn app_services_exposes_real_di_slots() {
        let services = AppServices::default();

        assert_eq!(
            services.core_executor().service_name(),
            "clearra-core-executor"
        );
        assert_eq!(services.file_resolver().service_name(), "app-file-resolver");
        assert_eq!(
            services.language_resolver().service_name(),
            "clearra-i18n-language-resolver"
        );
        assert_eq!(services.clock().service_name(), "system-clock");
        assert_eq!(
            services.diagnostic_sink().service_name(),
            "app-diagnostic-sink"
        );

        let resolver = services.file_resolver_for(&AppFilePolicy::new(true));
        assert!(resolver.policy().verbose_paths());
    }
}

mod case_app_request_overrides_context_language_and_file_policy {
    use super::*;

    #[test]
    fn app_request_overrides_context_language_and_file_policy() {
        let request = AppRequest::new(AppCommand::Rules(RulesAppCommand::new("list")))
            .with_language(LanguageId::En)
            .with_file_policy(AppFilePolicy::new(false));
        let response = AppContext::default()
            .with_language(LanguageId::Ko)
            .with_file_policy(AppFilePolicy::new(true))
            .run(request);

        let Some(AppRenderModel::Rules(message)) = response.render_model() else {
            panic!("rules render model");
        };
        assert_eq!(field_text(message, "action"), Some("list".to_owned()));
    }
}

mod case_app_output_policy_can_suppress_render_model {
    use super::*;

    #[test]
    fn app_output_policy_can_suppress_render_model() {
        let response = AppContext::default().run(
            AppRequest::new(AppCommand::Rules(RulesAppCommand::new("list")))
                .with_output_policy(AppOutputPolicy::new(false)),
        );

        assert_eq!(response.status(), AppStatus::Success);
        assert!(response.render_model().is_none());
    }
}

mod case_packing_frontier_budget_sets_app_response_incomplete {
    use super::*;

    #[test]
    fn packing_frontier_budget_sets_app_response_incomplete() {
        let response = resource_response_from_fields(vec![
            field("resource_truncated", "true"),
            field("resource_truncation_reason", "frontier_budget_exceeded"),
            field("resource_probability_complete", "false"),
            field("count_complete", "false"),
            field("count_truncated_reason", "frontier_budget_exceeded"),
            field("probability_complete", "false"),
        ]);

        assert!(response.resource_report().truncated());
        assert_eq!(
            response.resource_report().truncation_reason(),
            Some("frontier_budget_exceeded")
        );
        assert!(!response.resource_report().probability_complete());
        assert!(response
            .diagnostics()
            .validation()
            .contains_code(DiagnosticCode::ECorePackingFailed));
    }
}

mod case_buildup_variant_budget_sets_count_complete_false {
    use super::*;

    #[test]
    fn buildup_variant_budget_sets_count_complete_false() {
        let response = resource_response_from_fields(vec![
            field("resource_truncated", "true"),
            field(
                "resource_truncation_reason",
                "buildup_enumeration_truncated",
            ),
            field("resource_probability_complete", "false"),
            field("count_complete", "false"),
            field("count_truncated_reason", "buildup_enumeration_truncated"),
            field("probability_complete", "false"),
        ]);

        assert!(response.resource_report().truncated());
        assert!(!response.resource_report().probability_complete());
        assert!(response
            .diagnostics()
            .validation()
            .contains_code(DiagnosticCode::WBuildUpEnumerationTruncated));
    }
}

mod case_coverage_capacity_sets_probability_complete_false {
    use super::*;

    #[test]
    fn coverage_capacity_sets_probability_complete_false() {
        let response = resource_response_from_fields(vec![
            field("resource_truncated", "true"),
            field("resource_truncation_reason", "coverage_rows_exceeded"),
            field("resource_probability_complete", "false"),
            field("count_complete", "false"),
            field("probability_complete", "false"),
        ]);

        assert!(response.resource_report().truncated());
        assert!(!response.resource_report().probability_complete());
        assert!(response
            .diagnostics()
            .validation()
            .contains_code(DiagnosticCode::ECoverageCapacityExceeded));
    }
}

mod case_observed_universe_truncation_not_renormalized {
    use super::*;

    #[test]
    fn observed_universe_truncation_not_renormalized() {
        let response = resource_response_from_fields(vec![
            field("queue_mode", "observed"),
            field("supply_expansion_truncated", "true"),
            field("supply_probability_complete", "false"),
            field("supply_materialized_probability_mass", "0.5"),
            field("probability_complete", "false"),
            field("count_complete", "true"),
        ]);

        assert!(response.resource_report().truncated());
        assert_eq!(
            response.resource_report().truncation_reason(),
            Some("observed_universe_truncated")
        );
        assert!(!response.resource_report().probability_complete());
        assert!(response
            .diagnostics()
            .validation()
            .contains_code(DiagnosticCode::WObservedQueueProbabilityIncomplete));
    }
}

mod case_resource_cap_never_returns_complete_probability {
    use super::*;

    #[test]
    fn resource_cap_never_returns_complete_probability() {
        let response = resource_response_from_fields(vec![
            field("resource_truncated", "true"),
            field("resource_truncation_reason", "candidate_budget_exceeded"),
            field("resource_probability_complete", "true"),
            field("count_complete", "true"),
            field("probability_complete", "true"),
        ]);

        assert!(response.resource_report().truncated());
        assert!(!response.resource_report().probability_complete());
    }
}
