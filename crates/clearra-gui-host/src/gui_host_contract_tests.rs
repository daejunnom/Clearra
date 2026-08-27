use crate::{
    DesktopTauriCommandBridge, GuiAppState, GuiBackendChoice, GuiBackendForm, GuiExecutionPhase,
    GuiExecutionState, GuiHostLanguageResolver, GuiJobId, GuiOpeningPcForm, GuiOutputFormat,
    GuiProblemForm, GuiScenarioPcForm, GuiScreen, PcRequestBuilder, ScenarioRequestBuilder,
    SetupRequestBuilder,
};
use serde_json::Value;

fn assert_product_runtime_identity(value: &Value) {
    let expected = clearra_host_contract::ProductBuildIdentity::current();
    let identity = value.as_object().expect("desktop runtime_identity object");

    assert_eq!(identity.len(), 5);
    assert_eq!(identity["engine_build_id"], expected.engine_build_id());
    assert_eq!(identity["source_commit"], expected.source_commit());
    assert_eq!(
        identity["contract_schema_version"],
        expected.contract_schema_version()
    );
    assert_eq!(
        identity["supply_semantics_id"],
        expected.supply_semantics_id()
    );
    assert_eq!(
        identity["artifact_schema_version"],
        expected.artifact_schema_version()
    );
}

#[test]
fn gui_state_binds_verified_structural_profiles_into_the_app_request() {
    let state = GuiAppState::default().with_problem_form(GuiProblemForm::OpeningPc(
        GuiOpeningPcForm::new(2, "srs-plus"),
    ));
    let request = crate::GuiToAppRequest::build(&state)
        .expect("canonical GUI request")
        .into_app_request();
    let profiles = request.request_profiles();
    assert_eq!(profiles.board().as_str(), "standard-10");
    assert_eq!(profiles.piece_set().as_str(), "standard-tetrominoes");
    assert_eq!(profiles.bag().as_str(), "standard-7-bag");
    assert_eq!(profiles.rule().as_str(), "srs-plus");
}

#[test]
fn gui_state_rejects_an_unverified_rule_profile_without_fallback() {
    let state = GuiAppState::default().with_problem_form(GuiProblemForm::OpeningPc(
        GuiOpeningPcForm::new(2, "custom"),
    ));
    let error = crate::GuiToAppRequest::build(&state)
        .expect_err("unverified GUI rule must fail closed at App authority");
    assert!(error.message().contains("profile"));
}

mod case_gui_pc_request_preserves_back_to_back_constraint {
    use clearra_app::AppCommand;

    use super::*;

    #[test]
    fn gui_pc_request_preserves_back_to_back_constraint() {
        let form = GuiOpeningPcForm::new(2, "srs-plus")
            .with_score_profiles("tetrio", "all-mini-plus")
            .with_back_to_back_preservation(true);
        let command = PcRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI PC request");
        let AppCommand::Pc(command) = command else {
            panic!("expected PC command");
        };
        let constraint = command.query().objective().execution_constraints();

        assert!(constraint.preserves_back_to_back());
        assert_eq!(constraint.spin_profile().as_str(), "all-mini-plus");
        assert!(!command.query().objective().score().requested());
    }
}

mod case_gui_failed_queue_request_uses_coverage_without_scoring {
    use clearra_app::AppCommand;
    use clearra_core_domain::objective::objective_kind::ObjectiveKind;

    use super::*;

    #[test]
    fn gui_failed_queue_request_uses_coverage_without_scoring() {
        let form = GuiOpeningPcForm::new(2, "srs-plus")
            .with_score_input("failed-queue", 0)
            .with_score_profiles("tetrio", "all-mini-plus")
            .with_back_to_back_preservation(true);
        let command = PcRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI failed-queue request");
        let AppCommand::Percent(command) = command else {
            panic!("expected percent-backed failed-queue command");
        };
        assert!(command.is_failed_queue());
        assert!(command.query().is_none());
        let objective = command
            .opening_query()
            .expect("opening failed-queue query")
            .objective();
        assert_eq!(objective.kind(), ObjectiveKind::All);
        assert!(!objective.score().requested());
        assert!(objective.execution_constraints().preserves_back_to_back());
        assert_eq!(
            objective.execution_constraints().spin_profile().as_str(),
            "all-mini-plus"
        );
    }
}

mod case_gui_pc_request_preserves_dependency_dag_policy {
    use clearra_app::AppCommand;

    use super::*;

