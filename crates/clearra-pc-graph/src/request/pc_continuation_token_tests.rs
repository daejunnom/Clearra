use clearra_core_domain::{
    objective::objective_kind::ObjectiveKind, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_rules::{
    kicks::{NoKick, VerifiedKickTableProfile},
    profile::{
        builtin_rules::{no_kick, srs_plus},
        rule_profile::RuleProfileId,
    },
};
use clearra_supply::{queue::fixed_sequence::FixedSequence, QueueObservationPolicy};

use crate::request::{
    PcCompletionGoal, PcContinuationToken, PcContinuationTokenCodec, PcCountPolicy, PcHoldPolicy,
    PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};

use super::*;

#[test]
fn opening_v2_token_preserves_rule_objective_and_profile_contract() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
        ])))
        .with_hold_policy(PcHoldPolicy::Disabled)
        .with_rule(no_kick())
        .with_objective(ObjectivePolicy::unique())
        .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);

    let token = PcContinuationTokenCodec::encode_opening_continuation(
        &query,
        None,
        &[PieceKind::T, PieceKind::S],
    );
    let PcContinuationToken::Opening(decoded) =
        PcContinuationTokenCodec::parse(&token).expect("pc2 token")
    else {
        panic!("opening token");
    };

    assert!(token.starts_with("pc2:"));
    assert_eq!(decoded.target(), PcTarget::two_lines());
    assert_eq!(decoded.board().id(), query.board().id());
    assert_eq!(decoded.piece_set().id(), query.piece_set().id());
    assert_eq!(decoded.bag().id(), query.bag().id());
    assert_eq!(decoded.rule().id(), query.rule().id());
    assert_eq!(decoded.objective().kind(), query.objective().kind());
    assert_eq!(decoded.hold_policy(), PcHoldPolicy::Disabled);
    assert_eq!(decoded.queue().len(), 2);
    assert_eq!(
        decoded.queue_observation_policy(),
        QueueObservationPolicy::VisibleSeven
    );
}

#[test]
fn opening_v2_token_preserves_count_policy_independently_of_objective() {
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_objective(ObjectivePolicy::minimum_cover())
        .with_count_policy(PcCountPolicy::CountUnique);

    let token = PcContinuationTokenCodec::encode_opening_continuation(
        &query,
        None,
        &[PieceKind::I, PieceKind::O],
    );
    let PcContinuationToken::Opening(decoded) =
        PcContinuationTokenCodec::parse(&token).expect("pc2 token")
    else {
        panic!("opening token");
    };

    assert!(token.ends_with(":ccount-unique"));
    assert_eq!(decoded.objective().kind(), ObjectiveKind::MinimumCover);
    assert_eq!(decoded.count_policy(), PcCountPolicy::CountUnique);
}

#[test]
fn opening_v2_tokens_without_count_policy_keep_legacy_objective_derivation() {
    let PcContinuationToken::Opening(unique) = PcContinuationTokenCodec::parse(
        "pc2:l2:bdstandard-10:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:ounique:e1:hnone:qI",
    )
    .expect("legacy unique token")
    else {
        panic!("opening token");
    };
    let PcContinuationToken::Opening(minimum_cover) = PcContinuationTokenCodec::parse(
        "pc2:l2:bdstandard-10:psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:omin-cover:e1:hnone:qI",
    )
    .expect("legacy minimum-cover token")
    else {
        panic!("opening token");
    };

    assert_eq!(unique.count_policy(), PcCountPolicy::CountUnique);
    assert_eq!(minimum_cover.count_policy(), PcCountPolicy::CountAll);
}

