//! SRP rationale: each exact accelerator is one request-owned CLI selection.
//! Reject duplicate and conflicting occurrences before typed lowering; this
//! contains no installation, eligibility, generation or solver policy.

use crate::{WebCommandError, WebCommandErrorCode};

#[derive(Default)]
pub(crate) struct ExactAcceleratorSelections {
    legal_board: Option<bool>,
    conditioned_reachability: Option<bool>,
}

impl ExactAcceleratorSelections {
    pub(crate) fn explicitly_enabled_option(&self) -> Option<&'static str> {
        if self.legal_board == Some(true) {
            Some("--legal-board")
        } else if self.conditioned_reachability == Some(true) {
            Some("--conditioned-reachability")
        } else {
            None
        }
    }

    pub(crate) fn legal_board(&mut self, enabled: bool) -> Result<bool, WebCommandError> {
        select_once(
            &mut self.legal_board,
            enabled,
            "--legal-board/--no-legal-board",
        )
    }

    pub(crate) fn conditioned_reachability(
        &mut self,
        enabled: bool,
    ) -> Result<bool, WebCommandError> {
        select_once(
            &mut self.conditioned_reachability,
            enabled,
            "--conditioned-reachability/--no-conditioned-reachability",
        )
    }
}

fn select_once(
    slot: &mut Option<bool>,
    enabled: bool,
    option: &str,
) -> Result<bool, WebCommandError> {
    if slot.is_some() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("{option} may be selected only once"),
        ));
    }
    *slot = Some(enabled);
    Ok(enabled)
}
