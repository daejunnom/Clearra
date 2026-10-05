//! SRP rationale: execute small real App products against the same canonical
//! candidate/coverage reducer with request-owned accelerator switches. This
//! is functional equality, not a benchmark or asset qualification producer.

use clearra_app::{
    AppCommand, AppContext, AppCoreExecutorService, AppRequest, AppResponse, AppServices,
    AppStatus, BuildCoverV2Request, BuildObjective, BuildProbabilityAppCommand,
    BuildProbabilityResultMode, BuildV2AppCommand, CoveragePortfolioAlternativeSet,
    FieldDocumentFormat, PcChanceIngressOrigin, PcMinimalsIngressOrigin, PcPathIngressOrigin,
    PcResultProjection, ProductCapabilityContract, ScenarioAppCommand, SetupScoreAppCommand,
    SetupScoreDocumentV1, PC_SCORE_MAX_PATTERNS,
};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy, score_objective_policy::ScoreProfileSelection,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, WorkerPolicy,
};
use clearra_problem::{
    BuildProbabilityField, BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
};
use clearra_rules::profile::rule_profile::{RuleProfile, RuleProfileId};
use clearra_supply::queue::fixed_sequence::FixedSequence;

const SETUP_SCORE_DOCUMENT: &str = "ctk3_w0kGEPVAACzgA2A9EAAw3A";

#[test]
fn actual_setup_score_fixture_has_two_i_targets_and_one_duplicate_page() {
    use clearra_ctk3::{Ctk3Color, Ctk3Piece};
    let document = clearra_ctk3::decode_ctk3_exact(SETUP_SCORE_DOCUMENT).unwrap();
    assert_eq!(document.width, 10);
    assert_eq!(document.pages.len(), 3);
    let masks: Vec<u64> = document
        .pages
        .iter()
        .map(|page| {
            assert_eq!(page.height, 1);
            assert_eq!(page.cells.len(), 10);
            assert_eq!(
                page.cells
                    .iter()
                    .filter(|&&color| color == Ctk3Color::Piece(Ctk3Piece::I))
                    .count(),
                4
            );
            page.cells
                .iter()
                .enumerate()
                .fold(0, |mask, (index, color)| {
                    assert!(matches!(
                        color,
                        Ctk3Color::Empty | Ctk3Color::Piece(Ctk3Piece::I)
                    ));
                    if *color == Ctk3Color::Empty {
                        mask
                    } else {
                        mask | (1_u64 << index)
                    }
                })
        })
        .collect();
    assert_eq!(masks, [0xf, 0x3c0, 0xf]);
}

fn policy(legal: bool, conditioned: bool, workers: usize) -> PcExecutionPolicy {
    PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(workers)
        .with_use_all_logical_processors(workers > 1)
        .with_exact_legal_board_enabled(legal)
        .with_conditioned_reachability_enabled(conditioned)
}

fn supported_worker_requests() -> impl Iterator<Item = usize> {
    let hardware = WorkerPolicy::hardware_worker_limit();
    let parallel_enabled = cfg!(feature = "parallel");
    // Native validation must reject oversubscription. The hosted CI runner
    // may have only two CPUs, and non-parallel builds cannot create a worker
    // pool. Never relabel an unavailable worker arm as an executed result.
    [1, 2, 11]
        .into_iter()
        .filter(move |&workers| workers <= hardware && (workers == 1 || parallel_enabled))
}

