use clearra_core_domain::board::standard_pc_board::Board256Mask;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ForwardBoard {
    words: [u64; 4],
}

pub(crate) struct BoardCatalog<Key> {
    values: BoardValueArena<Key>,
    slots: Vec<u32>,
}

const BOARD_VALUE_CHUNK_SHIFT: usize = 16;
const BOARD_VALUE_CHUNK_LEN: usize = 1 << BOARD_VALUE_CHUNK_SHIFT;
const BOARD_VALUE_CHUNK_MASK: usize = BOARD_VALUE_CHUNK_LEN - 1;

struct BoardValueArena<Key> {
    chunks: Vec<Vec<Key>>,
    len: usize,
}

impl<Key: Copy> BoardValueArena<Key> {
    fn push(&mut self, value: Key) {
        let chunk_index = self.len >> BOARD_VALUE_CHUNK_SHIFT;
        if chunk_index == self.chunks.len() {
            self.chunks.push(Vec::with_capacity(BOARD_VALUE_CHUNK_LEN));
        }
        self.chunks[chunk_index].push(value);
        self.len += 1;
    }

    fn get(&self, index: usize) -> Key {
        self.chunks[index >> BOARD_VALUE_CHUNK_SHIFT][index & BOARD_VALUE_CHUNK_MASK]
    }

    const fn len(&self) -> usize {
        self.len
    }

    fn iter(&self) -> impl Iterator<Item = Key> + '_ {
        self.chunks.iter().flat_map(|chunk| chunk.iter().copied())
    }
}

impl<Key> Default for BoardValueArena<Key> {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }
}

pub(crate) trait BoardCatalogKey: Copy + Eq {
    fn catalog_hash(self) -> u64;
}

impl BoardCatalogKey for u64 {
    fn catalog_hash(self) -> u64 {
        mix_board_word(0x9e37_79b9_7f4a_7c15, self)
    }
}

impl BoardCatalogKey for [u64; 2] {
    fn catalog_hash(self) -> u64 {
        mix_board_word(mix_board_word(0x9e37_79b9_7f4a_7c15, self[0]), self[1])
    }
}

impl BoardCatalogKey for [u64; 4] {
    fn catalog_hash(self) -> u64 {
        self.into_iter().fold(0x9e37_79b9_7f4a_7c15, mix_board_word)
    }
}

fn mix_board_word(hash: u64, word: u64) -> u64 {
    (hash ^ word.wrapping_add(0x9e37_79b9_7f4a_7c15))
        .rotate_left(27)
        .wrapping_mul(0x94d0_49bb_1331_11eb)
}

impl<Key: BoardCatalogKey> BoardCatalog<Key> {
    fn new() -> Self {
        Self {
            values: BoardValueArena::default(),
            slots: vec![0; 16],
        }
    }