    #[test]
    fn gui_pc_request_preserves_dependency_dag_policy() {
        let backend = GuiBackendForm::default().with_precompute_build_dependencies(true);
        let command =
            PcRequestBuilder::build_command(&GuiOpeningPcForm::new(2, "srs-plus"), &backend)
                .expect("GUI PC request");
        let AppCommand::Pc(command) = command else {
            panic!("expected PC command");
        };

        assert!(command
            .query()
            .execution_policy()
            .precompute_build_dependencies());
        let default_command = PcRequestBuilder::build_command(
            &GuiOpeningPcForm::new(2, "srs-plus"),
            &GuiBackendForm::default(),
        )
        .expect("default GUI PC request");
        let AppCommand::Pc(default_command) = default_command else {
            panic!("expected default PC command");
        };
        assert!(!default_command
            .query()
            .execution_policy()
            .precompute_build_dependencies());
    }
}

mod case_gui_opening_pc_preserves_hold_selection_without_queue_override {
    use clearra_app::AppCommand;

    use super::*;

    #[test]
    fn gui_opening_pc_preserves_hold_selection_without_queue_override() {
        let form = GuiOpeningPcForm::new(2, "srs-plus").with_hold_enabled(false);
        let command = PcRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI PC request");
        let AppCommand::Pc(command) = command else {
            panic!("expected PC command");
        };

        assert!(!command.query().hold_policy().is_enabled());
        assert_eq!(command.query().queue().mode(), "standard-7-bag");
    }
}

mod case_gui_pc_request_preserves_visible_seven_queue_knowledge {
    use clearra_app::AppCommand;
    use clearra_supply::QueueObservationPolicy;

    use super::*;

    #[test]
    fn gui_pc_request_preserves_visible_seven_queue_knowledge() {
        let form = GuiOpeningPcForm::new(4, "srs-plus")
            .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
        let command = PcRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI PC request");
        let AppCommand::Pc(command) = command else {
            panic!("expected PC command");
        };

        assert_eq!(
            command.query().queue_observation_policy(),
            QueueObservationPolicy::VisibleSeven
        );
    }
}

mod case_gui_pc_tiling_authority_boundary {
    use clearra_app::{
        AppCommand, PcResultProjection, PcTilingIngressOrigin, ProductCapabilityContract,
    };
    use clearra_supply::QueueObservationPolicy;

    use super::*;
    use crate::GuiToAppRequest;

    fn tiling_form() -> GuiOpeningPcForm {
        GuiOpeningPcForm::new(2, "srs-plus").with_score_input("tiling", 0)
    }

