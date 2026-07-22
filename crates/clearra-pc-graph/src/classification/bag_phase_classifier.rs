#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BagPhase {
    bag_index: usize,
    offset: usize,
}

impl BagPhase {
    pub fn bag_index(self) -> usize {
        self.bag_index
    }
}
impl BagPhase {
    pub fn offset(self) -> usize {
        self.offset
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BagPhaseClassifier;

impl BagPhaseClassifier {
    pub fn classify(queue_index: usize, bag_size: usize) -> Option<BagPhase> {
        if bag_size == 0 {
            return None;
        }
        Some(BagPhase {
            bag_index: queue_index / bag_size,
            offset: queue_index % bag_size,
        })
    }
}
impl BagPhaseClassifier {
    pub fn classify_standard_7(queue_index: usize) -> BagPhase {
        Self::classify(queue_index, 7).expect("standard bag size is positive")
    }
}
