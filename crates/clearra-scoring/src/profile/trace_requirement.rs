#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TraceRequirement {
    #[default]
    None,
    PlacementTrace,
    FullDropTrace,
    KickEvidenceTrace,
}

impl TraceRequirement {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "none" | "disabled" => Some(Self::None),
            "placement-trace" => Some(Self::PlacementTrace),
            "full-drop-trace" => Some(Self::FullDropTrace),
            "kick-evidence-trace" => Some(Self::KickEvidenceTrace),
            _ => None,
        }
    }
}
impl TraceRequirement {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PlacementTrace => "placement-trace",
            Self::FullDropTrace => "full-drop-trace",
            Self::KickEvidenceTrace => "kick-evidence-trace",
        }
    }
}
