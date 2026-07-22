#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FumenCopyButtonModel {
    enabled: bool,
    copy_payload_kind: &'static str,
    disabled_reason: &'static str,
}

impl FumenCopyButtonModel {
    pub fn from_payload(payload: Option<&str>) -> Self {
        if payload.is_some_and(|payload| !payload.trim().is_empty()) {
            Self {
                enabled: true,
                copy_payload_kind: "fumen-like",
                disabled_reason: "none",
            }
        } else {
            Self {
                enabled: false,
                copy_payload_kind: "none",
                disabled_reason: "fumen_like_output_unavailable",
            }
        }
    }
}
impl FumenCopyButtonModel {
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}
impl FumenCopyButtonModel {
    pub const fn copy_payload_kind(&self) -> &'static str {
        self.copy_payload_kind
    }
}
impl FumenCopyButtonModel {
    pub const fn disabled_reason(&self) -> &'static str {
        self.disabled_reason
    }
}
