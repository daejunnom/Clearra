pub const KEYS: &[&str] = &[
    "ui.language.selector.label",
    "ui.backend.auto.label",
    "ui.backend.auto.description",
    "ui.backend.cpu.label",
    "ui.backend.cpu.description",
    "ui.backend.gpu.label",
    "ui.backend.gpu.description",
    "ui.backend.hybrid.label",
    "ui.backend.hybrid.description",
    "ui.setup.result.total_solution_count",
    "ui.setup.result.retained_trace_count",
    "ui.setup.result.count_complete",
    "ui.setup.result.continue_available",
    "ui.setup.result.backend_fallback_reason",
    "ui.setup.result.coverage_probability",
    "ui.diagnostic.backend_fallback_used",
    "ui.problem.opening_pc.label",
    "ui.problem.scenario_pc.label",
    "ui.problem.setup.label",
    "ui.problem.build.label",
    "cli.help.top_level",
];

pub fn get(key: &str) -> Option<&'static str> {
    Some(match key {
        "ui.language.selector.label" => "Language",
        "ui.backend.auto.label" => "Auto",
        "ui.backend.auto.description" => "Selects the safest available backend for the query.",
        "ui.backend.cpu.label" => "CPU",
        "ui.backend.cpu.description" => {
            "User-facing CPU backend policy using the stable CPU executor path."
        }
        "ui.backend.gpu.label" => "GPU",
        "ui.backend.gpu.description" => {
            "User-facing GPU backend policy for frontier-count workloads."
        }
        "ui.backend.hybrid.label" => "Hybrid",
        "ui.backend.hybrid.description" => {
            "User-facing hybrid backend policy for GPU candidates plus CPU BuildUp."
        }
        "ui.setup.result.total_solution_count" => "Total solutions",
        "ui.setup.result.retained_trace_count" => "Retained traces",
        "ui.setup.result.count_complete" => "Count complete",
        "ui.setup.result.continue_available" => "Continue available",
        "ui.setup.result.backend_fallback_reason" => "Fallback reason",
        "ui.setup.result.coverage_probability" => "Coverage",
        "ui.diagnostic.backend_fallback_used" => "Backend fallback used",
        "ui.problem.opening_pc.label" => "Opening PC",
        "ui.problem.scenario_pc.label" => "Scenario PC",
        "ui.problem.setup.label" => "Setup",
        "ui.problem.build.label" => "Build",
        "cli.help.top_level" => "Clearra command line",
        _ => return None,
    })
}