fn minimum_request(
    height: u8,
    legal: bool,
    conditioned: bool,
    score: bool,
    workers: usize,
) -> AppRequest {
    let board = (0..height).fold(0, |board, row| board | (0x3f0_u64 << (row * 10)));
    let objective = if score {
        ObjectivePolicy::minimum_cover().with_score_profile(ScoreProfileSelection::Tetrio)
    } else {
        ObjectivePolicy::minimum_cover()
    };
    let execution = if score {
        policy(legal, conditioned, workers).with_max_patterns(PC_SCORE_MAX_PATTERNS)
    } else {
        policy(legal, conditioned, workers)
    };
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(u16::from(height), board),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; usize::from(height)])),
        PieceWindow::new(usize::from(height)),
    )
    .with_exact_pieces(Some(usize::from(height)))
    .with_count_policy(if score {
        PcCountPolicy::CountAll
    } else {
        // Public pc-pattern.v2 ingress is unique; the canonical minimals
        // product owns the internal full-source replay/normalization.
        PcCountPolicy::CountUnique
    })
    .with_execution_policy(execution)
    .with_objective(objective);
    let query = if score {
        query.with_retained_trace_limit(1)
    } else {
        query
    };
    let projection = if score {
        PcResultProjection::pc_score_minimals()
    } else {
        PcResultProjection::MinimumCoverV2(PcMinimalsIngressOrigin::CanonicalPcMinimals)
    };
    AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query).with_result_projection(projection),
    ))
    .with_product_capability_contract(if score {
        ProductCapabilityContract::PcScoreMinimals
    } else {
        ProductCapabilityContract::PcMinimals
    })
    .unwrap()
}

fn build_request(legal: bool, conditioned: bool) -> AppRequest {
    let core = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_execution_policy(policy(legal, conditioned, 1));
    let field =
        BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0]).unwrap();
    let query = BuildProbabilityQuery::new(core, field)
        .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
    AppRequest::new(AppCommand::BuildV2(BuildV2AppCommand::build_cover(
        BuildCoverV2Request::new(query, BuildObjective::MinCover).unwrap(),
    )))
}

fn build_probability_request(
    legal: bool,
    conditioned: bool,
    mode: BuildProbabilityResultMode,
) -> AppRequest {
    let core = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_execution_policy(policy(legal, conditioned, 1));
    let field =
        BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0]).unwrap();
    let mut query = BuildProbabilityQuery::new(core, field)
        .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
    if matches!(
        mode,
        BuildProbabilityResultMode::FieldAverageScore
            | BuildProbabilityResultMode::FixedQueueMaximumScore
            | BuildProbabilityResultMode::HighestScoreMinimumSet
    ) {
        query = query.with_score_summary(ScoreProfileSelection::Guideline, 0);
    }
    AppRequest::new(AppCommand::BuildProbability(
        BuildProbabilityAppCommand::new(query).with_result_mode(mode),
    ))
}

fn one_piece_report_request(legal: bool, conditioned: bool, replay: bool) -> AppRequest {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_execution_policy(policy(legal, conditioned, 1))
    .with_count_policy(if replay {
        PcCountPolicy::CountAll
    } else {
        PcCountPolicy::CountUnique
    })
    .with_objective(if replay {
        ObjectivePolicy::all()
    } else {
        ObjectivePolicy::unique()
    });
    let (projection, contract) = if replay {
        (
            PcResultProjection::PathFamilyV2(PcPathIngressOrigin::CanonicalPcPath),
            ProductCapabilityContract::PcPath,
        )
    } else {
        (
            PcResultProjection::ChanceProbabilityV2(PcChanceIngressOrigin::CanonicalPcChance),
            ProductCapabilityContract::PcChance,
        )
    };
    AppRequest::new(AppCommand::Scenario(
        ScenarioAppCommand::new(query).with_result_projection(projection),
    ))
    .with_product_capability_contract(contract)
    .unwrap()
}

fn setup_score_request(legal: bool, conditioned: bool, workers: usize) -> AppRequest {
    let document = SetupScoreDocumentV1::decode(FieldDocumentFormat::Ctk3, SETUP_SCORE_DOCUMENT)
        .expect("three source pages, two distinct horizontal I targets");
    let command = SetupScoreAppCommand::new(
        document,
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        None,
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::I,
        ])),
        None,
        2,
        false,
        ScoreProfileSelection::Tetrio,
        0,
        RuleProfile::new(RuleProfileId::SrsPlus),
        policy(legal, conditioned, workers),
    )
    .expect("Setup coverage and PC score continuation request");
    AppRequest::new(AppCommand::SetupScore(command))
}