    fn intern(&mut self, key: Key) -> u32 {
        if (self.values.len() + 1) * 10 > self.slots.len() * 7 {
            self.grow();
        }
        let mut slot = key.catalog_hash() as usize & (self.slots.len() - 1);
        loop {
            let encoded = self.slots[slot];
            if encoded == 0 {
                let id =
                    u32::try_from(self.values.len()).expect("forward board catalog exceeds u32");
                self.values.push(key);
                self.slots[slot] = id + 1;
                return id;
            }
            let id = encoded - 1;
            if self.values.get(id as usize) == key {
                return id;
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
    }

    fn get(&self, id: u32) -> Key {
        self.values.get(id as usize)
    }

    fn grow(&mut self) {
        self.slots.resize(self.slots.len() * 2, 0);
        self.slots.fill(0);
        for (id, key) in self.values.iter().enumerate() {
            let mut slot = key.catalog_hash() as usize & (self.slots.len() - 1);
            while self.slots[slot] != 0 {
                slot = (slot + 1) & (self.slots.len() - 1);
            }
            self.slots[slot] = id as u32 + 1;
        }
    }
}

pub(crate) enum ForwardBoardCatalog {
    Board64(BoardCatalog<u64>),
    Board128(BoardCatalog<[u64; 2]>),
    Board256(BoardCatalog<[u64; 4]>),
}

impl ForwardBoardCatalog {
    pub(crate) fn new(height: u8) -> Self {
        match height {
            0..=6 => Self::Board64(BoardCatalog::new()),
            7..=12 => Self::Board128(BoardCatalog::new()),
            _ => Self::Board256(BoardCatalog::new()),
        }
    }

    pub(crate) fn intern(&mut self, board: ForwardBoard) -> u32 {
        match self {
            Self::Board64(catalog) => {
                assert!(
                    (board.words[1] | board.words[2] | board.words[3]) == 0,
                    "64-bit forward board contains out-of-tier cells"
                );
                catalog.intern(board.words[0])
            }
            Self::Board128(catalog) => {
                assert!(
                    (board.words[2] | board.words[3]) == 0,
                    "128-bit forward board contains out-of-tier cells"
                );
                catalog.intern([board.words[0], board.words[1]])
            }
            Self::Board256(catalog) => catalog.intern(board.words),
        }
    }

    pub(crate) fn get(&self, id: u32) -> ForwardBoard {
        match self {
            Self::Board64(catalog) => ForwardBoard::from_words([catalog.get(id), 0, 0, 0]),
            Self::Board128(catalog) => {
                let words = catalog.get(id);
                ForwardBoard::from_words([words[0], words[1], 0, 0])
            }
            Self::Board256(catalog) => ForwardBoard::from_words(catalog.get(id)),
        }
    }
}

impl ForwardBoard {
    pub const EMPTY: Self = Self { words: [0; 4] };

    pub const fn from_mask(mask: Board256Mask) -> Self {
        Self {
            words: mask.words(),
        }
    }

    pub const fn from_words(words: [u64; 4]) -> Self {
        Self { words }
    }

    pub const fn words(self) -> [u64; 4] {
        self.words
    }

    pub const fn is_empty(self) -> bool {
        self.words[0] == 0 && self.words[1] == 0 && self.words[2] == 0 && self.words[3] == 0
    }

    #[inline]
    pub fn intersects_words<const ACTIVE_WORDS: usize>(self, other: Self) -> bool {
        debug_assert!((1..=4).contains(&ACTIVE_WORDS));
        self.words[..ACTIVE_WORDS]
            .iter()
            .zip(&other.words[..ACTIVE_WORDS])
            .any(|(left, right)| left & right != 0)
    }

    pub const fn union(self, other: Self) -> Self {
        Self::from_words([
            self.words[0] | other.words[0],
            self.words[1] | other.words[1],
            self.words[2] | other.words[2],
            self.words[3] | other.words[3],
        ])
    }

    #[inline]
    pub fn union_for_height(self, other: Self, height: u8) -> Self {
        match height {
            0..=6 => Self::from_words([self.words[0] | other.words[0], 0, 0, 0]),
            7..=12 => Self::from_words([
                self.words[0] | other.words[0],
                self.words[1] | other.words[1],
                0,
                0,
            ]),
            _ => self.union(other),
        }
    }

    pub const fn contains(self, index: u16) -> bool {
        if index >= 256 {
            return false;
        }
        self.words[index as usize / 64] & (1_u64 << (index as usize % 64)) != 0
    }

    pub fn insert(&mut self, index: u16) -> bool {
        if index >= 256 {
            return false;
        }
        self.words[index as usize / 64] |= 1_u64 << (index as usize % 64);
        true
    }

    pub fn row_bits(self, width: u8, row: u8) -> u16 {
        let mut bits = 0_u16;
        for x in 0..width {
            if self.contains(u16::from(row) * u16::from(width) + u16::from(x)) {
                bits |= 1_u16 << x;
            }
        }
        bits
    }
}

pub(crate) fn place_and_clear(
    width: u8,
    height: u8,
    board: ForwardBoard,
) -> (ForwardBoard, u32, u8) {
    if width == 10 {
        return match height {
            0..=6 => place_and_clear_10_u64(height, board),
            7..=12 => place_and_clear_10_words::<2>(height, board),
            _ => place_and_clear_10_words::<4>(height, board),
        };
    }
    place_and_clear_cells(width, height, board)
}

fn place_and_clear_10_u64(height: u8, board: ForwardBoard) -> (ForwardBoard, u32, u8) {
    let source = board.words[0];
    let mut compacted = 0_u64;
    let mut cleared_rows = 0_u32;
    let mut output_row = 0_u32;
    for input_row in 0..u32::from(height) {
        let row = (source >> (input_row * 10)) & 0x3ff;
        if row == 0x3ff {
            cleared_rows |= 1_u32 << input_row;
        } else {
            compacted |= row << (output_row * 10);
            output_row += 1;
        }
    }
    (
        ForwardBoard::from_words([compacted, 0, 0, 0]),
        cleared_rows,
        cleared_rows.count_ones() as u8,
    )
}

fn place_and_clear_10_words<const ACTIVE_WORDS: usize>(
    height: u8,
    board: ForwardBoard,
) -> (ForwardBoard, u32, u8) {
    let mut compacted = [0_u64; 4];
    let mut cleared_rows = 0_u32;
    let mut output_row = 0_u8;
    for input_row in 0..height {
        let row = read_packed_row_10::<ACTIVE_WORDS>(board.words, input_row);
        if row == 0x3ff {
            cleared_rows |= 1_u32 << input_row;
        } else {
            write_packed_row_10::<ACTIVE_WORDS>(&mut compacted, output_row, row);
            output_row += 1;
        }
    }
    (
        ForwardBoard::from_words(compacted),
        cleared_rows,
        cleared_rows.count_ones() as u8,
    )
}

#[inline]
fn read_packed_row_10<const ACTIVE_WORDS: usize>(words: [u64; 4], row: u8) -> u16 {
    let bit = usize::from(row) * 10;
    let word = bit / 64;
    debug_assert!(word < ACTIVE_WORDS);
    let shift = bit % 64;
    let mut packed = words[word] >> shift;
    if shift > 54 && word + 1 < ACTIVE_WORDS {
        packed |= words[word + 1] << (64 - shift);
    }
    (packed & 0x3ff) as u16
}

#[inline]
fn write_packed_row_10<const ACTIVE_WORDS: usize>(words: &mut [u64; 4], row: u8, bits: u16) {
    let bit = usize::from(row) * 10;
    let word = bit / 64;
    debug_assert!(word < ACTIVE_WORDS);
    let shift = bit % 64;
    words[word] |= u64::from(bits) << shift;
    if shift > 54 && word + 1 < ACTIVE_WORDS {
        words[word + 1] |= u64::from(bits) >> (64 - shift);
    }
}

fn place_and_clear_cells(width: u8, height: u8, board: ForwardBoard) -> (ForwardBoard, u32, u8) {
    let full_row = (1_u16 << width) - 1;
    let mut compacted = ForwardBoard::EMPTY;
    let mut cleared_rows = 0_u32;
    let mut output_row = 0_u8;
    for input_row in 0..height {
        let row = board.row_bits(width, input_row);
        if row == full_row {
            cleared_rows |= 1_u32 << input_row;
            continue;
        }
        for x in 0..width {
            if row & (1_u16 << x) != 0 {
                compacted.insert(u16::from(output_row) * u16::from(width) + u16::from(x));
            }
        }
        output_row += 1;
    }
    (compacted, cleared_rows, cleared_rows.count_ones() as u8)
}
