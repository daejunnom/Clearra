use crate::profile::{SpinProfileId, SpinProfileRegistry};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinClassifierDescriptor {
    id: &'static str,
    display_name: &'static str,
    all_piece_classifier: bool,
    profile_specific_exact_available: bool,
}

impl SpinClassifierDescriptor {
    pub const fn new(
        id: &'static str,
        display_name: &'static str,
        all_piece_classifier: bool,
        profile_specific_exact_available: bool,
    ) -> Self {
        Self {
            id,
            display_name,
            all_piece_classifier,
            profile_specific_exact_available,
        }
    }
}
impl SpinClassifierDescriptor {
    pub const fn id(self) -> &'static str {
        self.id
    }
}
impl SpinClassifierDescriptor {
    pub const fn display_name(self) -> &'static str {
        self.display_name
    }
}
impl SpinClassifierDescriptor {
    pub const fn all_piece_classifier(self) -> bool {
        self.all_piece_classifier
    }
}
impl SpinClassifierDescriptor {
    pub const fn profile_specific_exact_available(self) -> bool {
        self.profile_specific_exact_available
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpinClassifierRegistry;

impl SpinClassifierRegistry {
    pub fn builtins() -> Vec<SpinClassifierDescriptor> {
        let mut descriptors = vec![
            SpinClassifierDescriptor::new("disabled", "Disabled", false, true),
            SpinClassifierDescriptor::new("t-spin-simple", "T-spin simple", false, false),
        ];
        descriptors.extend(
            SpinProfileRegistry::builtins()
                .profiles()
                .iter()
                .map(|profile| {
                    let id = profile.id();
                    SpinClassifierDescriptor::new(
                        id.as_str(),
                        id.display_name(),
                        id.recognizes_non_t_immobile_spins(),
                        true,
                    )
                }),
        );
        descriptors.extend([
            SpinClassifierDescriptor::new(
                "kick-sensitive-special",
                "Kick-sensitive special",
                true,
                false,
            ),
            SpinClassifierDescriptor::new(
                "source-pinned-special",
                "Source-pinned special",
                true,
                false,
            ),
        ]);
        descriptors
    }
}
impl SpinClassifierRegistry {
    pub fn get(id: &str) -> Option<SpinClassifierDescriptor> {
        let normalized = normalize_id(id);
        Self::builtins()
            .into_iter()
            .find(|descriptor| descriptor.id() == normalized)
    }
}
impl SpinClassifierRegistry {
    pub fn supports_all_piece(id: &str) -> bool {
        Self::get(id).is_some_and(|descriptor| descriptor.all_piece_classifier())
    }
}

fn normalize_id(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "t-spin-corner-based" | "t-spin-3-corner" => SpinProfileId::TSpins.as_str().to_owned(),
        "all-piece-spin" => SpinProfileId::AllSpin.as_str().to_owned(),
        "srs-plus-all-mini" => SpinProfileId::AllMiniPlus.as_str().to_owned(),
        _ => normalized,
    }
}
