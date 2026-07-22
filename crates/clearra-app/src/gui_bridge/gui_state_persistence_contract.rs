pub struct GuiStatePersistenceContract;

impl GuiStatePersistenceContract {
    pub fn preference_path() -> &'static str {
        "app-config/clearra-gui/preferences.json"
    }
}
impl GuiStatePersistenceContract {
    pub fn stable_keys() -> &'static [&'static str] {
        &[
            "schema_version",
            "language",
            "backend",
            "recent_problem_preset",
            "workers",
            "allow_backend_fallback",
            "deterministic",
            "default_output_format",
            "last_opened_fixture_dir",
            "theme",
        ]
    }
}
