#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpawnProfileId {
    Standard10,
    Arika,
    Custom,
}

impl SpawnProfileId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Standard10 => "standard-10-spawn",
            Self::Arika => "arika-spawn",
            Self::Custom => "custom-spawn",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpawnProfile {
    id: SpawnProfileId,
    x: i16,
    y: i16,
}

impl SpawnProfile {
    pub const STANDARD_10: Self = Self {
        id: SpawnProfileId::Standard10,
        x: 4,
        y: 20,
    };
}
impl SpawnProfile {
    pub const fn new(x: i16, y: i16) -> Self {
        Self {
            id: SpawnProfileId::Custom,
            x,
            y,
        }
    }
}
impl SpawnProfile {
    pub const fn arika(x: i16, y: i16) -> Self {
        Self {
            id: SpawnProfileId::Arika,
            x,
            y,
        }
    }
}
impl SpawnProfile {
    pub fn id(self) -> SpawnProfileId {
        self.id
    }
}
impl SpawnProfile {
    pub fn x(self) -> i16 {
        self.x
    }
}
impl SpawnProfile {
    pub fn y(self) -> i16 {
        self.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_spawn_profile_has_stable_identity() {
        let profile = SpawnProfile::STANDARD_10;

        assert_eq!(profile.id(), SpawnProfileId::Standard10);
        assert_eq!(profile.id().as_str(), "standard-10-spawn");
        assert_eq!(profile.x(), 4);
        assert_eq!(profile.y(), 20);
    }
}
