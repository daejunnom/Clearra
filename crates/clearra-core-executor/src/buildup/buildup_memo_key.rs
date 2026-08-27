use clearra_supply::hold_automaton::{HoldAutomatonMemoKey, HoldAutomatonState};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CacheIdentity(pub u64);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct DeletedLineState {
    pub deleted_row_mask: u16,
    pub deleted_count: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BuildUpMemoKey {
    pub cache_identity: CacheIdentity,
    pub remaining_ops_bitset: u16,
    pub current_board_mask: u64,
    pub deleted_line_state: DeletedLineState,
    pub hold_automaton_state: HoldAutomatonMemoKey,
    pub reachability_relevant_state: u64,
    pub cleared_lines: u8,
}

impl BuildUpMemoKey {
    pub fn new(
        cache_identity: CacheIdentity,
        remaining_ops_bitset: u16,
        current_board_mask: u64,
        deleted_line_state: DeletedLineState,
        hold_automaton_state: HoldAutomatonState,
        reachability_relevant_state: u64,
        cleared_lines: u8,
    ) -> Self {
        Self {
            cache_identity,
            remaining_ops_bitset,
            current_board_mask,
            deleted_line_state,
            hold_automaton_state: hold_automaton_state.memo_key(),
            reachability_relevant_state,
            cleared_lines,
        }
    }
}
impl BuildUpMemoKey {
    pub fn stable_hash(self) -> u64 {
        let mut hash = fnv_seed();
        mix_u64(&mut hash, self.cache_identity.0);
        mix_u16(&mut hash, self.remaining_ops_bitset);
        mix_u64(&mut hash, self.current_board_mask);
        mix_u16(&mut hash, self.deleted_line_state.deleted_row_mask);
        mix_u8(&mut hash, self.deleted_line_state.deleted_count);
        mix_u64(&mut hash, self.hold_automaton_state.stable_hash());
        mix_u64(&mut hash, self.reachability_relevant_state);
        mix_u8(&mut hash, self.cleared_lines);
        hash
    }
}

fn fnv_seed() -> u64 {
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

fn mix_u64(hash: &mut u64, value: u64) {
    for shift in (0..64).step_by(8) {
        mix_u8(hash, ((value >> shift) & 0xff) as u8);
    }
}

#[cfg(test)]
#[path = "buildup_memo_key_tests.rs"]
mod tests;
