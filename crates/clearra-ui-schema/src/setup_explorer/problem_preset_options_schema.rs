use clearra_i18n::TranslationKey;
use clearra_problem::SearchProblemPreset;

use crate::i18n::LocalizedLabelSchema;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProblemPresetOptionsSchema {
    options: Vec<ProblemPresetOptionSchema>,
}

impl ProblemPresetOptionsSchema {
    pub fn m28() -> Self {
        Self {
            options: [
                SearchProblemPreset::OpeningPc,
                SearchProblemPreset::ScenarioPc,
                SearchProblemPreset::Setup,
                SearchProblemPreset::Build,
            ]
            .into_iter()
            .map(ProblemPresetOptionSchema::from_preset)
            .collect(),
        }
    }
}
impl ProblemPresetOptionsSchema {
    pub fn options(&self) -> &[ProblemPresetOptionSchema] {
        &self.options
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProblemPresetOptionSchema {
    id: String,
    label: String,
    localized_label: LocalizedLabelSchema,
    description: String,
    enabled: bool,
    output_contract_fields: Vec<String>,
}

impl ProblemPresetOptionSchema {
    pub fn from_preset(preset: SearchProblemPreset) -> Self {
        let (label, description, output_contract_fields) = match preset {
            SearchProblemPreset::OpeningPc => (
                "Opening PC",
                "Empty-board perfect clear preset compiled into SearchProblem.",
                vec![
                    "total_solution_count",
                    "retained_trace_count",
                    "coverage_probability",
                    "backend_fallback_reason",
                ],
            ),
            SearchProblemPreset::ScenarioPc => (
                "Scenario PC",
                "Clear-to-empty scenario preset for setup continuation analysis.",
                vec![
                    "total_solution_count",
                    "retained_trace_count",
                    "coverage_probability",
                    "unsupported_reason",
                ],
            ),
            SearchProblemPreset::Setup => (
                "Setup Search",
                "Setup family, tiling, build, raw metrics, and post-PC explorer preset.",
                vec![
                    "raw_metrics_export",
                    "setup_raw_coverage_export",
                    "score_evaluation_basis",
                    "coverage_probability",
                ],
            ),
            SearchProblemPreset::Build => (
                "Build Coverage",
                "Build template coverage preset through slot assignment and BuildUp.",
                vec![
                    "packing_candidate_count",
                    "build_variant_count",
                    "coverage_probability",
                ],
            ),
        };

        Self {
            id: preset.as_str().to_owned(),
            label: label.to_owned(),
            localized_label: LocalizedLabelSchema::new(problem_preset_label_key(preset), label),
            description: description.to_owned(),
            enabled: true,
            output_contract_fields: output_contract_fields
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }
}
impl ProblemPresetOptionSchema {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl ProblemPresetOptionSchema {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl ProblemPresetOptionSchema {
    pub fn localized_label(&self) -> &LocalizedLabelSchema {
        &self.localized_label
    }
}
impl ProblemPresetOptionSchema {
    pub fn description(&self) -> &str {
        &self.description
    }
}
impl ProblemPresetOptionSchema {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
impl ProblemPresetOptionSchema {
    pub fn output_contract_fields(&self) -> &[String] {
        &self.output_contract_fields
    }
}

fn problem_preset_label_key(preset: SearchProblemPreset) -> TranslationKey {
    match preset {
        SearchProblemPreset::OpeningPc => TranslationKey::new("ui.problem.opening_pc.label"),
        SearchProblemPreset::ScenarioPc => TranslationKey::new("ui.problem.scenario_pc.label"),
        SearchProblemPreset::Setup => TranslationKey::new("ui.problem.setup.label"),
        SearchProblemPreset::Build => TranslationKey::new("ui.problem.build.label"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_preset_options_use_search_problem_preset_ids() {
        let schema = ProblemPresetOptionsSchema::m28();
        let ids = schema
            .options()
            .iter()
            .map(ProblemPresetOptionSchema::id)
            .collect::<Vec<_>>();

        assert_eq!(ids, ["opening-pc", "scenario-pc", "setup", "build"]);
        assert!(schema
            .options()
            .iter()
            .all(ProblemPresetOptionSchema::enabled));
        assert!(schema.options()[1]
            .output_contract_fields()
            .iter()
            .any(|field| field == "unsupported_reason"));
        assert_eq!(
            schema.options()[0].localized_label().key().as_str(),
            "ui.problem.opening_pc.label"
        );
    }
}
