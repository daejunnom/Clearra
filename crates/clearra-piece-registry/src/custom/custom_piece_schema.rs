use clearra_core_domain::piece::rotation::RotationState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceRotationBounds {
    rotation_state: RotationState,
    min_x: i8,
    max_x: i8,
    min_y: i8,
    max_y: i8,
}

impl PieceRotationBounds {
    pub const fn new(
        rotation_state: RotationState,
        min_x: i8,
        max_x: i8,
        min_y: i8,
        max_y: i8,
    ) -> Self {
        Self {
            rotation_state,
            min_x,
            max_x,
            min_y,
            max_y,
        }
    }
}
impl PieceRotationBounds {
    pub const fn rotation_state(self) -> RotationState {
        self.rotation_state
    }
}
impl PieceRotationBounds {
    pub const fn min_x(self) -> i8 {
        self.min_x
    }
}
impl PieceRotationBounds {
    pub const fn max_x(self) -> i8 {
        self.max_x
    }
}
impl PieceRotationBounds {
    pub const fn min_y(self) -> i8 {
        self.min_y
    }
}
impl PieceRotationBounds {
    pub const fn max_y(self) -> i8 {
        self.max_y
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceSpawnOffset {
    rotation_state: RotationState,
    x: i8,
    y: i8,
}

impl PieceSpawnOffset {
    pub const fn new(rotation_state: RotationState, x: i8, y: i8) -> Self {
        Self {
            rotation_state,
            x,
            y,
        }
    }
}
impl PieceSpawnOffset {
    pub const fn rotation_state(self) -> RotationState {
        self.rotation_state
    }
}
impl PieceSpawnOffset {
    pub const fn x(self) -> i8 {
        self.x
    }
}
impl PieceSpawnOffset {
    pub const fn y(self) -> i8 {
        self.y
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PieceSourceProvenance {
    source_label: Option<String>,
    source_url: Option<String>,
    license: Option<String>,
}

impl PieceSourceProvenance {
    pub fn new(
        source_label: Option<String>,
        source_url: Option<String>,
        license: Option<String>,
    ) -> Self {
        Self {
            source_label,
            source_url,
            license,
        }
    }
}
impl PieceSourceProvenance {
    pub fn unspecified() -> Self {
        Self::default()
    }
}
impl PieceSourceProvenance {
    pub fn source_label(&self) -> Option<&str> {
        self.source_label.as_deref()
    }
}
impl PieceSourceProvenance {
    pub fn source_url(&self) -> Option<&str> {
        self.source_url.as_deref()
    }
}
impl PieceSourceProvenance {
    pub fn license(&self) -> Option<&str> {
        self.license.as_deref()
    }
}
