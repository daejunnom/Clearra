#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllCollector;

impl AllCollector {
    pub fn collect<T: Clone>(items: &[T]) -> Vec<T> {
        items.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_collector_preserves_every_item() {
        assert_eq!(AllCollector::collect(&[1, 1, 2]), vec![1, 1, 2]);
    }
}
