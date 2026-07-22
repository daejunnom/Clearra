use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UniqueCollector;

impl UniqueCollector {
    pub fn collect_by_key<T, K, F>(items: &[T], mut key_for: F) -> Vec<T>
    where
        T: Clone,
        K: Ord,
        F: FnMut(&T) -> K,
    {
        let mut seen = BTreeSet::new();
        let mut unique = Vec::new();

        for item in items {
            if seen.insert(key_for(item)) {
                unique.push(item.clone());
            }
        }

        unique
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_collector_keeps_first_item_for_each_key() {
        let items = [("first", 1), ("duplicate", 1), ("second", 2)];

        let unique = UniqueCollector::collect_by_key(&items, |item| item.1);

        assert_eq!(unique, vec![("first", 1), ("second", 2)]);
    }
}
