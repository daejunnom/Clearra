use crate::render::AppRenderModel;
use clearra_validation::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

pub(crate) fn objective_diagnostics_from_render_model(
    render_model: &AppRenderModel,
) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    let Some(result) = render_model.core_result() else {
        return report;
    };
    let Some(reason @ ("pattern_weight_model_not_materialized" | "pattern_weight_count_mismatch")) =
        result.field("objective_incomplete_reason")
    else {
        return report;
    };

    report.push(
        Diagnostic::new(
            DiagnosticCode::WObjectivePatternWeightModelNotMaterialized,
            "the objective was not reduced because its pattern weights were unavailable or inconsistent",
        )
        .with_location(EvidenceLocation::new("app_response.objective_result"))
        .with_evidence(ValidationEvidence::new(
            "objective_incomplete_reason",
            reason,
        ))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Materialize the PatternWeightModel for this PieceSource before requesting the objective.",
        )),
    );
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_executor::CoreExecutionResult;

    #[test]
    fn missing_objective_weights_emit_diagnostic() {
        let render_model = AppRenderModel::Scenario(CoreExecutionResult::new(
            vec![
                ("objective_complete".to_owned(), "false".to_owned()),
                (
                    "objective_incomplete_reason".to_owned(),
                    "pattern_weight_model_not_materialized".to_owned(),
                ),
            ],
            Vec::new(),
        ));

        let report = objective_diagnostics_from_render_model(&render_model);

        assert!(report.contains_code(DiagnosticCode::WObjectivePatternWeightModelNotMaterialized));
    }
}