    #[test]
    fn canonical_gui_pc_tiling_attaches_projection_and_product_contract() {
        let state =
            GuiAppState::default().with_problem_form(GuiProblemForm::OpeningPc(tiling_form()));
        let request = GuiToAppRequest::build(&state)
            .expect("canonical GUI pc tiling request")
            .into_app_request();

        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcTiling)
        );
        let AppCommand::Pc(command) = request.command() else {
            panic!("expected opening PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::TilingFamilyV1(PcTilingIngressOrigin::CanonicalPcTiling)
        );
    }

    #[test]
    fn generic_tiling_objective_does_not_gain_typed_pc_tiling_authority() {
        let state = GuiAppState::default().with_problem_form(GuiProblemForm::OpeningPc(
            GuiOpeningPcForm::new(2, "srs-plus").with_score_input("tiling-only", 0),
        ));
        let request = GuiToAppRequest::build(&state)
            .expect("generic GUI tiling objective")
            .into_app_request();

        assert_eq!(request.product_capability_contract(), None);
        let AppCommand::Pc(command) = request.command() else {
            panic!("expected opening PC command");
        };
        assert_eq!(command.result_projection(), PcResultProjection::Standard);
    }

    #[test]
    fn canonical_gui_pc_tiling_rejects_inactive_semantics() {
        let invalid_forms = [
            GuiOpeningPcForm::new(2, "srs").with_score_input("tiling", 0),
            tiling_form().with_score_profiles("guideline", "t-spins"),
            tiling_form().with_score_profiles("tetrio", "all-spin"),
            GuiOpeningPcForm::new(2, "srs-plus").with_score_input("tiling", 1),
            tiling_form().with_back_to_back_preservation(true),
            tiling_form().with_solution_probabilities(true),
            tiling_form().with_queue_observation_policy(QueueObservationPolicy::VisibleSeven),
        ];
        for form in invalid_forms {
            let error = PcRequestBuilder::build_command(&form, &GuiBackendForm::default())
                .expect_err("inactive pc tiling semantics must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);
        }

        for backend in [
            GuiBackendForm::default().with_tablebase_requested(true),
            GuiBackendForm::default().with_precompute_build_dependencies(true),
        ] {
            let error = PcRequestBuilder::build_command(&tiling_form(), &backend)
                .expect_err("inactive pc tiling backend semantics must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);
        }
    }
}

mod case_gui_pc_portfolio_authority_boundary {
    use clearra_app::{
        AppCommand, PcMinimalsIngressOrigin, PcResultProjection, PcScoreIngressOrigin,
        PcScoreMinimalsIngressOrigin, ProductCapabilityContract, PC_SCORE_MAX_PATTERNS,
    };
    use clearra_core_domain::objective::objective_kind::ObjectiveKind;
    use clearra_pc_graph::request::{PcCountPolicy, RequestedSearchBackend, WorkerPolicy};

    use super::*;
    use crate::GuiToAppRequest;

    fn scenario_form(mode: &str) -> GuiScenarioPcForm {
        GuiScenarioPcForm::new(1, 0x3f, "I", "srs-plus")
            .with_execution_input(1, None, true, "all")
            .with_score_input(mode, 0)
    }

    fn score_finder_form() -> GuiScenarioPcForm {
        scenario_form("score-finder").with_score_profiles("jstris-ultra", "t-spins")
    }

    #[test]
    fn canonical_gui_pc_minimals_attaches_exact_projection_and_unique_count() {
        let state = GuiAppState::default()
            .with_problem_form(GuiProblemForm::ScenarioPc(scenario_form("minimum-cover")));
        let request = GuiToAppRequest::build(&state)
            .expect("canonical GUI pc minimals request")
            .into_app_request();

        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcMinimals)
        );
        let AppCommand::Scenario(command) = request.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals)
        );
        assert_eq!(command.query().count_policy(), PcCountPolicy::CountUnique);
    }

    #[test]
    fn canonical_gui_pc_score_owns_the_fixed_cpu_single_session_policy() {
        let state = GuiAppState::default()
            .with_problem_form(GuiProblemForm::ScenarioPc(scenario_form("summary")));
        let request = GuiToAppRequest::build(&state)
            .expect("canonical GUI pc score request")
            .into_app_request();

        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcScore)
        );
        let AppCommand::Scenario(command) = request.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScore)
        );
        let policy = command.query().execution_policy();
        assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
        assert_eq!(policy.worker_policy(), WorkerPolicy::Fixed(1));
        assert_eq!(policy.workers(), 1);
        assert!(!policy.allow_backend_fallback());
        assert_eq!(policy.max_patterns(), PC_SCORE_MAX_PATTERNS);
    }

    #[test]
    fn canonical_gui_pc_score_finder_attaches_the_fixed_witness_contract() {
        let state = GuiAppState::default()
            .with_problem_form(GuiProblemForm::ScenarioPc(score_finder_form()));
        let request = GuiToAppRequest::build(&state)
            .expect("canonical GUI pc score-finder request")
            .into_app_request();

        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcScoreFinder)
        );
        let AppCommand::Scenario(command) = request.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::ScoreSummaryV2(PcScoreIngressOrigin::CanonicalPcScoreFinder)
        );
        assert_eq!(command.query().remaining_queue().mode(), "fixed");
        assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
        assert_eq!(command.query().retained_trace_limit(), 1);
        assert_eq!(
            command.query().objective().score().profile().as_str(),
            "jstris-ultra"
        );
        assert_eq!(
            command.query().objective().score().spin_profile().as_str(),
            "t-spins"
        );
        let policy = command.query().execution_policy();
        assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
        assert_eq!(policy.worker_policy(), WorkerPolicy::Fixed(1));
        assert!(!policy.allow_backend_fallback());
        assert_eq!(policy.max_patterns(), PC_SCORE_MAX_PATTERNS);
    }

    #[test]
    fn canonical_gui_pc_score_finder_rejects_opening_and_nonfixed_or_inactive_inputs() {
        let opening = GuiOpeningPcForm::new(2, "srs-plus")
            .with_score_input("score-finder", 0)
            .with_score_profiles("jstris-ultra", "t-spins");
        let error = PcRequestBuilder::build_command(&opening, &GuiBackendForm::default())
            .expect_err("score-finder must require a scenario board");
        assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);

        let invalid_forms = [
            GuiScenarioPcForm::new(1, 0x3f, "", "srs-plus")
                .with_execution_input(1, None, true, "all")
                .with_score_input("score-finder", 0)
                .with_score_profiles("jstris-ultra", "t-spins"),
            score_finder_form().with_queue_pattern(),
            score_finder_form().with_standard_7_bag(),
            scenario_form("score-finder"),
            score_finder_form().with_score_input("score-finder", 2),
            score_finder_form().with_back_to_back_preservation(true),
        ];
        for form in invalid_forms {
            let error = ScenarioRequestBuilder::build_command(&form, &GuiBackendForm::default())
                .expect_err("noncanonical pc score-finder input must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);
        }

        for backend in [
            GuiBackendForm::new(GuiBackendChoice::Gpu),
            GuiBackendForm::default().with_workers(2),
            GuiBackendForm::default().with_tablebase_requested(true),
        ] {
            let error = ScenarioRequestBuilder::build_command(&score_finder_form(), &backend)
                .expect_err("pc score-finder execution override must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);
        }
    }

    #[test]
    fn canonical_gui_pc_score_minimals_binds_score_only_minimum_cover_and_fixed_execution() {
        let state = GuiAppState::default()
            .with_problem_form(GuiProblemForm::ScenarioPc(scenario_form("score-minimals")));
        let request = GuiToAppRequest::build(&state)
            .expect("canonical GUI pc score-minimals request")
            .into_app_request();

        assert_eq!(
            request.product_capability_contract(),
            Some(ProductCapabilityContract::PcScoreMinimals)
        );
        let AppCommand::Scenario(command) = request.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(
            command.result_projection(),
            PcResultProjection::ScorePortfolioV2(
                PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals
            )
        );
        assert_eq!(
            command.query().objective().kind(),
            ObjectiveKind::MinimumCover
        );
        assert!(command.query().objective().score().requested());
        assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
        assert_eq!(command.query().retained_trace_limit(), 1);
        let policy = command.query().execution_policy();
        assert_eq!(policy.requested_backend(), RequestedSearchBackend::Cpu);
        assert_eq!(policy.worker_policy(), WorkerPolicy::Fixed(1));
        assert!(!policy.allow_backend_fallback());
        assert_eq!(policy.max_patterns(), PC_SCORE_MAX_PATTERNS);
    }

    #[test]
    fn generic_minimum_alias_does_not_gain_pc_minimals_product_authority() {
        let state = GuiAppState::default()
            .with_problem_form(GuiProblemForm::ScenarioPc(scenario_form("minimum")));
        let request = GuiToAppRequest::build(&state)
            .expect("generic GUI minimum-cover objective")
            .into_app_request();

        assert_eq!(request.product_capability_contract(), None);
        let AppCommand::Scenario(command) = request.command() else {
            panic!("expected scenario PC command");
        };
        assert_eq!(command.result_projection(), PcResultProjection::Standard);
    }

    #[test]
    fn canonical_gui_portfolio_modes_reject_unaccounted_execution_overrides() {
        for backend in [
            GuiBackendForm::default().with_memory_budget_mb(64),
            GuiBackendForm::default().with_tablebase_requested(true),
            GuiBackendForm::default().with_precompute_build_dependencies(true),
        ] {
            let error =
                ScenarioRequestBuilder::build_command(&scenario_form("minimum-cover"), &backend)
                    .expect_err("pc minimals inactive execution override must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);
        }

        for backend in [
            GuiBackendForm::new(GuiBackendChoice::Gpu),
            GuiBackendForm::default().with_workers(2),
            GuiBackendForm::default().with_use_all_logical_processors(true),
        ] {
            let error = ScenarioRequestBuilder::build_command(&scenario_form("summary"), &backend)
                .expect_err("pc score execution override must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);

            let error =
                ScenarioRequestBuilder::build_command(&scenario_form("score-minimals"), &backend)
                    .expect_err("pc score-minimals execution override must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);
        }

        for form in [
            scenario_form("score-minimals").with_back_to_back_preservation(true),
            scenario_form("score-minimals").with_solution_probabilities(true),
        ] {
            let error = ScenarioRequestBuilder::build_command(&form, &GuiBackendForm::default())
                .expect_err("pc score-minimals semantic override must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);
        }
    }
}

mod case_gui_pc_save_authority_boundary {
    use clearra_app::{
        AppCommand, PcResultProjection, PcSaveIngressOrigin, ProductCapabilityContract,
    };
    use clearra_core_domain::objective::objective_kind::ObjectiveKind;
    use clearra_pc_graph::request::PcCountPolicy;
    use clearra_supply::QueueObservationPolicy;

    use super::*;
    use crate::GuiToAppRequest;

    fn save_form(mode: &str) -> GuiScenarioPcForm {
        GuiScenarioPcForm::new(2, 0xf3fcf, "", "srs-plus")
            .with_standard_7_bag()
            .with_execution_input(1, None, false, "all")
            .with_score_input(mode, 0)
    }

    #[test]
    fn canonical_gui_pc_save_modes_attach_distinct_contracts_and_fixed_semantics() {
        for (mode, contract, projection) in [
            (
                "saves",
                ProductCapabilityContract::PcSaves,
                PcResultProjection::SaveGroupsV2(PcSaveIngressOrigin::CanonicalPcSaves),
            ),
            (
                "best-save",
                ProductCapabilityContract::PcBestSave,
                PcResultProjection::BestSaveV2(PcSaveIngressOrigin::CanonicalPcBestSave),
            ),
        ] {
            let state = GuiAppState::default()
                .with_problem_form(GuiProblemForm::ScenarioPc(save_form(mode)));
            let request = GuiToAppRequest::build(&state)
                .expect("canonical GUI pc save request")
                .into_app_request();

            assert_eq!(request.product_capability_contract(), Some(contract));
            let AppCommand::Scenario(command) = request.command() else {
                panic!("expected scenario PC command");
            };
            assert_eq!(command.result_projection(), projection);
            assert_eq!(command.query().objective().kind(), ObjectiveKind::All);
            assert_eq!(command.query().count_policy(), PcCountPolicy::CountAll);
            assert_eq!(command.query().retained_trace_limit(), 1);
            assert_eq!(command.query().remaining_queue().mode(), "standard-7-bag");
        }
    }

    #[test]
    fn canonical_gui_pc_save_modes_reject_ambiguous_or_inactive_semantics() {
        let invalid_forms = [
            GuiScenarioPcForm::new(2, 0xf3fcf, "I", "srs-plus")
                .with_execution_input(1, None, false, "all")
                .with_score_input("saves", 0),
            save_form("saves").with_execution_input(1, None, false, "unique"),
            save_form("saves").with_back_to_back_preservation(true),
            save_form("saves").with_solution_probabilities(true),
            save_form("saves").with_queue_observation_policy(QueueObservationPolicy::VisibleSeven),
        ];
        for form in invalid_forms {
            let error = ScenarioRequestBuilder::build_command(&form, &GuiBackendForm::default())
                .expect_err("noncanonical pc save semantics must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);
        }

        for backend in [
            GuiBackendForm::default().with_memory_budget_mb(64),
            GuiBackendForm::default().with_tablebase_requested(true),
            GuiBackendForm::default().with_precompute_build_dependencies(true),
        ] {
            let error = ScenarioRequestBuilder::build_command(&save_form("best-save"), &backend)
                .expect_err("pc best-save execution override must fail closed");
            assert_eq!(error.code(), crate::RequestBuildErrorCode::ValidationFailed);
        }
    }
}

mod case_gui_setup_request_uses_residue_and_cycle_boundary_policy {
    use clearra_app::AppCommand;
    use clearra_problem::SetupCycleResetBorrowPolicy;

    use super::*;

    #[test]
    fn gui_setup_request_uses_residue_and_cycle_boundary_policy() {
        let form = crate::GuiSetupSearchForm::new("I,T,O", true, "srs-plus")
            .with_candidate_priority("pc")
            .with_length_preference("longer");
        let command = SetupRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI setup request");
        let AppCommand::Setup(command) = command else {
            panic!("expected setup command");
        };

        assert_eq!(command.query().residue().remaining_count(), 3);
        assert_eq!(command.query().residue().cycle(), Some(7));
        assert_eq!(
            command.query().cycle_reset_borrow_policy(),
            SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
        );
        assert_eq!(
            command.query().candidate_priority(),
            clearra_problem::SetupCandidatePriority::PcProbabilityFirst
        );
        assert_eq!(
            command.query().length_preference(),
            clearra_problem::SetupLengthPreference::Longer
        );
        assert_eq!(command.query().rule().id().as_str(), "srs-plus");
    }

    #[test]
    fn gui_setup_request_preserves_selected_kick_table() {
        let form = crate::GuiSetupSearchForm::new("IOTS", false, "srs-x");
        let command = SetupRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI setup request");
        let AppCommand::Setup(command) = command else {
            panic!("expected setup command");
        };

        assert_eq!(command.query().rule().id().as_str(), "srs-x");
    }

    #[test]
    fn gui_setup_request_preserves_jstris_180_kick_table() {
        let form = crate::GuiSetupSearchForm::new("IOTS", false, "jstris-180");
        let command = SetupRequestBuilder::build_command(&form, &GuiBackendForm::default())
            .expect("GUI setup request");
        let AppCommand::Setup(command) = command else {
            panic!("expected setup command");
        };

        assert_eq!(command.query().rule().id().as_str(), "jstris-180");
    }
}

mod case_gui_app_state_builds_app_request_preview_from_forms {
    use super::*;

    #[test]
    fn gui_app_state_builds_app_request_preview_from_forms() {
        let state = GuiAppState::default();
        let preview = state.app_request_preview().expect("AppRequest preview");

        assert_eq!(state.current_language(), "en");
        assert_eq!(state.current_screen(), GuiScreen::PcSearch);
        assert_eq!(preview.request_model(), "clearra-app/AppRequest");
        assert_eq!(preview.app_request_kind(), "Pc");
        assert_eq!(preview.selected_problem_preset(), "opening-pc");
        assert_eq!(preview.selected_backend(), "auto");
        assert_eq!(preview.selected_lines(), 2);
        assert_eq!(preview.selected_rule(), "srs-plus");
        assert_eq!(
            preview.compiled_command_preview(),
            "clearra pc --lines 2 --backend auto"
        );
        assert_eq!(preview.solver_execution(), "not_started");
    }
}

mod case_gui_domain_model_tracks_screens_backend_and_output_forms {
    use super::*;

    #[test]
    fn gui_domain_model_tracks_screens_backend_and_output_forms() {
        let screens = GuiScreen::ALL
            .iter()
            .map(|screen| screen.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            screens,
            [
                "home",
                "pc-search",
                "scenario-pc",
                "setup-search",
                "build-coverage",
                "rules",
                "scoring",
                "render",
                "settings",
                "diagnostics"
            ]
        );

        let backend_form = GuiBackendForm::default()
            .with_backend(GuiBackendChoice::Hybrid)
            .with_workers(6)
            .with_memory_budget_mb(512)
            .with_candidate_budget(8192)
            .with_pattern_budget(2048);

        assert_eq!(backend_form.backend(), GuiBackendChoice::Hybrid);
        assert!(backend_form.allow_fallback());
        assert_eq!(backend_form.workers(), 6);
        assert!(backend_form.deterministic());
        assert_eq!(backend_form.memory_budget_mb(), 512);
        assert_eq!(backend_form.candidate_budget(), 8192);
        assert_eq!(backend_form.pattern_budget(), 2048);

        let state = GuiAppState::default().with_backend_form(backend_form);
        assert_eq!(state.output_form().format(), GuiOutputFormat::Text);
        assert_eq!(state.render_form().unsupported_reason(), None);
    }
}

mod case_gui_default_memory_budget_is_unbounded {
    use crate::request::BackendRequestBuilder;

    use super::*;

    #[test]
    fn gui_default_memory_budget_is_unbounded() {
        let form = GuiBackendForm::default();
        let policy = BackendRequestBuilder::build_execution_policy(&form)
            .expect("default GUI execution policy");

        assert_eq!(form.memory_budget_mb(), 0);
        assert_eq!(policy.max_memory_mib(), None);

        let capped =
            BackendRequestBuilder::build_execution_policy(&form.with_memory_budget_mb(512))
                .expect("explicit GUI memory policy");
        assert_eq!(capped.max_memory_mib(), Some(512));
    }
}

mod case_gui_worker_policy_preserves_auto_and_full_cpu_opt_in {
    use clearra_pc_graph::request::WorkerPolicy;

    use crate::request::BackendRequestBuilder;

    use super::*;

    #[test]
    fn gui_worker_policy_preserves_auto_and_full_cpu_opt_in() {
        let hardware = WorkerPolicy::hardware_worker_limit();
        let automatic = BackendRequestBuilder::build_execution_policy(&GuiBackendForm::default())
            .expect("automatic GUI execution policy");
        assert_eq!(automatic.workers(), WorkerPolicy::default_worker_limit());
        assert_eq!(automatic.workers_requested(), None);

        let all = BackendRequestBuilder::build_execution_policy(
            &GuiBackendForm::default()
                .with_workers(0)
                .with_use_all_logical_processors(true),
        )
        .expect("all-logical-processors GUI execution policy");
        assert_eq!(all.workers(), hardware);
        assert_eq!(all.workers_requested(), None);
        assert!(all.use_all_logical_processors());
    }
}

mod case_gui_app_state_preserves_execution_job_and_diagnostics {
    use super::*;

    #[test]
    fn gui_app_state_preserves_execution_job_and_diagnostics() {
        let state = GuiAppState::default()
            .with_problem_form(GuiProblemForm::opening_pc(2, "srs-plus"))
            .with_execution_state(GuiExecutionState::running(GuiJobId::new(42)))
            .with_recent_result("unsupported")
            .with_diagnostic("E_NATIVE_CORE_UNAVAILABLE");

        assert_eq!(state.execution_state().phase(), GuiExecutionPhase::Running);
        assert_eq!(
            state.execution_state().active_job_id().map(GuiJobId::get),
            Some(42)
        );
        assert_eq!(state.recent_result(), Some("unsupported"));
        assert_eq!(state.diagnostics(), ["E_NATIVE_CORE_UNAVAILABLE"]);
        assert_eq!(state.user_preferences().language_id().as_str(), "en");
    }
}

mod case_gui_host_default_language_is_english_when_no_locale_signal {
    use super::*;

    #[test]
    fn gui_host_default_language_is_english_when_no_locale_signal() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(Some("auto"), None, None, None).as_str(),
            "en"
        );
    }
}

mod case_gui_host_default_language_is_korean_when_os_locale_ko {
    use super::*;

    #[test]
    fn gui_host_default_language_is_korean_when_os_locale_ko() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(Some("auto"), None, None, Some("ko-KR"))
                .as_str(),
            "ko"
        );
    }
}

mod case_gui_host_stored_preference_wins_over_os_locale {
    use super::*;

    #[test]
    fn gui_host_stored_preference_wins_over_os_locale() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(
                Some("auto"),
                Some("en"),
                None,
                Some("ko-KR")
            )
            .as_str(),
            "en"
        );
    }
}