#[test]
fn scenario_v2_token_preserves_full_query_contract() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
        ])),
        PieceWindow::new(3),
    )
    .with_hold_piece(Some(PieceKind::S))
    .with_rule(srs_plus())
    .with_exact_pieces(Some(2))
    .with_min_remaining_queue(1)
    .with_allow_hold(true)
    .with_requires_180(true)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
    .with_retained_trace_limit(7);

    let token = PcContinuationTokenCodec::encode_scenario_continuation(&query).expect("sc2 token");
    let PcContinuationToken::Scenario(decoded) =
        PcContinuationTokenCodec::parse(&token).expect("decode sc2")
    else {
        panic!("scenario token");
    };

    assert!(token.starts_with("sc2:"));
    assert_eq!(decoded.initial_board(), query.initial_board());
    assert_eq!(decoded.piece_set().id(), query.piece_set().id());
    assert_eq!(decoded.bag().id(), query.bag().id());
    assert_eq!(decoded.rule().id(), query.rule().id());
    assert_eq!(decoded.hold_state(), query.hold_state());
    assert_eq!(
        decoded.remaining_queue().len(),
        query.remaining_queue().len()
    );
    assert_eq!(decoded.piece_window(), query.piece_window());
    assert_eq!(decoded.exact_pieces(), query.exact_pieces());
    assert_eq!(decoded.min_remaining_queue(), query.min_remaining_queue());
    assert_eq!(decoded.allow_hold(), query.allow_hold());
    assert_eq!(decoded.requires_180(), query.requires_180());
    assert_eq!(decoded.completion_goal(), PcCompletionGoal::ClearToEmpty);
    assert_eq!(decoded.count_policy(), query.count_policy());
    assert_eq!(decoded.objective().kind(), query.objective().kind());
    assert_eq!(decoded.retained_trace_limit(), query.retained_trace_limit());
    assert_eq!(
        decoded.queue_observation_policy(),
        QueueObservationPolicy::VisibleSeven
    );
    assert!(decoded.verified_kick_profile().is_none());
}

#[test]
fn scenario_replay_token_is_separate_from_continuation_token() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_retained_trace_limit(1);

    let token = PcContinuationTokenCodec::encode_scenario_replay(&query).expect("sr2 token");
    let PcContinuationToken::ScenarioReplay(decoded) =
        PcContinuationTokenCodec::parse(&token).expect("decode sr2")
    else {
        panic!("scenario replay token");
    };

    assert!(token.starts_with("sr2:"));
    assert_eq!(decoded.initial_board(), query.initial_board());
    assert_eq!(
        decoded.remaining_queue().len(),
        query.remaining_queue().len()
    );
    assert_eq!(decoded.exact_pieces(), query.exact_pieces());
    assert_eq!(decoded.retained_trace_limit(), query.retained_trace_limit());
}

#[test]
fn scenario_v2_token_preserves_verified_kick_profile_override() {
    let verified = VerifiedKickTableProfile::try_new(NoKick::profile()).expect("verified no-kick");
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_rule(no_kick())
    .with_verified_kick_table_profile(verified.clone())
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1);

    let token = PcContinuationTokenCodec::encode_scenario_continuation(&query).expect("sc2 token");
    let PcContinuationToken::Scenario(decoded) =
        PcContinuationTokenCodec::parse(&token).expect("decode sc2")
    else {
        panic!("scenario token");
    };

    assert!(token.starts_with("sc2:"));
    assert!(token.contains(":k"));
    assert_ne!(token.rsplit(':').next(), Some("knone"));
    assert_eq!(decoded.rule().id(), query.rule().id());
    assert_eq!(decoded.verified_kick_profile(), Some(&verified));
}

#[test]
fn v1_tokens_migrate_to_current_encoding() {
    let PcContinuationToken::Opening(opening) =
        PcContinuationTokenCodec::parse("pc1:l2:e0:hnone:qIOT").expect("pc1")
    else {
        panic!("opening token");
    };
    let PcContinuationToken::ScenarioReplay(scenario) =
        PcContinuationTokenCodec::parse("sc1:w10:v2:m0x00000000000003f0:hnone:qI:p1").expect("sc1")
    else {
        panic!("scenario token");
    };

    assert_eq!(opening.rule().id(), RuleProfileId::SrsPlus);
    assert_eq!(opening.objective().kind(), ObjectiveKind::All);
    assert_eq!(scenario.rule().id(), RuleProfileId::SrsPlus);
    assert_eq!(scenario.count_policy(), PcCountPolicy::CountAll);
    assert_eq!(
        opening.queue_observation_policy(),
        QueueObservationPolicy::FullQueueOracle
    );
    assert_eq!(
        scenario.queue_observation_policy(),
        QueueObservationPolicy::FullQueueOracle
    );

    let current_opening = PcContinuationTokenCodec::encode_opening_continuation(
        &opening,
        None,
        &[PieceKind::I, PieceKind::O, PieceKind::T],
    );
    let current_scenario =
        PcContinuationTokenCodec::encode_scenario_replay(&scenario).expect("sr2 token");
    assert!(current_opening.starts_with("pc2:"));
    assert!(current_scenario.starts_with("sr2:"));
    assert!(matches!(
        PcContinuationTokenCodec::parse(&current_opening),
        Ok(PcContinuationToken::Opening(_))
    ));
    assert!(matches!(
        PcContinuationTokenCodec::parse(&current_scenario),
        Ok(PcContinuationToken::ScenarioReplay(_))
    ));
}