#[cfg(feature = "parallel")]
fn register_setup_system_host() {
    use clearra_app::{
        register_native_build_probability_host, NativeBuildProbabilityHostRegistration,
        SystemNativeBuildProbabilityAdmissionProvider,
    };
    use std::{
        path::PathBuf,
        sync::Once,
        time::{SystemTime, UNIX_EPOCH},
    };
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        // Same production provider as CLI startup; only durable test output
        // is isolated under the existing managed Desktop functional root.
        let journal = repository.join("_local/artifacts/v081-desktop-native-smoke/app-journals");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let uuid = format!(
            "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
            std::process::id(),
            (nonce >> 48) & 0xffff,
            (nonce >> 36) & 0xfff,
            (nonce >> 24) & 0xfff,
            nonce & 0xffffffffffff
        );
        register_native_build_probability_host(
            NativeBuildProbabilityHostRegistration::new(
                SystemNativeBuildProbabilityAdmissionProvider,
                journal,
                uuid,
            )
            .expect("private durable host registration"),
        )
        .expect("real source-bound native Build admission provider");
    });
}

#[derive(Debug, Eq, PartialEq)]
struct PortfolioMeaning {
    // Deliberately exclude request/telemetry identities. These fields are the
    // entire semantic candidate map, coverage universe and canonical choice,
    // not a scrubbed whole-response comparison that could hide missing rows.
    candidates: Vec<(u64, String)>,
    coverage: Vec<PatternBitSet>,
    required: PatternBitSet,
    candidate_map_sha256: String,
    canonical_ids: Vec<u64>,
    optimal_cardinality: usize,
}

fn portfolio_meaning(set: &CoveragePortfolioAlternativeSet) -> PortfolioMeaning {
    PortfolioMeaning {
        candidates: set
            .candidates()
            .iter()
            .map(|candidate| {
                (
                    candidate.candidate_id(),
                    candidate.normalized_key().to_owned(),
                )
            })
            .collect(),
        coverage: set.coverage_rows().to_vec(),
        required: set.required_patterns().clone(),
        candidate_map_sha256: set.candidate_map_sha256().to_owned(),
        canonical_ids: set.canonical_page().portfolio().candidate_ids().to_vec(),
        optimal_cardinality: set.optimal_cardinality(),
    }
}

fn success(context: &AppContext, request: AppRequest) -> AppResponse {
    let response = context.run(request);
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    response
}