mod case_gui_host_explicit_language_wins_over_stored_preference {
    use super::*;

    #[test]
    fn gui_host_explicit_language_wins_over_stored_preference() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(
                Some("ko"),
                Some("en"),
                None,
                Some("en-US")
            )
            .as_str(),
            "ko"
        );
    }
}

mod case_gui_host_env_language_wins_over_os_locale {
    use super::*;

    #[test]
    fn gui_host_env_language_wins_over_os_locale() {
        assert_eq!(
            GuiHostLanguageResolver::resolve_with_sources(
                Some("auto"),
                None,
                Some("ko"),
                Some("en-US")
            )
            .as_str(),
            "ko"
        );
    }
}

mod case_tauri_command_calls_clearra_gui_host_only {
    use super::*;

    #[test]
    pub(crate) fn tauri_command_calls_clearra_gui_host_only() {
        let bridge = DesktopTauriCommandBridge::default();
        let response = bridge
            .run_request(
                r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "queue": "IIOOO",
                "hold_enabled": false,
                "backend": "auto"
            }"#,
            )
            .expect("desktop run request");
        let value: Value = serde_json::from_str(&response).expect("desktop response JSON");
        assert_product_runtime_identity(&value["runtime_identity"]);
        let render_capability = &value["capability_report"]["render_capability"];
        assert_eq!(render_capability["png_supported"], true);
        assert_eq!(render_capability["gif_supported"], true);
        assert_eq!(render_capability["render_exact"], true);
        assert!(render_capability["unsupported_reason"].is_null());
        assert_eq!(value["command"], "pc");
        let diagnostics = value["diagnostics"].as_array().expect("diagnostics array");
        let status = value["status"]
            .as_str()
            .expect("desktop response status string");

