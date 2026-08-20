use clearra_core_domain::{objective::objective_kind::ObjectiveKind, pc::pc_target::PcTarget};
use clearra_pc_graph::request::{
    validate_pc_observation_objective, OpeningPcSearchQuery, PcQueueInput, PcScenarioQuery,
};
use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    validators::{
        board_validator::validate_board_profile,
        objective_validator::validate_objective_kind,
        pc_execution_policy_validator::validate_pc_execution_policy,
        pc_execution_policy_validator::PcBackendCompatibilityContext,
        pc_scenario_query_validator,
        piece_set_validator::validate_piece_set_profile,
        rule_validator::validate_rule_profile_with_verified_kick_profile,
        supply_validator::{
            validate_bag_aligned_pattern, validate_fixed_sequence, validate_observed_queue,
        },
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcQueryValidator;

impl PcQueryValidator {
    pub fn validate(query: &OpeningPcSearchQuery) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        report.append(Self::validate_target(query.target()));
        report.append(validate_board_profile(query.board()));
        report.append(validate_piece_set_profile(query.piece_set()));
        report.append(validate_rule_profile_with_verified_kick_profile(
            query.rule(),
            query.verified_kick_profile(),
        ));
        report.append(validate_objective_kind(query.objective().kind()));
        report.append(validate_observation_objective_contract(
            query.queue_observation_policy(),
            query.objective().kind(),
        ));
        report.append(match query.queue() {
            PcQueueInput::FixedSequence(sequence) => validate_fixed_sequence(sequence),
            PcQueueInput::BagAlignedPattern(pattern) => validate_bag_aligned_pattern(pattern),
            PcQueueInput::PatternExpression(_) => DiagnosticReport::new(),
            PcQueueInput::Standard7Bag => DiagnosticReport::new(),
            PcQueueInput::Observed(queue) => validate_observed_queue(queue),
        });
        report.append(validate_pc_execution_policy(
            query.execution_policy(),
            PcBackendCompatibilityContext::opening(query.objective().kind()),
            "pc.execution",
        ));
        validate_bag_contract(query, &mut report);

        if !report.has_errors() {
            report.push(pc_query_supported_diagnostic(query));
        }

        report
    }
}
impl PcQueryValidator {
    pub fn validate_target(target: PcTarget) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        if matches!(target.lines(), 2 | 4 | 6) {
            report.push(
                Diagnostic::new(
                    DiagnosticCode::IPcTargetMvpSupported,
                    "PC target is supported by the MVP1 checkpoint DAG",
                )
                .with_location(EvidenceLocation::new("pc.target"))
                .with_evidence(ValidationEvidence::new("lines", target.lines().to_string())),
            );
        } else {
            report.push(
                Diagnostic::new(
                    DiagnosticCode::EPcTargetUnsupportedMvp,
                    "only 2L, 4L, and 6L PC targets are supported in MVP1",
                )
                .with_location(EvidenceLocation::new("pc.target"))
                .with_evidence(ValidationEvidence::new("lines", target.lines().to_string()))
                .with_suggested_next_step(SuggestedNextStep::new(
                    "Use a 2-line, 4-line, or 6-line PC target.",
                )),
            );
        }
        report
    }
}

pub fn validate_pc_target(target: PcTarget) -> DiagnosticReport {
    PcQueryValidator::validate_target(target)
}

pub fn validate_opening_pc_search_query(query: &OpeningPcSearchQuery) -> DiagnosticReport {
    PcQueryValidator::validate(query)
}

pub fn validate_pc_scenario_query(query: &PcScenarioQuery) -> DiagnosticReport {
    pc_scenario_query_validator::validate_pc_scenario_query(query)
}

pub(crate) fn validate_observation_objective_contract(
    observation: QueueObservationPolicy,
    objective: ObjectiveKind,
) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    if let Err(error) = validate_pc_observation_objective(observation, objective) {
        report.push(invalid_pc_query(
            "pc.queue_observation_policy",
            error.message(),
            error.code(),
        ));
    }
    report
}

fn validate_bag_contract(query: &OpeningPcSearchQuery, report: &mut DiagnosticReport) {
    if query.bag().piece_set_id() != query.piece_set().id() {
        report.push(invalid_pc_query(
            "pc.bag",
            "PC query bag profile must use the selected piece set profile",
            "bag_piece_set_mismatch",
        ));
    }

    if query.bag().bag_size() != 7 {
        report.push(invalid_pc_query(
            "pc.bag",
            "PC query MVP1 supports only the standard 7-bag profile",
            "unsupported_bag_size",
        ));
    }
}

fn pc_query_supported_diagnostic(query: &OpeningPcSearchQuery) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::IPcQueryMvpSupported,
        "PC search query is valid for the MVP1 PC search path",
    )
    .with_location(EvidenceLocation::new("pc.query"))
    .with_evidence(ValidationEvidence::new(
        "target_lines",
        query.target().lines().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "board_profile",
        query.board().id().as_str(),
    ))
    .with_evidence(ValidationEvidence::new(
        "piece_set_profile",
        query.piece_set().id().as_str(),
    ))
    .with_evidence(ValidationEvidence::new(
        "bag_profile",
        query.bag().id().as_str(),
    ))
    .with_evidence(ValidationEvidence::new("queue_mode", query.queue().mode()))
    .with_evidence(ValidationEvidence::new(
        "queue_len",
        query.queue().len().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "hold_enabled",
        query.hold_policy().is_enabled().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "rule_profile",
        query.rule().id().as_str(),
    ))
    .with_evidence(ValidationEvidence::new(
        "verified_kick_profile",
        query.verified_kick_profile().is_some().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "objective",
        format!("{:?}", query.objective().kind()),
    ))
    .with_evidence(ValidationEvidence::new(
        "requested_backend",
        query.execution_policy().requested_backend().as_str(),
    ))
    .with_evidence(ValidationEvidence::new(
        "execution_workers",
        query.execution_policy().workers().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "execution_deterministic",
        query.execution_policy().deterministic().to_string(),
    ))
}

fn invalid_pc_query(
    location: &'static str,
    message: &'static str,
    reason: &'static str,
) -> Diagnostic {
    Diagnostic::new(DiagnosticCode::EPcQueryInvalid, message)
        .with_location(EvidenceLocation::new(location))
        .with_evidence(ValidationEvidence::new("reason", reason))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Use the standard MVP1 PC defaults: standard 10 board, standard pieces, and standard 7-bag.",
        ))
}

#[cfg(test)]
#[path = "pc_query_validator_tests.rs"]
mod tests;