fn failed_queue_meaning(response: &AppResponse) -> Vec<(String, String)> {
    response
        .render_model()
        .unwrap()
        .core_result()
        .unwrap()
        .summary_field_entries()
        .filter(|(key, _)| {
            matches!(
                *key,
                "result_mode"
                    | "build_failed_queue_contract"
                    | "failed_queue_probability"
                    | "total_pattern_count"
            ) || key.starts_with("failed_pattern_")
        })
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn assert_relation_snapshot(response: &AppResponse, requested: bool, installed: bool) {
    let result = response.render_model().unwrap().core_result().unwrap();
    assert_eq!(
        result.unique_field("conditioned_reachability_requested"),
        Some(if requested { "true" } else { "false" })
    );
    assert_eq!(
        result.unique_field("conditioned_reachability_policy_enabled"),
        Some(if requested { "true" } else { "false" })
    );
    assert_eq!(
        result.unique_field("conditioned_reachability_snapshot_active"),
        Some(if requested && installed {
            "true"
        } else {
            "false"
        })
    );
}

fn compare_products(installed: bool) {
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    for height in 1..=6 {
        let baseline = success(&context, minimum_request(height, false, false, false, 1));
        assert_relation_snapshot(&baseline, false, installed);
        let expected = baseline
            .product_capability_result()
            .unwrap()
            .pc_minimum_cover_v2()
            .unwrap();
        assert!(expected.completeness().complete());
        assert_eq!(expected.required_pattern_count(), 1);
        assert_eq!(expected.selected_solution_count(), 1);
        let meaning = portfolio_meaning(expected.portfolio_alternatives());
        assert!(!meaning.candidates.is_empty());
        for workers in supported_worker_requests() {
            for (legal, conditioned) in [(false, false), (false, true), (true, false), (true, true)]
            {
                if workers == 1 && !legal && !conditioned {
                    continue; // The exact baseline request has already executed above.
                }
                let actual = success(
                    &context,
                    minimum_request(height, legal, conditioned, false, workers),
                );
                assert_relation_snapshot(&actual, conditioned, installed);
                let report = actual
                    .product_capability_result()
                    .unwrap()
                    .pc_minimum_cover_v2()
                    .unwrap();
                assert!(report.completeness().complete());
                assert_eq!(
                    report.normalized_solution_set_hash(),
                    expected.normalized_solution_set_hash()
                );
                assert_eq!(
                    report.selected_solution_keys(),
                    expected.selected_solution_keys()
                );
                assert_eq!(
                    report.required_pattern_count(),
                    expected.required_pattern_count()
                );
                assert_eq!(
                    report.source_solution_count(),
                    expected.source_solution_count()
                );
                assert_eq!(
                    portfolio_meaning(report.portfolio_alternatives()),
                    meaning,
                    "{height}L workers={workers} legal={legal} conditioned={conditioned}"
                );
            }
        }
    }
    let baseline = success(&context, minimum_request(1, false, false, true, 1));
    let expected = baseline
        .product_capability_result()
        .unwrap()
        .pc_score_portfolio_v2()
        .unwrap();
    assert!(expected.completeness().complete());
    for workers in supported_worker_requests() {
        for (legal, conditioned) in [(false, false), (false, true), (true, false), (true, true)] {
            if workers == 1 && !legal && !conditioned {
                continue;
            }
            let actual = success(
                &context,
                minimum_request(1, legal, conditioned, true, workers),
            );
            assert_relation_snapshot(&actual, conditioned, installed);
            let report = actual
                .product_capability_result()
                .unwrap()
                .pc_score_portfolio_v2()
                .unwrap();
            assert!(report.completeness().complete());
            assert_eq!(report.pattern_best_scores(), expected.pattern_best_scores());
            assert_eq!(
                report.eligible_candidate_map_sha256(),
                expected.eligible_candidate_map_sha256()
            );
            assert_eq!(
                report.score_eligibility_sha256(),
                expected.score_eligibility_sha256()
            );
            assert_eq!(
                report.selected_solution_keys(),
                expected.selected_solution_keys()
            );
            assert_eq!(
                portfolio_meaning(report.portfolio_alternatives()),
                portfolio_meaning(expected.portfolio_alternatives()),
                "score-minimals workers={workers} legal={legal} conditioned={conditioned}"
            );
        }
    }
    let baseline = success(&context, build_request(false, false));
    let expected = baseline
        .product_capability_result()
        .unwrap()
        .build_coverage_portfolio_v2()
        .unwrap();
    assert!(expected.completeness().complete());
    assert_eq!(expected.source_candidate_count(), 1);
    assert_eq!(expected.union_probability(), "1");
    for (legal, conditioned) in [(false, true), (true, false), (true, true)] {
        let actual = success(&context, build_request(legal, conditioned));
        let report = actual
            .product_capability_result()
            .unwrap()
            .build_coverage_portfolio_v2()
            .unwrap();
        assert!(report.completeness().complete());
        assert_eq!(
            report.normalized_solution_set_hash(),
            expected.normalized_solution_set_hash()
        );
        assert_eq!(report.union_probability(), expected.union_probability());
        assert_eq!(
            report.canonical_candidate_keys(),
            expected.canonical_candidate_keys()
        );
        assert_eq!(
            portfolio_meaning(report.portfolio_alternative_owner().unwrap()),
            portfolio_meaning(expected.portfolio_alternative_owner().unwrap())
        );
    }
    for mode in [
        BuildProbabilityResultMode::CompleteReplayPaths,
        BuildProbabilityResultMode::FieldAverageScore,
        BuildProbabilityResultMode::FixedQueueMaximumScore,
        BuildProbabilityResultMode::HighestScoreMinimumSet,
        BuildProbabilityResultMode::FailedQueues,
    ] {
        let baseline = context.run(build_probability_request(false, false, mode));
        assert_eq!(
            baseline.status(),
            AppStatus::Success,
            "{mode:?}: {baseline:#?}"
        );
        let expected = baseline.public_result_payload();
        let expected_failed_queues = if mode == BuildProbabilityResultMode::FailedQueues {
            assert!(expected.is_none(), "failed queues use exact result fields");
            Some(failed_queue_meaning(&baseline))
        } else {
            assert!(expected.is_some(), "{mode:?} requires a typed payload");
            None
        };
        for (legal, conditioned) in [(false, true), (true, false), (true, true)] {
            let actual = context.run(build_probability_request(legal, conditioned, mode));
            assert_eq!(
                actual.status(),
                AppStatus::Success,
                "{mode:?} legal={legal} conditioned={conditioned}: {actual:#?}"
            );
            assert_eq!(
                actual.public_result_payload(),
                expected,
                "Build probability aggregation {mode:?} legal={legal} conditioned={conditioned}"
            );
            if let Some(expected_failed_queues) = &expected_failed_queues {
                assert_eq!(
                    &failed_queue_meaning(&actual),
                    expected_failed_queues,
                    "Build failed-queue complement legal={legal} conditioned={conditioned}"
                );
            }
        }
    }
    let baseline = success(&context, one_piece_report_request(false, false, false));
    let expected = baseline
        .product_capability_result()
        .unwrap()
        .pc_probability_v2()
        .unwrap();
    assert!(expected.completeness().complete());
    assert_eq!(expected.weighted_probability_bits(), 1.0_f64.to_bits());
    assert_eq!(expected.covered_pattern_count(), 1);
    for (legal, conditioned) in [(false, true), (true, false), (true, true)] {
        let actual = success(
            &context,
            one_piece_report_request(legal, conditioned, false),
        );
        let report = actual
            .product_capability_result()
            .unwrap()
            .pc_probability_v2()
            .unwrap();
        assert!(report.completeness().complete());
        assert_eq!(report.coverage_row_count(), expected.coverage_row_count());
        assert_eq!(report.total_pattern_count(), expected.total_pattern_count());
        assert_eq!(
            report.covered_pattern_count(),
            expected.covered_pattern_count()
        );
        assert_eq!(
            report.coverage_pattern_words(),
            expected.coverage_pattern_words()
        );
        assert_eq!(
            report.weighted_probability_bits(),
            expected.weighted_probability_bits()
        );
        assert_eq!(
            report.materialized_probability_mass_bits(),
            expected.materialized_probability_mass_bits()
        );
    }
    let baseline = success(&context, one_piece_report_request(false, false, true));
    let expected = baseline
        .product_capability_result()
        .unwrap()
        .pc_path_family_v2()
        .unwrap();
    assert!(expected.completeness().complete());
    assert!(expected.witness_count() > 0);
    assert_eq!(expected.witnesses().len() as u128, expected.witness_count());
    for witness in expected.witnesses() {
        assert_eq!(witness.steps().len(), 1);
        assert_eq!(witness.steps()[0].board_after_line_clear_mask(), 0);
        assert_eq!(witness.steps()[0].cleared_lines(), 1);
    }
    for (legal, conditioned) in [(false, true), (true, false), (true, true)] {
        let actual = success(&context, one_piece_report_request(legal, conditioned, true));
        let report = actual
            .product_capability_result()
            .unwrap()
            .pc_path_family_v2()
            .unwrap();
        assert!(report.completeness().complete());
        assert_eq!(report.witness_count(), expected.witness_count());
        assert_eq!(report.witnesses(), expected.witnesses());
        assert_eq!(report.canonical_witness(), expected.canonical_witness());
        assert_eq!(report.ordering(), expected.ordering());
    }
    #[cfg(feature = "parallel")]
    register_setup_system_host();
    let baseline = success(&context, setup_score_request(false, false, 1));
    let expected = baseline.public_result_payload().unwrap();
    let clearra_host_contract::ProductResultPayloadContent::SetupScoreRanking(ranking) =
        expected.content()
    else {
        panic!("actual Setup-score ranked payload required");
    };
    assert!(ranking.complete());
    assert_eq!(ranking.source_page_count(), "3");
    assert_eq!(ranking.candidate_count(), "2");
    assert_eq!(ranking.setup_pattern_count(), "1");
    assert_eq!(ranking.candidates().len(), 2);
    let score: f64 = ranking.average_priority_score().parse().unwrap();
    assert!(score.is_finite() && score > 0.0);
    for (index, candidate) in ranking.candidates().iter().enumerate() {
        assert_eq!(candidate.rank(), (index + 1).to_string());
        assert_eq!(candidate.setup_covered_pattern_count(), "1");
        assert_eq!(
            candidate.setup_covered_probability().parse::<f64>(),
            Ok(1.0)
        );
        assert_eq!(candidate.continuation_probability().parse::<f64>(), Ok(1.0));
        assert_eq!(
            candidate.unconditional_expected_score().parse::<f64>(),
            Ok(score)
        );
    }
    assert!(ranking.candidates()[0].candidate_id() < ranking.candidates()[1].candidate_id());
    for workers in supported_worker_requests() {
        for (legal, conditioned) in [(false, false), (false, true), (true, false), (true, true)] {
            let actual = success(&context, setup_score_request(legal, conditioned, workers));
            assert_eq!(
                actual.public_result_payload(),
                Some(expected),
                "Setup coverage/continuation/reduction workers={workers} legal={legal} conditioned={conditioned}"
            );
        }
    }
}

#[test]
fn unavailable_optional_assets_preserve_real_pc_score_and_build_reducers() {
    compare_products(false);
}

#[test]
#[ignore = "requires explicit download of the SRS+ signed legal-board and relation packs"]
fn installed_signed_assets_preserve_real_pc_score_and_build_reducers() {
    use clearra_accelerator_product_host::{
        embedded_catalog, CatalogProfileStatus, ProductCatalogKind,
    };
    use clearra_accelerator_runtime::{active_identity, install, qualify_signed, remove};
    use std::{path::PathBuf, sync::Arc};
    let root = PathBuf::from(
        std::env::var_os("CLEARRA_SIGNED_APP_SMOKE_DIR")
            .expect("explicit signed App smoke directory"),
    );
    let mut identities = Vec::new();
    for (kind, file) in [
        (
            ProductCatalogKind::ExactLegalBoard,
            "legal-board-srs-plus-v2.cllb",
        ),
        (
            ProductCatalogKind::BoardConditionedReachability,
            "conditioned-srs-plus.cllr",
        ),
    ] {
        let catalog = embedded_catalog(kind).unwrap();
        let CatalogProfileStatus::Qualified(asset) = catalog.profile("srs-plus").unwrap() else {
            panic!("current SRS+ catalog must be qualified");
        };
        let bytes: Arc<[u8]> = std::fs::read(root.join(file)).unwrap().into();
        assert_eq!(bytes.len() as u64, asset.authority().payload_bytes());
        install(qualify_signed(kind, "srs-plus", bytes, asset).unwrap()).unwrap();
        identities.push((
            kind,
            (
                asset.authority().generation_identity(),
                asset.authority().statement_identity(),
            ),
        ));
    }
    compare_products(true);
    for (kind, identity) in identities {
        assert_eq!(active_identity(kind, "srs-plus"), Some(identity));
        remove(kind, "srs-plus").unwrap();
        assert_eq!(active_identity(kind, "srs-plus"), None);
    }
}
