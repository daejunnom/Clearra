pub(super) const FAMILY_INVALID: u32 = 0;
pub(super) const FAMILY_EMPTY: u32 = 1;

const FAMILY_NODE_CHUNK_CAPACITY: usize = 4096;
const FAMILY_INTERN_MIN_CAPACITY: usize = 4096;
const FAMILY_INTERN_LOAD_NUMERATOR: usize = 7;
const FAMILY_INTERN_LOAD_DENOMINATOR: usize = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum FamilyNodeKind {
    Append = 1,
    Union = 2,
    Product = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FamilyNode {
    pub left: u32,
    pub right: u32,
    pub row_id: u32,
    pub kind: FamilyNodeKind,
    reserved: [u8; 3],
}

const _: () = assert!(core::mem::size_of::<FamilyNode>() == 16);

#[derive(Debug)]
pub(super) struct GeometrySolutionFamily {
    chunks: Vec<Vec<FamilyNode>>,
    node_count: u32,
    intern_slots: Vec<u32>,
    intern_count: usize,
    interning_disabled: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FamilyCheckpoint {
    node_count: u32,
}

impl GeometrySolutionFamily {
    pub fn new() -> Self {
        let mut intern_slots = Vec::new();
        let interning_disabled = intern_slots
            .try_reserve_exact(FAMILY_INTERN_MIN_CAPACITY)
            .is_err();
        if !interning_disabled {
            intern_slots.resize(FAMILY_INTERN_MIN_CAPACITY, FAMILY_INVALID);
        }
        Self {
            chunks: Vec::new(),
            node_count: 0,
            intern_slots,
            intern_count: 0,
            interning_disabled,
        }
    }

    pub fn append(&mut self, row_id: u32, suffix: u32) -> Option<u32> {
        if suffix == FAMILY_INVALID {
            return Some(FAMILY_INVALID);
        }
        self.intern(FamilyNode {
            left: suffix,
            right: FAMILY_INVALID,
            row_id,
            kind: FamilyNodeKind::Append,
            reserved: [0; 3],
        })
    }

    pub fn union(&mut self, mut left: u32, mut right: u32) -> Option<u32> {
        if left == FAMILY_INVALID {
            return Some(right);
        }
        if right == FAMILY_INVALID || left == right {
            return Some(left);
        }
        if right < left {
            core::mem::swap(&mut left, &mut right);
        }
        self.intern(FamilyNode {
            left,
            right,
            row_id: 0,
            kind: FamilyNodeKind::Union,
            reserved: [0; 3],
        })
    }

    pub fn product(&mut self, mut left: u32, mut right: u32) -> Option<u32> {
        if left == FAMILY_INVALID || right == FAMILY_INVALID {
            return Some(FAMILY_INVALID);
        }
        if left == FAMILY_EMPTY {
            return Some(right);
        }
        if right == FAMILY_EMPTY {
            return Some(left);
        }
        if right < left {
            core::mem::swap(&mut left, &mut right);
        }
        self.intern(FamilyNode {
            left,
            right,
            row_id: 0,
            kind: FamilyNodeKind::Product,
            reserved: [0; 3],
        })
    }

    pub fn node(&self, reference: u32) -> Option<FamilyNode> {
        let index = reference.checked_sub(2)? as usize;
        if index >= self.node_count as usize {
            return None;
        }
        let chunk = index / FAMILY_NODE_CHUNK_CAPACITY;
        let offset = index % FAMILY_NODE_CHUNK_CAPACITY;
        self.chunks
            .get(chunk)
            .and_then(|nodes| nodes.get(offset))
            .copied()
    }

    pub fn retained_bytes(&self) -> usize {
        self.chunks.capacity() * core::mem::size_of::<Vec<FamilyNode>>()
            + self
                .chunks
                .iter()
                .map(|chunk| chunk.capacity() * core::mem::size_of::<FamilyNode>())
                .sum::<usize>()
            + self.intern_slots.capacity() * core::mem::size_of::<u32>()
    }

    pub const fn node_count(&self) -> u32 {
        self.node_count
    }

    pub fn seal_for_reading(&mut self) {
        self.intern_slots = Vec::new();
        self.intern_count = 0;
        self.interning_disabled = true;
    }

    pub fn path_count(&self, root: u32) -> Option<u128> {
        self.path_count_table()?.get(root as usize).copied()
    }

    pub fn path_count_table(&self) -> Option<Vec<u128>> {
        let value_count = self.node_count as usize + 2;
        let mut counts = Vec::new();
        counts.try_reserve_exact(value_count).ok()?;
        counts.resize(value_count, 0_u128);
        counts[FAMILY_EMPTY as usize] = 1;
        for reference in 2..self.node_count + 2 {
            let node = self.node(reference)?;
            let left = *counts.get(node.left as usize)?;
            counts[reference as usize] = match node.kind {
                FamilyNodeKind::Append => left,
                FamilyNodeKind::Union => left.checked_add(*counts.get(node.right as usize)?)?,
                FamilyNodeKind::Product => left.checked_mul(*counts.get(node.right as usize)?)?,
            };
        }
        Some(counts)
    }

    pub fn checkpoint(&self) -> FamilyCheckpoint {
        FamilyCheckpoint {
            node_count: self.node_count,
        }
    }

    pub fn rewind(&mut self, checkpoint: FamilyCheckpoint) {
        if checkpoint.node_count >= self.node_count {
            return;
        }
        self.node_count = checkpoint.node_count;
        let retained_chunks = (self.node_count as usize).div_ceil(FAMILY_NODE_CHUNK_CAPACITY);
        self.chunks.truncate(retained_chunks);
        if let Some(last) = self.chunks.last_mut() {
            let retained_in_last =
                self.node_count as usize - (retained_chunks - 1) * FAMILY_NODE_CHUNK_CAPACITY;
            last.truncate(retained_in_last);
        }
        if self.interning_disabled {
            return;
        }
        self.intern_slots.fill(FAMILY_INVALID);
        self.intern_count = 0;
        for reference in 2..self.node_count + 2 {
            let Some(node) = self.node(reference) else {
                self.disable_interning();
                return;
            };
            self.insert_interned(reference, node_hash(node));
        }
    }

    fn intern(&mut self, node: FamilyNode) -> Option<u32> {
        if !self.interning_disabled {
            if let Some(reference) = self.find_interned(node) {
                return Some(reference);
            }
        }

        let reference = self.push_node(node)?;
        if !self.interning_disabled {
            self.ensure_intern_capacity();
            if !self.interning_disabled {
                self.insert_interned(reference, node_hash(node));
            }
        }
        Some(reference)
    }

    fn push_node(&mut self, node: FamilyNode) -> Option<u32> {
        if self.node_count > u32::MAX - 2 {
            return None;
        }
        let needs_chunk = self
            .chunks
            .last()
            .is_none_or(|chunk| chunk.len() == FAMILY_NODE_CHUNK_CAPACITY);
        if needs_chunk {
            let mut chunk = Vec::new();
            chunk.try_reserve_exact(FAMILY_NODE_CHUNK_CAPACITY).ok()?;
            self.chunks.try_reserve(1).ok()?;
            self.chunks.push(chunk);
        }
        let reference = self.node_count + 2;
        self.chunks.last_mut()?.push(node);
        self.node_count += 1;
        Some(reference)
    }

    fn find_interned(&self, node: FamilyNode) -> Option<u32> {
        if self.intern_slots.is_empty() {
            return None;
        }
        let mask = self.intern_slots.len() - 1;
        let mut slot = node_hash(node) as usize & mask;
        for _ in 0..self.intern_slots.len() {
            let reference = self.intern_slots[slot];
            if reference == FAMILY_INVALID {
                return None;
            }
            if self.node(reference) == Some(node) {
                return Some(reference);
            }
            slot = (slot + 1) & mask;
        }
        None
    }

    fn ensure_intern_capacity(&mut self) {
        if (self.intern_count + 1) * FAMILY_INTERN_LOAD_DENOMINATOR
            < self.intern_slots.len() * FAMILY_INTERN_LOAD_NUMERATOR
        {
            return;
        }
        let Some(next_capacity) = self.intern_slots.len().checked_mul(2) else {
            self.disable_interning();
            return;
        };
        let mut replacement = Vec::new();
        if replacement.try_reserve_exact(next_capacity).is_err() {
            self.disable_interning();
            return;
        }
        replacement.resize(next_capacity, FAMILY_INVALID);
        let old = core::mem::replace(&mut self.intern_slots, replacement);
        self.intern_count = 0;
        for reference in old
            .into_iter()
            .filter(|reference| *reference != FAMILY_INVALID)
        {
            let Some(node) = self.node(reference) else {
                self.disable_interning();
                return;
            };
            self.insert_interned(reference, node_hash(node));
        }
    }

    fn insert_interned(&mut self, reference: u32, hash: u64) {
        let mask = self.intern_slots.len() - 1;
        let mut slot = hash as usize & mask;
        loop {
            if self.intern_slots[slot] == FAMILY_INVALID {
                self.intern_slots[slot] = reference;
                self.intern_count += 1;
                return;
            }
            slot = (slot + 1) & mask;
        }
    }

    fn disable_interning(&mut self) {
        self.intern_slots = Vec::new();
        self.intern_count = 0;
        self.interning_disabled = true;
    }
}

fn node_hash(node: FamilyNode) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in [
        node.kind as u64,
        u64::from(node.left),
        u64::from(node.right),
        u64::from(node.row_id),
    ] {
        hash ^= value.wrapping_add(0x9e37_79b9_7f4a_7c15);
        hash = hash.rotate_left(17).wrapping_mul(0x94d0_49bb_1331_11eb);
    }
    hash ^ (hash >> 31)
}
