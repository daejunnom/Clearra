//! Focused App ingress checks without linking the monolithic App unit-test binary.

use clearra_app::{
    AppCommand, BuildCoverV2Request, BuildObjective, BuildV2AppCommand, FieldDocumentFormat,
    ScenarioAppCommand, SetupScoreAppCommand, SetupScoreDocumentV1,
};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_ctk3::{encode_ctk3, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece};
use clearra_objectives::policy::score_objective_policy::ScoreProfileSelection;
use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend,
};
use clearra_problem::{
    BuildProbabilityField, BuildProbabilityQuery, BuildSolutionProbabilityPolicy,
};
use clearra_rules::kicks::{
    KickOffsetSequence, KickTableEntry, KickTableProfile, KickTableProfileId, KickTransition,
    SrsKicks, VerifiedKickTableProfile,
};
use clearra_rules::profile::rule_profile::{RuleProfile, RuleProfileId};
use clearra_supply::queue::{fixed_sequence::FixedSequence, queue_parser};

fn one_piece_build(policy: PcExecutionPolicy) -> BuildProbabilityQuery {
    let core = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_execution_policy(policy);
    let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
        .expect("canonical target");
    BuildProbabilityQuery::new(core, field)
        .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include)
}

fn setup_document() -> SetupScoreDocumentV1 {
    let mut cells = vec![Ctk3Color::Empty; 20];
    cells[0..4].fill(Ctk3Color::Piece(Ctk3Piece::I));
    let encoded = encode_ctk3(&Ctk3Document::new(10, vec![Ctk3Page::new(2, cells)]))
        .expect("canonical document");
    SetupScoreDocumentV1::decode(FieldDocumentFormat::Ctk3, &encoded).expect("Setup-score input")
}

#[test]
fn build_and_setup_score_project_each_request_owned_accelerator_switch() {
    for legal in [false, true] {
        for conditioned in [false, true] {
            let policy = PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false)
                .with_exact_legal_board_enabled(legal)
                .with_conditioned_reachability_enabled(conditioned);
            let build =
                BuildCoverV2Request::new(one_piece_build(policy.clone()), BuildObjective::MinCover)
                    .expect("Build product request");
            assert_eq!(
                AppCommand::BuildV2(BuildV2AppCommand::build_cover(build))
                    .exact_accelerator_policy(),
                Some((legal, conditioned))
            );

            let setup = SetupScoreAppCommand::new(
                setup_document(),
                PcQueueInput::fixed_sequence(
                    queue_parser::parse_fixed_sequence("I").expect("Setup queue"),
                ),
                None,
                PcQueueInput::fixed_sequence(
                    queue_parser::parse_fixed_sequence("OTSJ").expect("continuation queue"),
                ),
                None,
                2,
                false,
                ScoreProfileSelection::Tetrio,
                0,
                RuleProfile::new(RuleProfileId::SrsPlus),
                policy,
            )
            .expect("Setup-score request");
            assert_eq!(
                AppCommand::SetupScore(setup).exact_accelerator_policy(),
                Some((legal, conditioned))
            );
        }
    }
}

#[test]
fn scenario_activation_requires_actual_builtin_first_success_order() {
    let builtin = SrsKicks::profile();
    let verify = |profile| VerifiedKickTableProfile::try_new(profile).unwrap();
    let builtin_import = verify(builtin.clone());
    let mut records = builtin.entries().to_vec();
    records.reverse();
    let reversed_records = verify(KickTableProfile::new(
        builtin.id(),
        builtin.source_rule(),
        records.clone(),
    ));
    let moved_kick = KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
    let entry = records
        .iter_mut()
        .find(|record| record.transition() == moved_kick)
        .unwrap();
    let mut candidates = entry.sequence().offsets().to_vec();
    candidates[..=2].rotate_right(1);
    *entry = KickTableEntry::new(moved_kick, KickOffsetSequence::new(candidates));
    let different_order = verify(KickTableProfile::new(
        builtin.id(),
        builtin.source_rule(),
        records,
    ));
    let unconnected = verify(KickTableProfile::new(
        KickTableProfileId::Imported,
        builtin.source_rule(),
        builtin.entries().to_vec(),
    ));
    let policy_for = |rule, profile: Option<VerifiedKickTableProfile>, policy| {
        let mut query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_rule(RuleProfile::new(rule))
        .with_execution_policy(policy);
        if let Some(profile) = profile {
            query = query.with_verified_kick_table_profile(profile);
        }
        AppCommand::Scenario(ScenarioAppCommand::new(query)).exact_accelerator_policy()
    };
    let defaults = PcExecutionPolicy::default();
    assert_eq!(
        policy_for(RuleProfileId::Srs, None, defaults.clone()),
        Some((true, true))
    );
    for legal in [false, true] {
        for conditioned in [false, true] {
            let policy = defaults
                .clone()
                .with_exact_legal_board_enabled(legal)
                .with_conditioned_reachability_enabled(conditioned)
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false);
            for accepted in [
                None,
                Some(builtin_import.clone()),
                Some(reversed_records.clone()),
            ] {
                assert_eq!(
                    policy_for(RuleProfileId::Srs, accepted, policy.clone()),
                    Some((legal, conditioned))
                );
            }
            for rejected in [different_order.clone(), unconnected.clone()] {
                assert_eq!(
                    policy_for(RuleProfileId::Srs, Some(rejected), policy.clone()),
                    None
                );
            }
        }
    }
    assert_eq!(policy_for(RuleProfileId::Asc, None, defaults), None);
}
