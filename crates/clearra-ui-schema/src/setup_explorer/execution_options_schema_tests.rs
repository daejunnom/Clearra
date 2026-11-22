use clearra_i18n::{LanguageId, TranslationCatalog};
use clearra_profiles::search::search_defaults::SearchDefaults;

use crate::dropdown::DropdownOption;

use crate::setup_explorer::{BackendPresetSchema, ExecutionOptionsSchema};

#[test]
fn execution_options_schema_uses_canonical_backends_and_defaults() {
    let schema = ExecutionOptionsSchema::mvp2();
    let backend_values = schema
        .backend_options()
        .iter()
        .map(DropdownOption::value)
        .collect::<Vec<_>>();

    assert_eq!(backend_values, ["auto", "cpu"]);
    let preset_values = schema
        .backend_presets()
        .iter()
        .map(BackendPresetSchema::id)
        .collect::<Vec<_>>();
    assert_eq!(preset_values, backend_values);
    assert!(schema.deterministic_default());
    assert!(schema.allow_backend_fallback_default());
    assert_eq!(
        schema.max_frontier_states_default(),
        SearchDefaults::MVP1.execution_max_frontier_states()
    );
    assert_eq!(
        schema.max_memory_mib_default(),
        SearchDefaults::MVP1.execution_max_memory_mib()
    );

    assert_eq!(
        schema
            .backend_options()
            .iter()
            .map(|option| option.value())
            .collect::<Vec<_>>(),
        ["auto", "cpu"]
    );
}

#[test]
fn backend_presets_expose_user_facing_capabilities() {
    let schema = ExecutionOptionsSchema::mvp2();

    let cpu = schema
        .backend_presets()
        .iter()
        .find(|preset| preset.id() == "cpu")
        .expect("cpu");
    assert!(cpu.enabled());
    assert_eq!(cpu.localized_label().key().as_str(), "ui.backend.cpu.label");
    assert_eq!(
        cpu.localized_label()
            .resolve(TranslationCatalog::new(LanguageId::Ko))
            .text(),
        "CPU"
    );
    assert!(cpu.supports_trace_enumeration());
    assert!(cpu.supports_total_count());
    assert!(cpu.supports_sample_traces());
    assert!(cpu.supports_deterministic());
    assert!(cpu.supports_frontier_budget());
}

#[test]
fn gpu_device_options_are_populated_from_runtime_inventory() {
    let schema = ExecutionOptionsSchema::mvp2()
        .with_gpu_device_inventory([(0, "Integrated adapter"), (3, "Discrete adapter")]);

    let options = schema.gpu_device_options();
    assert_eq!(
        options
            .iter()
            .map(DropdownOption::value)
            .collect::<Vec<_>>(),
        ["auto", "0", "3"]
    );
    assert_eq!(options[2].label(), "Discrete adapter");
}
