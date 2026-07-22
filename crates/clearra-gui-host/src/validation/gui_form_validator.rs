use crate::{
    model::{GuiAppState, GuiProblemForm},
    validation::{
        GuiBackendValidator, GuiRenderValidator, GuiValidationDiagnostic, GuiValidationSummary,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GuiFormValidator;

impl GuiFormValidator {
    pub fn validate_state(state: &GuiAppState) -> GuiValidationSummary {
        let mut summary = GuiValidationSummary::new();
        Self::validate_problem_form(state.problem_form(), &mut summary);
        summary.append(GuiBackendValidator::validate(state.backend_form()));
        summary.append(GuiRenderValidator::validate(state.render_form()));
        summary
    }
}
impl GuiFormValidator {
    fn validate_problem_form(form: &GuiProblemForm, summary: &mut GuiValidationSummary) {
        if form.selected_lines() == 0 {
            summary.push(GuiValidationDiagnostic::invalid_form(
                "lines",
                "GUI problem form requires lines > 0",
            ));
        }

        match form {
            GuiProblemForm::OpeningPc(opening) => {
                if opening.lines() == 0 {
                    summary.push(GuiValidationDiagnostic::invalid_form(
                        "opening_pc.lines",
                        "opening PC requires lines > 0",
                    ));
                }
            }
            GuiProblemForm::ScenarioPc(scenario) => {
                validate_piece_queue(
                    scenario.remaining_queue(),
                    "scenario.remaining_queue",
                    summary,
                );
                validate_field_mask(
                    scenario.visible_height(),
                    scenario.initial_board_mask(),
                    summary,
                );
            }
            GuiProblemForm::SetupSearch(setup) => {
                validate_piece_queue(setup.queue(), "setup.queue", summary);
            }
            GuiProblemForm::BuildCoverage(cover) => {
                if cover.template_id().trim().is_empty() {
                    summary.push(GuiValidationDiagnostic::invalid_form(
                        "build_coverage.template_id",
                        "build coverage requires a template id",
                    ));
                }
            }
        }
    }
}

fn validate_piece_queue(value: &str, field: &str, summary: &mut GuiValidationSummary) {
    let mut seen_piece = false;
    for (index, character) in value.chars().enumerate() {
        if character.is_whitespace() || character == ',' {
            continue;
        }
        seen_piece = true;
        if !"IOTSZJL".contains(character) {
            summary.push(GuiValidationDiagnostic::invalid_form(
                field,
                format!("queue piece valid check failed at index {index}: '{character}'"),
            ));
        }
    }
    if !seen_piece {
        summary.push(GuiValidationDiagnostic::invalid_form(
            field,
            "queue must contain at least one valid piece",
        ));
    }
}

fn validate_field_mask(visible_height: u8, mask: u64, summary: &mut GuiValidationSummary) {
    if visible_height == 0 {
        summary.push(GuiValidationDiagnostic::invalid_form(
            "scenario.visible_height",
            "scenario visible height requires lines > 0",
        ));
        return;
    }

    let cell_count = u32::from(visible_height) * 10;
    if cell_count > 64 {
        summary.push(GuiValidationDiagnostic::invalid_form(
            "scenario.initial_board_mask",
            "field mask valid check failed: visible board exceeds Board64",
        ));
        return;
    }

    let allowed = if cell_count == 64 {
        u64::MAX
    } else {
        (1u64 << cell_count) - 1
    };
    if mask & !allowed != 0 {
        summary.push(GuiValidationDiagnostic::invalid_form(
            "scenario.initial_board_mask",
            "field mask valid check failed: mask contains cells outside visible rows",
        ));
    }
}
