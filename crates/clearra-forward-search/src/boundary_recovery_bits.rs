//! Persistent role identities. The common <=64-piece case stays one word;
//! longer sources allocate only the words reached by an actual lock.
use std::sync::Arc;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum RoleSet {
    Inline(u64),
    Extended(Arc<[u64]>),
}

impl RoleSet {
    pub(super) const EMPTY: Self = Self::Inline(0);

    fn word(&self, index: usize) -> u64 {
        match self {
            Self::Inline(word) => {
                if index == 0 {
                    *word
                } else {
                    0
                }
            }
            Self::Extended(words) => words.get(index).copied().unwrap_or(0),
        }
    }

    pub(super) fn contains(&self, index: usize) -> bool {
        self.word(index / 64) & (1_u64 << (index % 64)) != 0
    }

    pub(super) fn with(&self, index: usize) -> Self {
        if let Self::Inline(word) = self {
            if index < 64 {
                return Self::Inline(*word | (1_u64 << index));
            }
        }
        let word_count = match self {
            Self::Inline(_) => 1,
            Self::Extended(words) => words.len(),
        }
        .max(index / 64 + 1);
        let mut words = (0..word_count)
            .map(|word| self.word(word))
            .collect::<Vec<_>>();
        words[index / 64] |= 1_u64 << (index % 64);
        Self::Extended(words.into())
    }

    pub(super) fn count(&self) -> usize {
        match self {
            Self::Inline(word) => word.count_ones() as usize,
            Self::Extended(words) => words.iter().map(|word| word.count_ones() as usize).sum(),
        }
    }

    pub(super) fn first_unset(&self, count: usize) -> Option<usize> {
        for index in 0..count.div_ceil(64) {
            let remaining = (count - index * 64).min(64);
            let mask = u64::MAX >> (64 - remaining);
            let missing = !self.word(index) & mask;
            if missing != 0 {
                return Some(index * 64 + missing.trailing_zeros() as usize);
            }
        }
        None
    }

    pub(super) fn contains_all(&self, count: usize) -> bool {
        self.first_unset(count).is_none()
    }

    pub(super) fn partially_contains(&self, start: usize, end: usize) -> bool {
        let mut any = false;
        let mut all = true;
        if start >= end {
            return false;
        }
        for word in start / 64..end.div_ceil(64) {
            let low = start.saturating_sub(word * 64).min(64);
            let high = (end - word * 64).min(64);
            let mask = (u64::MAX >> (64 - high)) & (u64::MAX << low);
            let selected = self.word(word) & mask;
            any |= selected != 0;
            all &= selected == mask;
        }
        any && !all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_roles_cross_every_word_boundary_without_aliasing() {
        let mut roles = RoleSet::EMPTY;
        for index in 0..1025 {
            let previous = roles.clone();
            assert_eq!(roles.first_unset(1025), Some(index));
            roles = roles.with(index);
            assert!(!previous.contains(index));
            assert!(roles.contains(index));
            assert_eq!(roles.count(), index + 1);
            assert!(roles.contains_all(index + 1));
        }
        assert!(roles.contains_all(1025));
        assert!(!roles.contains(1025));
        let partial = RoleSet::EMPTY.with(63).with(64).with(70);
        assert!(partial.partially_contains(63, 71));
        assert!(!partial.partially_contains(63, 65));
        assert!(!partial.partially_contains(71, 78));
    }
}
