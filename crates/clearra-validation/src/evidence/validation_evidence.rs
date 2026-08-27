#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationEvidence {
    key: String,
    value: String,
}

impl ValidationEvidence {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}
impl ValidationEvidence {
    pub fn key(&self) -> &str {
        &self.key
    }
}
impl ValidationEvidence {
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Returns only the heap payload retained by the key and value strings,
    /// measured by `String` allocation capacity. The inline evidence owner is
    /// excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_add_bytes(self.key.capacity() as u128, self.value.capacity() as u128)
    }
}

fn checked_add_bytes(left: u128, right: u128) -> Option<u128> {
    left.checked_add(right)
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::{checked_add_bytes, ValidationEvidence};

    #[test]
    fn evidence_retained_capacity_counts_both_string_capacities() {
        let mut key = String::with_capacity(64);
        key.push_str("actual");
        let key_capacity = key.capacity() as u128;
        let mut value = String::with_capacity(96);
        value.push_str("expected");
        let value_capacity = value.capacity() as u128;
        let evidence = ValidationEvidence::new(key, value);

        assert_eq!(
            evidence.checked_retained_capacity_bytes(),
            key_capacity.checked_add(value_capacity)
        );
    }

    #[test]
    fn evidence_capacity_addition_fails_closed_on_overflow() {
        assert_eq!(checked_add_bytes(u128::MAX, 1), None);
    }
}
