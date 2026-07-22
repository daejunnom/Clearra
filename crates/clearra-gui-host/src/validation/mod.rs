mod gui_backend_validator;
mod gui_file_path_validator;
mod gui_form_validator;
mod gui_render_validator;
mod gui_validation_diagnostic;
mod gui_validation_summary;

// G9 validation contract markers: lines > 0, queue piece valid, field mask valid,
// backend supported or fallback allowed, fixture file path safe, skin asset valid,
// render option supported.
pub use gui_backend_validator::GuiBackendValidator;
pub use gui_file_path_validator::GuiFilePathValidator;
pub use gui_form_validator::GuiFormValidator;
pub use gui_render_validator::GuiRenderValidator;
pub use gui_validation_diagnostic::GuiValidationDiagnostic;
pub use gui_validation_summary::GuiValidationSummary;