        if cfg!(any(feature = "native-c-core", feature = "wasm-cpu-runtime")) {
            assert_eq!(status, "success");
        }

        match status {
            "success" => {
                assert!(!value["result"].is_null());
                assert_eq!(value["resource_report"]["solver_executed"], true);
                assert!(!diagnostics.iter().any(|diagnostic| {
                    matches!(
                        diagnostic["code"].as_str(),
                        Some("E_PRODUCT_RUNTIME_UNSUPPORTED" | "E_NATIVE_CORE_UNAVAILABLE")
                    )
                }));
            }
            "unsupported" => {
                assert!(value["result"].is_null());
                assert_eq!(value["backend_report"]["backend_selected"], "none");
                assert_eq!(value["backend_report"]["fallback_used"], false);
                assert_eq!(value["resource_report"]["solver_executed"], false);
                assert_eq!(value["resource_report"]["probability_complete"], false);
                assert!(diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic["code"] == "E_PRODUCT_RUNTIME_UNSUPPORTED"));
            }
            other => panic!("unexpected desktop runtime status: {other}"),
        }
    }
}
pub(crate) use case_tauri_command_calls_clearra_gui_host_only::tauri_command_calls_clearra_gui_host_only;

mod case_desktop_form_builds_app_request {
    use super::*;

