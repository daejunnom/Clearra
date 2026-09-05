use core::cmp::Ordering;

use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    hold::hold_policy::HoldPolicy,
    pattern_universe::PieceMultisetKey,
    piece_source::{PieceSourceId, PieceSourceKind},
    QueueObservationPolicy,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SupplyProvenanceId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SupplyObservationIdentity {
    pub policy: QueueObservationPolicy,
    pub observation_id: u64,
}

impl SupplyObservationIdentity {
    pub const fn new(policy: QueueObservationPolicy, observation_id: u64) -> Self {
        Self {
            policy,
            observation_id,
        }
    }

    pub const fn full_queue_oracle() -> Self {
        Self::new(QueueObservationPolicy::FullQueueOracle, 0)
    }

    const fn policy_tag(self) -> u8 {
        match self.policy {
            QueueObservationPolicy::FullQueueOracle => 1,
            QueueObservationPolicy::VisibleSeven => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SupplyHoldState {
    Disabled,
    Empty,
    Occupied(PieceKind),
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SupplyBranchKind {
    Current,
    SwapHeld,
    StoreCurrent,
}

impl SupplyBranchKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::SwapHeld => "swap-held",
            Self::StoreCurrent => "store-current",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SupplyExecutionState {
    pub piece_source_id: PieceSourceId,
    pub source_kind: PieceSourceKind,
    pub cursor: u16,
    pub hold_piece: Option<PieceKind>,
    pub hold_empty: bool,
    pub hold_policy: HoldPolicy,
    pub bag_epoch: u16,
    pub bag_remainder_key: u64,
    pub observation: SupplyObservationIdentity,
    pub provenance: SupplyProvenanceId,
}

impl SupplyExecutionState {
    pub fn new(
        piece_source_id: PieceSourceId,
        cursor: u16,
        hold_piece: Option<PieceKind>,
        bag_epoch: u16,
        bag_remainder_key: u64,
        provenance: SupplyProvenanceId,
    ) -> Self {
        Self::with_contract(
            piece_source_id,
            PieceSourceKind::MaterializedPatternUniverse,
            cursor,
            hold_piece,
            HoldPolicy::Allowed,
            bag_epoch,
            bag_remainder_key,
            SupplyObservationIdentity::full_queue_oracle(),
            provenance,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_contract(
        piece_source_id: PieceSourceId,
        source_kind: PieceSourceKind,
        cursor: u16,
        hold_piece: Option<PieceKind>,
        hold_policy: HoldPolicy,
        bag_epoch: u16,
        bag_remainder_key: u64,
        observation: SupplyObservationIdentity,
        provenance: SupplyProvenanceId,
    ) -> Self {
        Self {
            piece_source_id,
            source_kind,
            cursor,
            hold_piece,
            hold_empty: hold_piece.is_none(),
            hold_policy,
            bag_epoch,
            bag_remainder_key,
            observation,
            provenance,
        }
    }

    pub fn memo_key(self) -> SupplyExecutionMemoKey {
        SupplyExecutionMemoKey {
            piece_source_id: self.piece_source_id,
            source_kind: self.source_kind,
            cursor: self.cursor,
            hold_piece: self.hold_piece,
            hold_empty: self.hold_empty,
            hold_policy: self.hold_policy,
            bag_epoch: self.bag_epoch,
            bag_remainder_key: self.bag_remainder_key,
            observation: self.observation,
            provenance: self.provenance,
        }
    }

    pub const fn piece_source_id(self) -> PieceSourceId {
        self.piece_source_id
    }

    pub const fn source_kind(self) -> PieceSourceKind {
        self.source_kind
    }

    pub const fn cursor(self) -> u16 {
        self.cursor
    }

    pub const fn hold_piece(self) -> Option<PieceKind> {
        self.hold_piece
    }

    pub const fn hold_empty(self) -> bool {
        self.hold_empty
    }

    pub const fn hold_policy(self) -> HoldPolicy {
        self.hold_policy
    }

    pub const fn bag_epoch(self) -> u16 {
        self.bag_epoch
    }

    pub const fn bag_remainder_key(self) -> u64 {
        self.bag_remainder_key
    }

    pub const fn observation(self) -> SupplyObservationIdentity {
        self.observation
    }

    pub const fn provenance(self) -> SupplyProvenanceId {
        self.provenance
    }

    pub const fn hold_state(self) -> SupplyHoldState {
        match (self.hold_policy, self.hold_piece) {
            (HoldPolicy::Forbidden, _) => SupplyHoldState::Disabled,
            (_, Some(piece)) => SupplyHoldState::Occupied(piece),
            (_, None) => SupplyHoldState::Empty,
        }
    }

    pub fn apply(
        self,
        transition: HoldTransition,
        current_piece: PieceKind,
        next_piece: Option<PieceKind>,
    ) -> Result<SupplyExecutionStep, SupplyExecutionError> {
        SupplyExecutionAutomaton::sequence().transition(
            self,
            transition.into(),
            current_piece,
            next_piece,
        )
    }

    fn validate(self) -> Result<(), SupplyExecutionError> {
        if self.hold_empty != self.hold_piece.is_none()
            || (self.hold_policy == HoldPolicy::Forbidden && self.hold_piece.is_some())
        {
            return Err(SupplyExecutionError::InvalidHoldState);
        }
        Ok(())
    }

    fn ordering_key(self) -> (u64, u32, u16, u8, u8, u8, u16, u64, u8, u64, u64) {
        (
            self.piece_source_id.get(),
            self.source_kind.as_u32(),
            self.cursor,
            self.hold_piece.map_or(0, piece_tag),
            self.hold_empty as u8,
            hold_policy_tag(self.hold_policy),
            self.bag_epoch,
            self.bag_remainder_key,
            self.observation.policy_tag(),
            self.observation.observation_id,
            self.provenance.0,
        )
    }
}

impl Ord for SupplyExecutionState {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ordering_key().cmp(&other.ordering_key())
    }
}

impl PartialOrd for SupplyExecutionState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SupplyExecutionMemoKey {
    pub piece_source_id: PieceSourceId,
    pub source_kind: PieceSourceKind,
    pub cursor: u16,
    pub hold_piece: Option<PieceKind>,
    pub hold_empty: bool,
    pub hold_policy: HoldPolicy,
    pub bag_epoch: u16,
    pub bag_remainder_key: u64,
    pub observation: SupplyObservationIdentity,
    pub provenance: SupplyProvenanceId,
}

impl SupplyExecutionMemoKey {
    pub fn stable_hash(self) -> u64 {
        let mut hash = fnv_seed();
        mix_u64(&mut hash, self.piece_source_id.get());
        mix_u32(&mut hash, self.source_kind.as_u32());
        mix_u16(&mut hash, self.cursor);
        mix_u8(&mut hash, self.hold_piece.map_or(0, piece_tag));
        mix_u8(&mut hash, self.hold_empty as u8);
        mix_u8(&mut hash, hold_policy_tag(self.hold_policy));
        mix_u16(&mut hash, self.bag_epoch);
        mix_u64(&mut hash, self.bag_remainder_key);
        mix_u8(&mut hash, self.observation.policy_tag());
        mix_u64(&mut hash, self.observation.observation_id);
        mix_u64(&mut hash, self.provenance.0);
        hash
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SupplyTransitionEvidence {
    pub used_piece: PieceKind,
    pub queue_current_piece: PieceKind,
    pub queue_next_piece: Option<PieceKind>,
    pub queue_advances: u8,
    pub cursor_before: u16,
    pub cursor_after: u16,
    pub hold_before: SupplyHoldState,
    pub hold_after: SupplyHoldState,
    pub branch_kind: SupplyBranchKind,
    pub piece_source_id: PieceSourceId,
    pub source_kind: PieceSourceKind,
    pub observation: SupplyObservationIdentity,
    pub provenance: SupplyProvenanceId,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SupplyExecutionStep {
    pub used_piece: PieceKind,
    pub next_state: SupplyExecutionState,
    pub evidence: SupplyTransitionEvidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupplyExecutionError {
    EmptyBagProfile,
    InvalidHoldState,
    InvalidBagRemainder,
    HoldForbidden,
    HoldRequired,
    MissingHeldPiece,
    HoldSlotOccupied,
    MissingNextPiece,
    CursorExhausted,
    BagEpochExhausted,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HoldTransition {
    UseCurrent,
    SwapHeld,
    StoreCurrentThenUseNext,
}

impl From<HoldTransition> for SupplyBranchKind {
    fn from(value: HoldTransition) -> Self {
        match value {
            HoldTransition::UseCurrent => Self::Current,
            HoldTransition::SwapHeld => Self::SwapHeld,
            HoldTransition::StoreCurrentThenUseNext => Self::StoreCurrent,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupplyExecutionAutomaton {
    full_bag: Option<PieceMultisetKey>,
}

impl Default for SupplyExecutionAutomaton {
    fn default() -> Self {
        Self::sequence()
    }
}

impl SupplyExecutionAutomaton {
    pub const fn sequence() -> Self {
        Self { full_bag: None }
    }

    pub fn for_bag(bag_pattern: &[PieceKind]) -> Result<Self, SupplyExecutionError> {
        let full_bag = PieceMultisetKey::from_pieces(bag_pattern.iter().copied());
        if full_bag.total_count() == 0 {
            return Err(SupplyExecutionError::EmptyBagProfile);
        }
        if full_bag.counts().into_iter().any(|count| count > 0x0f) {
            return Err(SupplyExecutionError::InvalidBagRemainder);
        }
        Ok(Self {
            full_bag: Some(full_bag),
        })
    }

    pub fn project_bag_initial_state(
        &self,
        mut state: SupplyExecutionState,
        hold_enabled: bool,
        placed_piece_count: usize,
    ) -> Result<SupplyExecutionState, SupplyExecutionError> {
        let full_bag = self.full_bag.ok_or(SupplyExecutionError::EmptyBagProfile)?;
        if state.hold_empty != state.hold_piece.is_none()
            || (!hold_enabled && state.hold_piece.is_some())
        {
            return Err(SupplyExecutionError::InvalidHoldState);
        }
        state.source_kind = PieceSourceKind::BagUniverse;
        state.hold_policy = if hold_enabled {
            if state.hold_policy == HoldPolicy::Required {
                HoldPolicy::Required
            } else {
                HoldPolicy::Allowed
            }
        } else {
            HoldPolicy::Forbidden
        };
        let remainder =
            decode_initial_remainder(state.bag_remainder_key, full_bag, state.cursor == 0)?;
        state.bag_remainder_key = encode_remainder(remainder);
        self.validate_bag_state(state)?;

        let optional_hold_draw = usize::from(hold_enabled && state.hold_piece.is_none());
        let maximum_draw_count = placed_piece_count
            .checked_add(optional_hold_draw)
            .ok_or(SupplyExecutionError::CursorExhausted)?;
        let maximum_draw_count_u16 =
            u16::try_from(maximum_draw_count).map_err(|_| SupplyExecutionError::CursorExhausted)?;
        state
            .cursor
            .checked_add(maximum_draw_count_u16)
            .ok_or(SupplyExecutionError::CursorExhausted)?;

        let refill_draw_count =
            maximum_draw_count.saturating_sub(usize::from(remainder.total_count()));
        let full_bag_size = usize::from(full_bag.total_count());
        let maximum_refills = if refill_draw_count == 0 {
            0
        } else {
            refill_draw_count
                .checked_add(full_bag_size - 1)
                .ok_or(SupplyExecutionError::BagEpochExhausted)?
                / full_bag_size
        };
        let maximum_refills =
            u16::try_from(maximum_refills).map_err(|_| SupplyExecutionError::BagEpochExhausted)?;
        state
            .bag_epoch
            .checked_add(maximum_refills)
            .ok_or(SupplyExecutionError::BagEpochExhausted)?;
        Ok(state)
    }

    pub fn transition(
        &self,
        state: SupplyExecutionState,
        branch_kind: SupplyBranchKind,
        current_piece: PieceKind,
        next_piece: Option<PieceKind>,
    ) -> Result<SupplyExecutionStep, SupplyExecutionError> {
        state.validate()?;
        match branch_kind {
            SupplyBranchKind::Current => {
                if state.hold_policy == HoldPolicy::Required {
                    return Err(SupplyExecutionError::HoldRequired);
                }
                let mut next_state = state;
                next_state.cursor = checked_advance(state.cursor, 1)?;
                Ok(step(
                    state,
                    next_state,
                    current_piece,
                    current_piece,
                    None,
                    1,
                    branch_kind,
                ))
            }
            SupplyBranchKind::SwapHeld => {
                if state.hold_policy == HoldPolicy::Forbidden {
                    return Err(SupplyExecutionError::HoldForbidden);
                }
                let held = state
                    .hold_piece
                    .ok_or(SupplyExecutionError::MissingHeldPiece)?;
                let mut next_state = state;
                next_state.cursor = checked_advance(state.cursor, 1)?;
                set_hold_piece(&mut next_state, Some(current_piece));
                Ok(step(
                    state,
                    next_state,
                    held,
                    current_piece,
                    None,
                    1,
                    branch_kind,
                ))
            }
            SupplyBranchKind::StoreCurrent => {
                if state.hold_policy == HoldPolicy::Forbidden {
                    return Err(SupplyExecutionError::HoldForbidden);
                }
                if state.hold_piece.is_some() {
                    return Err(SupplyExecutionError::HoldSlotOccupied);
                }
                let next_piece = next_piece.ok_or(SupplyExecutionError::MissingNextPiece)?;
                let mut next_state = state;
                next_state.cursor = checked_advance(state.cursor, 2)?;
                set_hold_piece(&mut next_state, Some(current_piece));
                Ok(step(
                    state,
                    next_state,
                    next_piece,
                    current_piece,
                    Some(next_piece),
                    2,
                    branch_kind,
                ))
            }
        }
    }

    pub fn for_each_matching_bag_step(
        &self,
        state: SupplyExecutionState,
        desired_piece: PieceKind,
        mut visit: impl FnMut(SupplyExecutionStep),
    ) -> Result<(), SupplyExecutionError> {
        self.validate_bag_state(state)?;

        if state.hold_policy != HoldPolicy::Required {
            if let Some(next_state) = self.draw_bag_piece(state, desired_piece)? {
                visit(step(
                    state,
                    next_state,
                    desired_piece,
                    desired_piece,
                    None,
                    1,
                    SupplyBranchKind::Current,
                ));
            }
        }

        match state.hold_state() {
            SupplyHoldState::Disabled => {}
            SupplyHoldState::Occupied(held) if held == desired_piece => {
                for current_piece in PieceKind::STANDARD_TETROMINOES {
                    let Some(mut next_state) = self.draw_bag_piece(state, current_piece)? else {
                        continue;
                    };
                    set_hold_piece(&mut next_state, Some(current_piece));
                    visit(step(
                        state,
                        next_state,
                        desired_piece,
                        current_piece,
                        None,
                        1,
                        SupplyBranchKind::SwapHeld,
                    ));
                }
            }
            SupplyHoldState::Empty => {
                for current_piece in PieceKind::STANDARD_TETROMINOES {
                    let Some(after_current) = self.draw_bag_piece(state, current_piece)? else {
                        continue;
                    };
                    let Some(mut next_state) = self.draw_bag_piece(after_current, desired_piece)?
                    else {
                        continue;
                    };
                    set_hold_piece(&mut next_state, Some(current_piece));
                    visit(step(
                        state,
                        next_state,
                        desired_piece,
                        current_piece,
                        Some(desired_piece),
                        2,
                        SupplyBranchKind::StoreCurrent,
                    ));
                }
            }
            SupplyHoldState::Occupied(_) => {}
        }
        Ok(())
    }

    pub fn write_matching_bag_steps(
        &self,
        state: SupplyExecutionState,
        desired_piece: PieceKind,
        branches: &mut Vec<SupplyExecutionStep>,
    ) -> Result<(), SupplyExecutionError> {
        branches.clear();
        self.for_each_matching_bag_step(state, desired_piece, |step| branches.push(step))
    }

    pub fn advance_bag_cursor(
        &self,
        state: SupplyExecutionState,
        drawn_piece: PieceKind,
    ) -> Result<Option<SupplyExecutionState>, SupplyExecutionError> {
        self.validate_bag_state(state)?;
        self.draw_bag_piece(state, drawn_piece)
    }

    fn validate_bag_state(&self, state: SupplyExecutionState) -> Result<(), SupplyExecutionError> {
        state.validate()?;
        let full_bag = self.full_bag.ok_or(SupplyExecutionError::EmptyBagProfile)?;
        let remainder = decode_remainder(state.bag_remainder_key)?;
        if PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .any(|piece| remainder.count(piece) > full_bag.count(piece))
        {
            return Err(SupplyExecutionError::InvalidBagRemainder);
        }
        Ok(())
    }

    fn draw_bag_piece(
        &self,
        mut state: SupplyExecutionState,
        piece: PieceKind,
    ) -> Result<Option<SupplyExecutionState>, SupplyExecutionError> {
        let full_bag = self.full_bag.ok_or(SupplyExecutionError::EmptyBagProfile)?;
        let mut remainder = decode_remainder(state.bag_remainder_key)?;
        if remainder.total_count() == 0 {
            if state.cursor != 0 {
                state.bag_epoch = state
                    .bag_epoch
                    .checked_add(1)
                    .ok_or(SupplyExecutionError::BagEpochExhausted)?;
            }
            remainder = full_bag;
        }
        if !remove_piece(&mut remainder, piece) {
            return Ok(None);
        }
        state.cursor = checked_advance(state.cursor, 1)?;
        state.bag_remainder_key = encode_remainder(remainder);
        Ok(Some(state))
    }
}

fn step(
    before: SupplyExecutionState,
    next_state: SupplyExecutionState,
    used_piece: PieceKind,
    queue_current_piece: PieceKind,
    queue_next_piece: Option<PieceKind>,
    queue_advances: u8,
    branch_kind: SupplyBranchKind,
) -> SupplyExecutionStep {
    SupplyExecutionStep {
        used_piece,
        next_state,
        evidence: SupplyTransitionEvidence {
            used_piece,
            queue_current_piece,
            queue_next_piece,
            queue_advances,
            cursor_before: before.cursor,
            cursor_after: next_state.cursor,
            hold_before: before.hold_state(),
            hold_after: next_state.hold_state(),
            branch_kind,
            piece_source_id: before.piece_source_id,
            source_kind: before.source_kind,
            observation: before.observation,
            provenance: before.provenance,
        },
    }
}

fn set_hold_piece(state: &mut SupplyExecutionState, hold_piece: Option<PieceKind>) {
    state.hold_piece = hold_piece;
    state.hold_empty = hold_piece.is_none();
}

fn checked_advance(cursor: u16, amount: u16) -> Result<u16, SupplyExecutionError> {
    cursor
        .checked_add(amount)
        .ok_or(SupplyExecutionError::CursorExhausted)
}

fn decode_initial_remainder(
    key: u64,
    full_bag: PieceMultisetKey,
    fresh_source: bool,
) -> Result<PieceMultisetKey, SupplyExecutionError> {
    if key == 0 && fresh_source {
        return Ok(full_bag);
    }
    let remainder = decode_remainder(key)?;
    if PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .any(|piece| remainder.count(piece) > full_bag.count(piece))
    {
        return Err(SupplyExecutionError::InvalidBagRemainder);
    }
    Ok(remainder)
}

fn decode_remainder(key: u64) -> Result<PieceMultisetKey, SupplyExecutionError> {
    let storage_mask = (1usize..=7).fold(0_u64, |mask, piece| mask | (0xf_u64 << (piece * 4)));
    if key & !storage_mask != 0 {
        return Err(SupplyExecutionError::InvalidBagRemainder);
    }
    let mut counts = [0_u8; 7];
    for (index, count) in counts.iter_mut().enumerate() {
        *count = ((key >> ((index + 1) * 4)) & 0xf) as u8;
    }
    Ok(PieceMultisetKey::from_counts(counts))
}

fn encode_remainder(remainder: PieceMultisetKey) -> u64 {
    remainder
        .counts()
        .into_iter()
        .enumerate()
        .fold(0_u64, |key, (index, count)| {
            key | (u64::from(count) << ((index + 1) * 4))
        })
}

fn remove_piece(remainder: &mut PieceMultisetKey, piece: PieceKind) -> bool {
    let mut counts = remainder.counts();
    let index = usize::from(piece_tag(piece) - 1);
    if counts[index] == 0 {
        return false;
    }
    counts[index] -= 1;
    *remainder = PieceMultisetKey::from_counts(counts);
    true
}

const fn hold_policy_tag(policy: HoldPolicy) -> u8 {
    match policy {
        HoldPolicy::Forbidden => 1,
        HoldPolicy::Allowed => 2,
        HoldPolicy::Required => 3,
    }
}

const fn piece_tag(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 1,
        PieceKind::O => 2,
        PieceKind::T => 3,
        PieceKind::S => 4,
        PieceKind::Z => 5,
        PieceKind::J => 6,
        PieceKind::L => 7,
    }
}

const fn fnv_seed() -> u64 {
    14_695_981_039_346_656_037
}

fn mix_u8(hash: &mut u64, value: u8) {
    *hash ^= u64::from(value);
    *hash = hash.wrapping_mul(1_099_511_628_211);
}

fn mix_u16(hash: &mut u64, value: u16) {
    mix_u8(hash, (value & 0x00ff) as u8);
    mix_u8(hash, ((value >> 8) & 0x00ff) as u8);
}

fn mix_u32(hash: &mut u64, value: u32) {
    for shift in (0..32).step_by(8) {
        mix_u8(hash, ((value >> shift) & 0xff) as u8);
    }
}

fn mix_u64(hash: &mut u64, value: u64) {
    for shift in (0..64).step_by(8) {
        mix_u8(hash, ((value >> shift) & 0xff) as u8);
    }
}
