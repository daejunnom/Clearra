//! Minimal non-negative arbitrary-precision arithmetic for CTK3 combinadics.
//!
//! CTK3 fields may contain up to 31 * 31 cells.  Their combination ranks do
//! not fit in any primitive Rust integer, so the wire codec keeps the tiny set
//! of operations it needs here instead of gaining a runtime dependency.

use core::cmp::Ordering;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BigNat {
    // Little-endian base-2^32 limbs.  Zero is represented by an empty vector.
    limbs: Vec<u32>,
}

impl BigNat {
    pub(crate) const fn zero() -> Self {
        Self { limbs: Vec::new() }
    }

    pub(crate) fn one() -> Self {
        Self { limbs: vec![1] }
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    pub(crate) fn bit_len(&self) -> usize {
        self.limbs.last().map_or(0, |top| {
            (self.limbs.len() - 1) * 32 + (32 - top.leading_zeros() as usize)
        })
    }

    pub(crate) fn bit(&self, index: usize) -> bool {
        self.limbs
            .get(index / 32)
            .is_some_and(|limb| limb & (1u32 << (index % 32)) != 0)
    }

    pub(crate) fn set_bit(&mut self, index: usize) {
        let limb = index / 32;
        if self.limbs.len() <= limb {
            self.limbs.resize(limb + 1, 0);
        }
        self.limbs[limb] |= 1u32 << (index % 32);
    }

    pub(crate) fn add_assign(&mut self, other: &Self) {
        if self.limbs.len() < other.limbs.len() {
            self.limbs.resize(other.limbs.len(), 0);
        }
        let mut carry = 0u64;
        for index in 0..self.limbs.len() {
            let rhs = u64::from(other.limbs.get(index).copied().unwrap_or(0));
            let value = u64::from(self.limbs[index]) + rhs + carry;
            self.limbs[index] = value as u32;
            carry = value >> 32;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
        }
    }

    pub(crate) fn checked_sub_assign(&mut self, other: &Self) -> bool {
        if BigNat::cmp(&*self, other) == Ordering::Less {
            return false;
        }
        let mut borrow = 0u64;
        for index in 0..self.limbs.len() {
            let lhs = u64::from(self.limbs[index]);
            let rhs = u64::from(other.limbs.get(index).copied().unwrap_or(0)) + borrow;
            if lhs >= rhs {
                self.limbs[index] = (lhs - rhs) as u32;
                borrow = 0;
            } else {
                self.limbs[index] = ((1u64 << 32) + lhs - rhs) as u32;
                borrow = 1;
            }
        }
        debug_assert_eq!(borrow, 0);
        self.normalize();
        true
    }

    pub(crate) fn mul_u32_assign(&mut self, factor: u32) {
        if factor == 0 || self.is_zero() {
            self.limbs.clear();
            return;
        }
        let mut carry = 0u64;
        for limb in &mut self.limbs {
            let value = u64::from(*limb) * u64::from(factor) + carry;
            *limb = value as u32;
            carry = value >> 32;
        }
        if carry != 0 {
            self.limbs.push(carry as u32);
        }
    }

    /// Divides by a non-zero primitive and returns the remainder.
    pub(crate) fn div_u32_assign(&mut self, divisor: u32) -> u32 {
        debug_assert_ne!(divisor, 0);
        let mut remainder = 0u64;
        for limb in self.limbs.iter_mut().rev() {
            let value = (remainder << 32) | u64::from(*limb);
            *limb = (value / u64::from(divisor)) as u32;
            remainder = value % u64::from(divisor);
        }
        self.normalize();
        remainder as u32
    }

    pub(crate) fn subtract_one(&self) -> Self {
        let mut value = self.clone();
        let removed = value.checked_sub_assign(&Self::one());
        debug_assert!(removed);
        value
    }

    fn normalize(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }
}

impl Ord for BigNat {
    fn cmp(&self, other: &Self) -> Ordering {
        self.limbs
            .len()
            .cmp(&other.limbs.len())
            .then_with(|| self.limbs.iter().rev().cmp(other.limbs.iter().rev()))
    }
}

impl PartialOrd for BigNat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(crate) fn combination_count(total: usize, selected: usize) -> BigNat {
    if selected > total {
        return BigNat::zero();
    }
    let count = selected.min(total - selected);
    let mut value = BigNat::one();
    for index in 1..=count {
        value.mul_u32_assign((total - count + index) as u32);
        let remainder = value.div_u32_assign(index as u32);
        debug_assert_eq!(remainder, 0);
    }
    value
}

pub(crate) fn combination_rank(positions: &[usize]) -> BigNat {
    let mut rank = BigNat::zero();
    for (index, position) in positions.iter().copied().enumerate() {
        rank.add_assign(&combination_count(position, index + 1));
    }
    rank
}

pub(crate) fn combination_unrank(
    total: usize,
    selected: usize,
    source_rank: &BigNat,
) -> Option<Vec<usize>> {
    if selected > total {
        return None;
    }
    let mut positions = vec![0; selected];
    let mut rank = source_rank.clone();
    if selected == 0 {
        return rank.is_zero().then_some(positions);
    }
    let mut upper = total.checked_sub(1)?;
    for index in (1..=selected).rev() {
        let mut low = index - 1;
        let mut high = upper;
        let mut position = low;
        while low <= high {
            let candidate = low + (high - low) / 2;
            if combination_count(candidate, index) <= rank {
                position = candidate;
                low = candidate + 1;
            } else if candidate == 0 {
                break;
            } else {
                high = candidate - 1;
            }
        }
        positions[index - 1] = position;
        if !rank.checked_sub_assign(&combination_count(position, index)) {
            return None;
        }
        if index > 1 {
            upper = position.checked_sub(1)?;
        }
    }
    rank.is_zero().then_some(positions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinadics_round_trip_beyond_primitive_width() {
        let positions = (0..480).map(|index| index * 2).collect::<Vec<_>>();
        let rank = combination_rank(&positions);
        assert!(rank.bit_len() > 128);
        assert_eq!(
            combination_unrank(961, positions.len(), &rank),
            Some(positions)
        );
    }
}