    #[test]
    fn desktop_form_builds_app_request() {
        let bridge = DesktopTauriCommandBridge::default();
        let report = bridge
            .validate_request(
                r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "queue": "IIOOO",
                "hold_enabled": false,
                "backend": "cpu"
            }"#,
            )
            .expect("desktop validation report");
        let value: Value = serde_json::from_str(&report).expect("validation JSON");

        assert_eq!(value["command"], "validate_request");
        assert_eq!(value["app_request_model"], "clearra-app/AppRequest");
        assert_eq!(value["valid"], true);
        assert!(value["diagnostics"].is_array());
    }
}

mod case_desktop_job_queue_reports_progress_cancel_result {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn all_events_delivered_in_order_completed_job_releases_active_slot_and_second_job_can_start() {
        let mut bridge = DesktopTauriCommandBridge::default();
        let first_job = bridge
            .start_job(request_json())
            .expect("start first desktop job");
        let first_events = drain_until_terminal(&mut bridge, first_job);
        assert_event_order(&first_events, first_job);

        let second_job = bridge
            .start_job(request_json())
            .expect("completed job releases the active slot");
        assert_ne!(first_job, second_job);
        let second_events = drain_until_terminal(&mut bridge, second_job);
        assert_event_order(&second_events, second_job);
    }

