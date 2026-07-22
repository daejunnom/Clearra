#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MirrorCanonicalTable;

impl MirrorCanonicalTable {
    pub fn canonical_pair(left: u64, right: u64) -> (u64, bool) {
        if right < left {
            (right, true)
        } else {
            (left, false)
        }
    }
}
