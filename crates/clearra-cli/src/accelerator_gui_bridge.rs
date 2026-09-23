//! In-process Desktop adapter for the two explicit native asset lifecycles.
//! The ordinary check/status/remove actions use the same CLI parser and JSON
//! response. Only download needs a cancellation/progress observer; it calls
//! the same underlying signed store rather than launching another CLI.

use std::sync::atomic::AtomicBool;

use crate::{exit::ExitCode, run_with_args};

const PROFILES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];

/// Desktop executes the same exact AppRequest but does not pass through the
/// CLI router. Activate only its selected profile before the GUI job starts;
/// any unavailable or invalid asset remains an exact-search cache miss.
pub fn activate_native_accelerators_for_request(request: &clearra_app::AppRequest) {
    crate::legal_board_assets::activate_for_request(request);
    crate::conditioned_reachability_assets::activate_for_request(request);
}

pub fn run_native_accelerator_action(
    product: &str,
    action: &str,
    profile: &str,
    cancelled: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<String, String> {
    if !PROFILES.contains(&profile) {
        return Err("accelerator: unknown kick-table profile".to_owned());
    }
    let command = match product {
        "exact-legal-board" => "legal-board",
        "board-conditioned-reachability" => "reachability-pack",
        _ => return Err("accelerator: unknown product".to_owned()),
    };
    if !matches!(action, "check" | "status" | "download" | "remove") {
        return Err("accelerator: unsupported lifecycle action".to_owned());
    }
    if action == "download" {
        let value = match product {
            "exact-legal-board" => {
                crate::legal_board_assets::download_observed(profile, cancelled, progress)
            }
            "board-conditioned-reachability" => {
                crate::conditioned_reachability_assets::download_observed(
                    profile, cancelled, progress,
                )
            }
            _ => unreachable!(),
        }
        .map_err(str::to_owned)?;
        return Ok(value.to_string());
    }
    let output = run_with_args([
        "clearra",
        "--format",
        "json",
        command,
        action,
        "--profile",
        profile,
    ]);
    if output.exit_code() != ExitCode::Success {
        return Err(output.stderr().to_owned());
    }
    Ok(output.stdout().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_boundary_rejects_unknown_identity_before_io() {
        let cancelled = AtomicBool::new(false);
        let mut progress = |_, _| {};
        assert!(run_native_accelerator_action(
            "exact-legal-board",
            "check",
            "other",
            &cancelled,
            &mut progress
        )
        .is_err());
        assert!(
            run_native_accelerator_action("other", "check", "srs", &cancelled, &mut progress)
                .is_err()
        );
        assert!(run_native_accelerator_action(
            "exact-legal-board",
            "generate",
            "srs",
            &cancelled,
            &mut progress
        )
        .is_err());
    }
}
