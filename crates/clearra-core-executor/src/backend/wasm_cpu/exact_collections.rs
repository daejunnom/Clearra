use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasherDefault, Hasher},
};

/// Fast query-local hashing for compact exact solver keys.
///
/// Hash values select buckets only. `HashSet` still confirms the complete key
/// with `Eq`, so collisions cannot authorize dedupe or remove a search state.
#[derive(Clone)]
pub(super) struct ExactKeyHasher {
    state: u64,
}

impl Default for ExactKeyHasher {
    fn default() -> Self {
        Self {
            state: 0x6a09_e667_f3bc_c909,
        }
    }
}

impl ExactKeyHasher {
    #[inline]
    fn absorb(&mut self, value: u64) {
        let mut mixed = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^= mixed >> 31;
        self.state ^= mixed;
        self.state = self
            .state
            .rotate_left(27)
            .wrapping_mul(5)
            .wrapping_add(0x52dc_e729);
    }
}

impl Hasher for ExactKeyHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            self.absorb(u64::from_le_bytes(
                chunk.try_into().expect("eight-byte chunk"),
            ));
        }
        let remainder = chunks.remainder();
        if !remainder.is_empty() {
            let mut tail = [0_u8; 8];
            tail[..remainder.len()].copy_from_slice(remainder);
            self.absorb(u64::from_le_bytes(tail) ^ ((remainder.len() as u64) << 56));
        }
        self.absorb(bytes.len() as u64);
    }

    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.absorb(u64::from(value));
    }

    #[inline]
    fn write_u16(&mut self, value: u16) {
        self.absorb(u64::from(value));
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.absorb(u64::from(value));
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.absorb(value);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.absorb(value as u64);
    }

    #[inline]
    fn write_i8(&mut self, value: i8) {
        self.absorb(value as u8 as u64);
    }

    #[inline]
    fn write_i16(&mut self, value: i16) {
        self.absorb(value as u16 as u64);
    }

    #[inline]
    fn write_i32(&mut self, value: i32) {
        self.absorb(value as u32 as u64);
    }

    #[inline]
    fn write_i64(&mut self, value: i64) {
        self.absorb(value as u64);
    }

    #[inline]
    fn write_isize(&mut self, value: isize) {
        self.absorb(value as usize as u64);
    }
}

pub(super) type ExactHashSet<T> = HashSet<T, BuildHasherDefault<ExactKeyHasher>>;
pub(super) type ExactHashMap<K, V> = HashMap<K, V, BuildHasherDefault<ExactKeyHasher>>;
