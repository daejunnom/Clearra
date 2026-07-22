use clearra_app::AppResponse;

use super::super::{field_value, first_field};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayPieceOwnershipView {
    available: bool,
    owned_cell_count: usize,
    ownership_source: String,
}

impl ReplayPieceOwnershipView {
    pub fn from_response(response: &AppResponse) -> Self {
        let owned_cell_count = first_field(
            response,
            &[
                "owned_cell_count",
                "colored_cell_ownership_count",
                "replay_owned_cell_count",
            ],
        )
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
        let ownership_source = field_value(response, "replay_ownership_source")
            .unwrap_or_else(|| "not_available".to_owned());

        Self {
            available: owned_cell_count > 0 || ownership_source != "not_available",
            owned_cell_count,
            ownership_source,
        }
    }
}
impl ReplayPieceOwnershipView {
    pub const fn available(&self) -> bool {
        self.available
    }
}
impl ReplayPieceOwnershipView {
    pub const fn owned_cell_count(&self) -> usize {
        self.owned_cell_count
    }
}
impl ReplayPieceOwnershipView {
    pub fn ownership_source(&self) -> &str {
        &self.ownership_source
    }
}
