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

    assert_eq!(backend_values, ["auto", "cpu", "gpu", "hybrid"]);
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

    let gpu = schema
        .backend_options()
        .iter()
        .find(|option| option.value() == "gpu")
        .expect("gpu option");
    assert!(!gpu.is_disabled());
    assert_eq!(
        gpu.localized_label()
            .expect("localized label")
            .key()
            .as_str(),
        "ui.backend.gpu.label"
    );
    assert!(gpu.disabled_reason().is_none());

    let hybrid = schema
        .backend_options()
        .iter()
        .find(|option| option.value() == "hybrid")
        .expect("hybrid option");
    assert!(!hybrid.is_disabled());
    assert!(hybrid.disabled_reason().is_none());

    assert_eq!(
        schema
            .backend_options()
            .iter()
            .map(|option| option.value())
            .collect::<Vec<_>>(),
        ["auto", "cpu", "gpu", "hybrid"]
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

    let gpu = schema
        .backend_presets()
        .iter()
        .find(|preset| preset.id() == "gpu")
        .expect("gpu");
    assert!(gpu.enabled());
    assert_eq!(
        gpu.localized_description()
            .resolve(TranslationCatalog::new(LanguageId::Ko))
            .text(),
        "frontier count 작업용 GPU 백엔드입니다."
    );
    assert_eq!(gpu.disabled_reason_code(), None);
    assert_eq!(gpu.requires_feature(), None);
    assert!(!gpu.supports_trace_enumeration());
    assert!(gpu.supports_total_count());
    assert!(gpu.supports_sample_traces());
    assert!(gpu.supports_deterministic());
    assert!(gpu.supports_frontier_budget());

    let hybrid = schema
        .backend_presets()
        .iter()
        .find(|preset| preset.id() == "hybrid")
        .expect("hybrid");
    assert!(hybrid.enabled());
    assert_eq!(hybrid.disabled_reason_code(), None);
    assert_eq!(hybrid.requires_feature(), None);
    assert!(!hybrid.supports_trace_enumeration());
    assert!(hybrid.supports_total_count());
    assert!(hybrid.supports_sample_traces());
    assert!(hybrid.supports_frontier_budget());
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