    #[test]
    fn finished_unpolled_job_is_reaped_before_the_next_job_starts() {
        let mut bridge = DesktopTauriCommandBridge::default();
        let first_job = bridge
            .start_job(request_json())
            .expect("start orphaned desktop job");
        let deadline = Instant::now() + Duration::from_secs(10);

        let second_job = loop {
            match bridge.start_job(request_json()) {
                Ok(job_id) => break job_id,
                Err(error) if error.message() == "desktop job already active" => {
                    assert!(
                        Instant::now() < deadline,
                        "orphaned desktop job did not finish"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("reap finished orphaned desktop job: {error}"),
            }
        };

        assert_ne!(first_job, second_job);
        let second_events = drain_until_terminal(&mut bridge, second_job);
        assert_event_order(&second_events, second_job);
    }

    #[cfg(feature = "native-c-core")]
    #[test]
    fn cancel_button_stops_real_search() {
        let mut bridge = DesktopTauriCommandBridge::default();
        let job_id = bridge
            .start_job(
                r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 6,
                "queue": "IOTSZJLIOTSZJLI",
                "hold_enabled": false,
                "backend": "cpu"
            }"#,
            )
            .expect("start desktop job");
        let mut events = drain_until_progress(&mut bridge, job_id);
        bridge
            .cancel_job(job_id)
            .expect("request real search cancellation");
        events.extend(drain_until_terminal(&mut bridge, job_id));
        assert_eq!(
            events.last().and_then(|event| event["event"].as_str()),
            Some("cancelled")
        );
        assert_eq!(
            events
                .last()
                .and_then(|event| event["scope_released"].as_bool()),
            Some(true)
        );
    }

    fn request_json() -> &'static str {
        r#"{
            "app_request_model": "clearra-app/AppRequest",
            "command": "pc",
            "lines": 2,
            "queue": "IIOOO",
            "hold_enabled": false,
            "backend": "auto"
        }"#
    }

    fn drain_until_terminal(bridge: &mut DesktopTauriCommandBridge, job_id: u64) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut events = Vec::new();
        loop {
            let batch = bridge
                .get_job_events(job_id)
                .expect("desktop job event batch");
            let mut batch: Vec<Value> =
                serde_json::from_str(&batch).expect("desktop job event JSON array");
            let terminal = batch.iter().any(|event| {
                matches!(
                    event["event"].as_str(),
                    Some("completed" | "failed" | "cancelled")
                )
            });
            events.append(&mut batch);
            if terminal {
                return events;
            }
            assert!(Instant::now() < deadline, "desktop job did not terminate");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(feature = "native-c-core")]
    fn drain_until_progress(bridge: &mut DesktopTauriCommandBridge, job_id: u64) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut events = Vec::new();
        loop {
            let batch = bridge
                .get_job_events(job_id)
                .expect("desktop pre-cancel event batch");
            let mut batch: Vec<Value> =
                serde_json::from_str(&batch).expect("desktop pre-cancel event JSON array");
            assert!(
                !batch.iter().any(|event| matches!(
                    event["event"].as_str(),
                    Some("completed" | "failed" | "cancelled")
                )),
                "native search ended before the cancellation signal was exercised"
            );
            let progress = batch.iter().any(|event| event["event"] == "progress");
            events.append(&mut batch);
            if progress {
                return events;
            }
            assert!(
                Instant::now() < deadline,
                "desktop job did not begin execution"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn assert_event_order(events: &[Value], job_id: u64) {
        assert_eq!(
            events.first().and_then(|event| event["event"].as_str()),
            Some("started")
        );
        assert!(events.iter().any(|event| event["event"] == "progress"));
        assert!(matches!(
            events.last().and_then(|event| event["event"].as_str()),
            Some("completed" | "failed")
        ));
        assert!(events.iter().all(|event| event["job_id"] == job_id));
        let completed = events
            .iter()
            .find(|event| event["event"] == "completed")
            .expect("desktop async job must emit a completed response");
        assert_product_runtime_identity(&completed["response"]["runtime_identity"]);
    }
}

mod case_desktop_tauri_command_calls_gui_host_only {
    use super::*;

    #[test]
    fn desktop_tauri_command_calls_gui_host_only() {
        tauri_command_calls_clearra_gui_host_only();
    }
}

mod case_desktop_does_not_own_core_c_or_render_atlas {
    use super::*;

    #[test]
    fn desktop_does_not_own_core_c_or_render_atlas() {
        let bridge = DesktopTauriCommandBridge::default();
        let response = bridge
            .run_request(
                r#"{
                "app_request_model": "clearra-app/AppRequest",
                "command": "pc",
                "lines": 2,
                "queue": "IIOOO",
                "hold_enabled": false,
                "backend": "auto"
            }"#,
            )
            .expect("desktop run request");
        let value: Value = serde_json::from_str(&response).expect("desktop response JSON");

        assert_eq!(value["command"], "pc");
        assert!(value.get("raw_pointer").is_none());
        assert!(value.get("render_atlas").is_none());
    }
}
