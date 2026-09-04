use clearra_gui_host::DesktopTauriCommandBridge;
use serde_json::json;

fn validate(
    value: serde_json::Value,
) -> Result<String, clearra_gui_host::DesktopTauriCommandError> {
    DesktopTauriCommandBridge::default().validate_request(&value.to_string())
}

#[test]
fn production_entrypoint_accepts_only_the_complete_cli_envelope() {
    let canonical = json!({
        "app_request_model": "clearra-cli/CommandRequest",
        "command": "cli",
        "language": "ko",
        "arguments": ["clearra", "pc", "tiling", "--lines", "2", "--patterns", "P7"],
    });
    assert!(validate(canonical).is_ok());

    for rejected in [
        json!({
            "app_request_model": "clearra-app/AppRequest",
            "command": "pc",
            "lines": 2,
        }),
        json!({
            "app_request_model": "clearra-cli/CommandRequest",
            "command": "cli",
            "language": "en",
        }),
        json!({
            "app_request_model": "clearra-cli/CommandRequest",
            "command": "cli",
            "language": "en",
            "arguments": ["clearra", "pc", "tiling", "--lines", "2"],
            "lines": 2,
        }),
    ] {
        let error = validate(rejected).expect_err("non-CLI desktop envelope must fail closed");
        assert_eq!(error.code(), "desktop-invalid-request");
    }
}

#[test]
fn production_entrypoint_does_not_reexpose_retired_gui_save_products() {
    for product in ["saves", "best-save"] {
        let error = validate(json!({
            "app_request_model": "clearra-cli/CommandRequest",
            "command": "cli",
            "language": "en",
            "arguments": ["clearra", "pc", product, "--lines", "2"],
        }))
        .expect_err("retired GUI save product must fail closed");
        assert_eq!(error.code(), "desktop-invalid-request");
    }
}

#[test]
fn production_entrypoint_preserves_literal_exact_argv_without_shell_interpretation() {
    let comment = "literal | && ` $(x) > < ; &\tline\nquote\" slash\\ bell\u{0007}";
    assert!(validate(json!({
        "app_request_model": "clearra-cli/CommandRequest",
        "command": "cli",
        "language": "en",
        "arguments": [
            "clearra",
            "utility",
            "fumen",
            "text-to-fumen",
            "--format",
            "fumen",
            "--comment",
            comment,
        ],
    }))
    .is_ok());
}

#[test]
fn production_entrypoint_rejects_nul_in_exact_argv() {
    let error = validate(json!({
        "app_request_model": "clearra-cli/CommandRequest",
        "command": "cli",
        "language": "en",
        "arguments": [
            "clearra",
            "utility",
            "fumen",
            "text-to-fumen",
            "--format",
            "fumen",
            "--comment",
            "left\0right",
        ],
    }))
    .expect_err("NUL must not cross the production Desktop exact-argv boundary");
    assert_eq!(error.code(), "desktop-invalid-request");
}
