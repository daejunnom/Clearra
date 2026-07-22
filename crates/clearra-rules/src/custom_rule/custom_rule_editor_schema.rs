use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

use crate::{
    kicks::{KickTableEntry, KickTableProfile, KickTransition},
    line_clear::LineClearPolicy,
    rotation::RotationSystem,
    spawn::SpawnProfile,
};

use super::{
    custom_rule_runtime::{
        CustomRuleBoardBackend, CustomRuleRuntimeFeature, LockReachabilityPolicy,
    },
    single_piece_id,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRuleSpawnRule {
    piece_id: String,
    spawn_profile: SpawnProfile,
}

impl CustomRuleSpawnRule {
    pub fn new(piece_id: impl Into<String>, spawn_profile: SpawnProfile) -> Self {
        Self {
            piece_id: piece_id.into(),
            spawn_profile,
        }
    }
}
impl CustomRuleSpawnRule {
    pub fn for_piece(piece: PieceKind, spawn_profile: SpawnProfile) -> Self {
        Self::new(piece.as_ascii().to_string(), spawn_profile)
    }
}
impl CustomRuleSpawnRule {
    pub fn piece_id(&self) -> &str {
        &self.piece_id
    }
}
impl CustomRuleSpawnRule {
    pub fn piece(&self) -> Option<PieceKind> {
        single_piece_id(self.piece_id())
    }
}
impl CustomRuleSpawnRule {
    pub fn spawn_profile(&self) -> SpawnProfile {
        self.spawn_profile
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRulePieceSpecificOverride {
    piece_id: String,
    kick_table_profile: Option<KickTableProfile>,
    spawn_profile: Option<SpawnProfile>,
}

impl CustomRulePieceSpecificOverride {
    pub fn new(piece_id: impl Into<String>) -> Self {
        Self {
            piece_id: piece_id.into(),
            kick_table_profile: None,
            spawn_profile: None,
        }
    }
}
impl CustomRulePieceSpecificOverride {
    pub fn with_kick_table_profile(mut self, kick_table_profile: KickTableProfile) -> Self {
        self.kick_table_profile = Some(kick_table_profile);
        self
    }
}
impl CustomRulePieceSpecificOverride {
    pub fn with_spawn_profile(mut self, spawn_profile: SpawnProfile) -> Self {
        self.spawn_profile = Some(spawn_profile);
        self
    }
}
impl CustomRulePieceSpecificOverride {
    pub fn piece_id(&self) -> &str {
        &self.piece_id
    }
}
impl CustomRulePieceSpecificOverride {
    pub fn piece(&self) -> Option<PieceKind> {
        single_piece_id(self.piece_id())
    }
}
impl CustomRulePieceSpecificOverride {
    pub fn kick_table_profile(&self) -> Option<&KickTableProfile> {
        self.kick_table_profile.as_ref()
    }
}
impl CustomRulePieceSpecificOverride {
    pub fn spawn_profile(&self) -> Option<SpawnProfile> {
        self.spawn_profile
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRuleEditorSchema {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) rotation_states: Vec<u8>,
    pub(crate) spawn_rules: Vec<CustomRuleSpawnRule>,
    pub(crate) kick_table_profile: KickTableProfile,
    pub(crate) first_success_order: Vec<KickTransition>,
    pub(crate) supports_180: bool,
    pub(crate) piece_specific_overrides: Vec<CustomRulePieceSpecificOverride>,
    pub(crate) line_clear_policy: LineClearPolicy,
    pub(crate) lock_reachability_mode: LockReachabilityPolicy,
    pub(crate) board_backends: Vec<CustomRuleBoardBackend>,
    pub(crate) runtime_features: Vec<CustomRuleRuntimeFeature>,
}

impl CustomRuleEditorSchema {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        rotation_states: Vec<u8>,
        spawn_rules: Vec<CustomRuleSpawnRule>,
        kick_table_profile: KickTableProfile,
        first_success_order: Vec<KickTransition>,
        supports_180: bool,
        piece_specific_overrides: Vec<CustomRulePieceSpecificOverride>,
        line_clear_policy: LineClearPolicy,
        lock_reachability_mode: LockReachabilityPolicy,
        board_backends: Vec<CustomRuleBoardBackend>,
        runtime_features: Vec<CustomRuleRuntimeFeature>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            rotation_states,
            spawn_rules,
            kick_table_profile,
            first_success_order,
            supports_180,
            piece_specific_overrides,
            line_clear_policy,
            lock_reachability_mode,
            board_backends,
            runtime_features,
        }
    }
}
impl CustomRuleEditorSchema {
    pub fn from_editor_draft(draft: CustomRuleEditorDraft) -> Self {
        let first_success_order = draft
            .kick_table_profile
            .entries()
            .iter()
            .map(KickTableEntry::transition)
            .collect();
        Self::new(
            draft.id,
            draft.label,
            RotationState::ALL
                .into_iter()
                .map(RotationState::quarter_turns)
                .collect(),
            PieceKind::STANDARD_TETROMINOES
                .into_iter()
                .map(|piece| CustomRuleSpawnRule::for_piece(piece, draft.spawn_profile))
                .collect(),
            draft.kick_table_profile,
            first_success_order,
            true,
            Vec::new(),
            draft.line_clear_policy,
            draft.lock_reachability_policy,
            vec![CustomRuleBoardBackend::Board64],
            vec![
                CustomRuleRuntimeFeature::CompactCDescriptor,
                CustomRuleRuntimeFeature::StandardTetrominoPieces,
                CustomRuleRuntimeFeature::Board64Search,
            ],
        )
    }
}
impl CustomRuleEditorSchema {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl CustomRuleEditorSchema {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl CustomRuleEditorSchema {
    pub fn rotation_states(&self) -> &[u8] {
        &self.rotation_states
    }
}
impl CustomRuleEditorSchema {
    pub fn spawn_rules(&self) -> &[CustomRuleSpawnRule] {
        &self.spawn_rules
    }
}
impl CustomRuleEditorSchema {
    pub fn kick_transitions(&self) -> &[KickTableEntry] {
        self.kick_table_profile.entries()
    }
}
impl CustomRuleEditorSchema {
    pub fn kick_table_profile(&self) -> &KickTableProfile {
        &self.kick_table_profile
    }
}
impl CustomRuleEditorSchema {
    pub fn first_success_order(&self) -> &[KickTransition] {
        &self.first_success_order
    }
}
impl CustomRuleEditorSchema {
    pub fn supports_180(&self) -> bool {
        self.supports_180
    }
}
impl CustomRuleEditorSchema {
    pub fn piece_specific_overrides(&self) -> &[CustomRulePieceSpecificOverride] {
        &self.piece_specific_overrides
    }
}
impl CustomRuleEditorSchema {
    pub fn line_clear_policy(&self) -> LineClearPolicy {
        self.line_clear_policy
    }
}
impl CustomRuleEditorSchema {
    pub fn lock_reachability_mode(&self) -> LockReachabilityPolicy {
        self.lock_reachability_mode
    }
}
impl CustomRuleEditorSchema {
    pub fn board_backends(&self) -> &[CustomRuleBoardBackend] {
        &self.board_backends
    }
}
impl CustomRuleEditorSchema {
    pub fn runtime_features(&self) -> &[CustomRuleRuntimeFeature] {
        &self.runtime_features
    }
}
impl CustomRuleEditorSchema {
    pub fn can_compile_to_c_descriptor(&self) -> bool {
        self.board_backends
            .iter()
            .any(|backend| *backend == CustomRuleBoardBackend::Board64)
            && self
                .runtime_features
                .contains(&CustomRuleRuntimeFeature::CompactCDescriptor)
            && self
                .runtime_features
                .contains(&CustomRuleRuntimeFeature::StandardTetrominoPieces)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRuleEditorDraft {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kick_table_profile: KickTableProfile,
    pub(crate) spawn_profile: SpawnProfile,
    rotation_system: RotationSystem,
    pub(crate) lock_reachability_policy: LockReachabilityPolicy,
    pub(crate) line_clear_policy: LineClearPolicy,
}

impl CustomRuleEditorDraft {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        kick_table_profile: KickTableProfile,
        spawn_profile: SpawnProfile,
        rotation_system: RotationSystem,
        lock_reachability_policy: LockReachabilityPolicy,
        line_clear_policy: LineClearPolicy,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kick_table_profile,
            spawn_profile,
            rotation_system,
            lock_reachability_policy,
            line_clear_policy,
        }
    }
}
impl CustomRuleEditorDraft {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl CustomRuleEditorDraft {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl CustomRuleEditorDraft {
    pub fn kick_table_profile(&self) -> &KickTableProfile {
        &self.kick_table_profile
    }
}
impl CustomRuleEditorDraft {
    pub fn spawn_profile(&self) -> SpawnProfile {
        self.spawn_profile
    }
}
impl CustomRuleEditorDraft {
    pub fn rotation_system(&self) -> RotationSystem {
        self.rotation_system
    }
}
impl CustomRuleEditorDraft {
    pub fn lock_reachability_policy(&self) -> LockReachabilityPolicy {
        self.lock_reachability_policy
    }
}
impl CustomRuleEditorDraft {
    pub fn line_clear_policy(&self) -> LineClearPolicy {
        self.line_clear_policy
    }
}
