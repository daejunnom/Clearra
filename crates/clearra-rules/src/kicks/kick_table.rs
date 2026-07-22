use std::collections::HashSet;

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

use crate::{profile::rule_profile::RuleProfileId, rotation::RotationRequest};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct KickOffset {
    dx: i8,
    dy: i8,
}

impl KickOffset {
    pub const fn new(dx: i8, dy: i8) -> Self {
        Self { dx, dy }
    }
}
impl KickOffset {
    pub fn dx(self) -> i8 {
        self.dx
    }
}
impl KickOffset {
    pub fn dy(self) -> i8 {
        self.dy
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickOffsetSequence {
    offsets: Vec<KickOffset>,
}

impl KickOffsetSequence {
    pub fn new(offsets: Vec<KickOffset>) -> Self {
        Self { offsets }
    }
}
impl KickOffsetSequence {
    pub fn no_kick() -> Self {
        Self::new(vec![KickOffset::new(0, 0)])
    }
}
impl KickOffsetSequence {
    pub fn offsets(&self) -> &[KickOffset] {
        &self.offsets
    }
}
impl KickOffsetSequence {
    pub fn len(&self) -> usize {
        self.offsets.len()
    }
}
impl KickOffsetSequence {
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KickTransition {
    piece: PieceKind,
    from: RotationState,
    to: RotationState,
}

impl KickTransition {
    pub const fn new(piece: PieceKind, from: RotationState, to: RotationState) -> Self {
        Self { piece, from, to }
    }
}
impl KickTransition {
    pub fn piece(self) -> PieceKind {
        self.piece
    }
}
impl KickTransition {
    pub fn from(self) -> RotationState {
        self.from
    }
}
impl KickTransition {
    pub fn to(self) -> RotationState {
        self.to
    }
}
impl KickTransition {
    pub fn rotation_request(self) -> RotationRequest {
        RotationRequest::new(self.from, self.to)
    }
}
impl KickTransition {
    pub fn is_180(self) -> bool {
        self.rotation_request().is_180()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickTableEntry {
    transition: KickTransition,
    sequence: KickOffsetSequence,
    unsupported_reason: Option<String>,
}

impl KickTableEntry {
    pub fn new(transition: KickTransition, sequence: KickOffsetSequence) -> Self {
        Self {
            transition,
            sequence,
            unsupported_reason: None,
        }
    }
}
impl KickTableEntry {
    pub fn with_unsupported_reason(mut self, reason: impl Into<String>) -> Self {
        self.unsupported_reason = Some(reason.into());
        self
    }
}
impl KickTableEntry {
    pub fn transition(&self) -> KickTransition {
        self.transition
    }
}
impl KickTableEntry {
    pub fn sequence(&self) -> &KickOffsetSequence {
        &self.sequence
    }
}
impl KickTableEntry {
    pub fn unsupported_reason(&self) -> Option<&str> {
        self.unsupported_reason.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KickTableProfileId {
    Srs90,
    NoKick,
    SrsPlus,
    SrsX,
    Asc,
    Ars,
    Imported,
    Custom,
}

impl KickTableProfileId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Srs90 => "srs-90",
            Self::NoKick => "no-kick",
            Self::SrsPlus => "srs-plus",
            Self::SrsX => "srs-x",
            Self::Asc => "asc",
            Self::Ars => "ars",
            Self::Imported => "imported",
            Self::Custom => "custom",
        }
    }
}
impl KickTableProfileId {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "srs-90" | "srs" => Some(Self::Srs90),
            "no-kick" | "nokick" => Some(Self::NoKick),
            "srs-plus" => Some(Self::SrsPlus),
            "srs-x" => Some(Self::SrsX),
            "asc" => Some(Self::Asc),
            "ars" => Some(Self::Ars),
            "imported" => Some(Self::Imported),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickTableProfile {
    id: KickTableProfileId,
    source_rule: RuleProfileId,
    entries: Vec<KickTableEntry>,
}

impl KickTableProfile {
    pub fn new(
        id: KickTableProfileId,
        source_rule: RuleProfileId,
        entries: Vec<KickTableEntry>,
    ) -> Self {
        Self {
            id,
            source_rule,
            entries,
        }
    }
}
impl KickTableProfile {
    pub fn id(&self) -> KickTableProfileId {
        self.id
    }
}
impl KickTableProfile {
    pub fn source_rule(&self) -> RuleProfileId {
        self.source_rule
    }
}
impl KickTableProfile {
    pub fn entries(&self) -> &[KickTableEntry] {
        &self.entries
    }
}
impl KickTableProfile {
    pub fn sequence_for(&self, transition: KickTransition) -> Option<&KickOffsetSequence> {
        self.entries
            .iter()
            .find(|entry| entry.transition() == transition)
            .map(KickTableEntry::sequence)
    }
}
impl KickTableProfile {
    pub fn supports_180(&self) -> bool {
        self.entries()
            .iter()
            .any(|entry| entry.transition().is_180() && !entry.sequence().is_empty())
    }
}
impl KickTableProfile {
    pub fn transition_count(&self) -> usize {
        self.entries.len()
    }
}
impl KickTableProfile {
    pub fn duplicate_transitions(&self) -> Vec<KickTransition> {
        let mut seen = HashSet::new();
        let mut duplicates = HashSet::new();
        for entry in self.entries() {
            let transition = entry.transition();
            if !seen.insert(transition) {
                duplicates.insert(transition);
            }
        }
        let mut duplicate_transitions = duplicates.into_iter().collect::<Vec<_>>();
        duplicate_transitions.sort_by_key(|transition| {
            (
                transition.piece(),
                transition.from().quarter_turns(),
                transition.to().quarter_turns(),
            )
        });
        duplicate_transitions
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickTable {
    sequence: KickOffsetSequence,
}

impl KickTable {
    pub fn new(offsets: Vec<KickOffset>) -> Self {
        Self {
            sequence: KickOffsetSequence::new(offsets),
        }
    }
}
impl KickTable {
    pub fn from_sequence(sequence: KickOffsetSequence) -> Self {
        Self { sequence }
    }
}
impl KickTable {
    pub fn no_kick() -> Self {
        Self::from_sequence(KickOffsetSequence::no_kick())
    }
}
impl KickTable {
    pub fn offsets(&self) -> &[KickOffset] {
        self.sequence.offsets()
    }
}
impl KickTable {
    pub fn sequence(&self) -> &KickOffsetSequence {
        &self.sequence
    }
}

#[cfg(test)]
#[path = "kick_table_tests.rs"]
mod tests;
