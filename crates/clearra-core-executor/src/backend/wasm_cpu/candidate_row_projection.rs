use super::extended_board::ExtendedBoard;

/// The cell carrier may differ, but row completion and logical-to-physical
/// projection have the same meaning for every supported field and subset.
#[derive(Clone, Copy)]
pub(super) enum CandidateCells<'a> {
    Compact(&'a [u64]),
    Extended(&'a [ExtendedBoard]),
}

impl CandidateCells<'_> {
    fn len(self) -> usize {
        match self {
            Self::Compact(cells) => cells.len(),
            Self::Extended(cells) => cells.len(),
        }
    }

    fn board(self, operation: usize) -> ExtendedBoard {
        match self {
            Self::Compact(cells) => ExtendedBoard::from_words([cells[operation], 0, 0, 0]),
            Self::Extended(cells) => cells[operation],
        }
    }

    fn union_selected(self, initial: ExtendedBoard, mut selected: u64) -> ExtendedBoard {
        // Keep the single-word carrier cheap without specializing the row
        // theorem or allocating converted copies of a candidate's cells.
        match self {
            Self::Compact(cells) => {
                let mut board = initial.words()[0];
                while selected != 0 {
                    let operation = selected.trailing_zeros() as usize;
                    selected &= selected - 1;
                    board |= cells[operation];
                }
                ExtendedBoard::from_words([board, 0, 0, 0])
            }
            Self::Extended(cells) => {
                let mut board = initial;
                while selected != 0 {
                    let operation = selected.trailing_zeros() as usize;
                    selected &= selected - 1;
                    board = board.union(cells[operation]);
                }
                board
            }
        }
    }
}

/// Candidate-owned row conjunctions. Equal contributor masks are compiled
/// once, so a simultaneous multi-row clear is one predicate at query time.
/// Operation masks remain exact through the full 60-operation field envelope.
#[derive(Clone, Debug)]
pub(super) struct CandidateRowProjection {
    contributor_masks: [u64; 24],
    row_masks: [u32; 24],
    completed_rows: u32,
    group_count: u8,
    width: u8,
    height: u8,
}

impl CandidateRowProjection {
    pub fn compile(
        width: u8,
        height: u8,
        logical_target: ExtendedBoard,
        cells: CandidateCells<'_>,
    ) -> Self {
        debug_assert!((1..=64).contains(&width));
        debug_assert!((1..=24).contains(&height));
        debug_assert!(u16::from(width) * u16::from(height) <= 256);
        debug_assert!(cells.len() <= 64);
        let mut contributors = [0_u64; 24];
        for operation in 0..cells.len() {
            for cell in cells.board(operation).cells() {
                let row = usize::from(cell) / usize::from(width);
                debug_assert!(row < usize::from(height));
                contributors[row] |= 1_u64 << operation;
            }
        }
        let mut projection = Self {
            contributor_masks: [0; 24],
            row_masks: [0; 24],
            completed_rows: completed_rows(width, height, logical_target),
            group_count: 0,
            width,
            height,
        };
        let mut remaining_rows = projection.completed_rows;
        while remaining_rows != 0 {
            let row = remaining_rows.trailing_zeros() as u8;
            remaining_rows &= remaining_rows - 1;
            let row_bit = 1_u32 << row;
            let mask = contributors[usize::from(row)];
            let count = usize::from(projection.group_count);
            let group = projection.contributor_masks[..count]
                .iter()
                .position(|&existing| existing == mask)
                .unwrap_or(count);
            if group == count {
                projection.contributor_masks[group] = mask;
                projection.group_count += 1;
            }
            projection.row_masks[group] |= row_bit;
        }
        projection
    }

    pub const fn completed_rows(&self) -> u32 {
        self.completed_rows
    }

    pub fn groups(&self) -> impl Iterator<Item = (u64, u32)> + '_ {
        let count = usize::from(self.group_count);
        self.contributor_masks[..count]
            .iter()
            .copied()
            .zip(self.row_masks[..count].iter().copied())
    }

    pub fn contributors(&self, row: u8) -> u64 {
        self.groups()
            .find_map(|(contributors, rows)| (rows & (1_u32 << row) != 0).then_some(contributors))
            .unwrap_or(0)
    }

    pub fn deleted_rows(&self, placed: u64) -> u32 {
        let mut deleted = 0;
        for group in 0..usize::from(self.group_count) {
            let contributors = self.contributor_masks[group];
            if contributors != 0 && contributors & !placed == 0 {
                deleted |= self.row_masks[group];
            }
        }
        deleted
    }

    pub fn compact(&self, board: ExtendedBoard, deleted_rows: u32) -> ExtendedBoard {
        compact_rows(self.width, self.height, board, deleted_rows)
    }

    pub fn frame(&self, deleted_rows: u32) -> RowProjectionFrame {
        RowProjectionFrame::compile(self.width, self.height, deleted_rows)
    }

    pub fn state(
        &self,
        initial: ExtendedBoard,
        cells: CandidateCells<'_>,
        placed: u64,
    ) -> (ExtendedBoard, u32) {
        let deleted = self.deleted_rows(placed);
        (
            self.compact(cells.union_selected(initial, placed), deleted),
            deleted,
        )
    }
}

/// At most twelve surviving row runs and three word boundaries occur within
/// a 24-row board. Splitting at word boundaries keeps each mask one machine
/// word, including in the common single-word representation.
const MAX_FRAME_PARTS: usize = 16;

#[derive(Clone, Copy, Debug)]
pub(super) struct RowProjectionFrame {
    masks: [u64; MAX_FRAME_PARTS],
    shifts: [u8; MAX_FRAME_PARTS],
    source_words: [u8; MAX_FRAME_PARTS],
    part_count: u8,
    single_word: bool,
}

impl RowProjectionFrame {
    fn compile(width: u8, height: u8, deleted_rows: u32) -> Self {
        let mut frame = Self {
            masks: [0; MAX_FRAME_PARTS],
            shifts: [0; MAX_FRAME_PARTS],
            source_words: [0; MAX_FRAME_PARTS],
            part_count: 0,
            single_word: u16::from(width) * u16::from(height) <= 64,
        };
        let mut row = 0_u8;
        let mut deleted = 0_u8;
        while row < height {
            while row < height && deleted_rows & (1_u32 << row) != 0 {
                deleted += 1;
                row += 1;
            }
            let first_row = row;
            while row < height && deleted_rows & (1_u32 << row) == 0 {
                row += 1;
            }
            if first_row == row {
                break;
            }
            let start = usize::from(first_row) * usize::from(width);
            let end = usize::from(row) * usize::from(width);
            for word in start / 64..=(end - 1) / 64 {
                let low = start.saturating_sub(word * 64);
                let high = (end - word * 64).min(64);
                let mask = low_bits(high) & !low_bits(low);
                let part = usize::from(frame.part_count);
                frame.masks[part] = mask;
                frame.shifts[part] = deleted * width;
                frame.source_words[part] = word as u8;
                frame.part_count += 1;
            }
        }
        frame
    }

    pub fn compact(&self, board: ExtendedBoard) -> ExtendedBoard {
        let source = board.words();
        if self.single_word {
            let mut compacted = 0;
            for part in 0..usize::from(self.part_count) {
                compacted |= (source[0] & self.masks[part]) >> self.shifts[part];
            }
            return ExtendedBoard::from_words([compacted, 0, 0, 0]);
        }
        let mut compacted = [0; 4];
        for part in 0..usize::from(self.part_count) {
            let source_word = usize::from(self.source_words[part]);
            let word_shift = usize::from(self.shifts[part]) / 64;
            let bit_shift = usize::from(self.shifts[part]) % 64;
            let value = source[source_word] & self.masks[part];
            let target_word = source_word - word_shift;
            compacted[target_word] |= value >> bit_shift;
            if bit_shift != 0 && target_word != 0 {
                compacted[target_word - 1] |= value << (64 - bit_shift);
            }
        }
        ExtendedBoard::from_words(compacted)
    }
}

fn low_bits(count: usize) -> u64 {
    if count == 64 {
        u64::MAX
    } else {
        (1_u64 << count) - 1
    }
}

/// Single-board projection uses one row pass. Callers projecting all operations
/// in one deletion frame retain `RowProjectionFrame` and reuse its run masks.
pub(super) fn compact_rows(
    width: u8,
    height: u8,
    board: ExtendedBoard,
    deleted_rows: u32,
) -> ExtendedBoard {
    if deleted_rows == 0 {
        let bit_count = usize::from(width) * usize::from(height);
        let mut words = board.words();
        if bit_count <= 64 {
            return ExtendedBoard::from_words([words[0] & low_bits(bit_count), 0, 0, 0]);
        }
        let complete_words = bit_count / 64;
        if bit_count % 64 != 0 {
            words[complete_words] &= low_bits(bit_count % 64);
        }
        words[bit_count.div_ceil(64)..].fill(0);
        return ExtendedBoard::from_words(words);
    }
    project_rows(width, height, board, Some(deleted_rows)).0
}

pub(super) fn completed_rows(width: u8, height: u8, board: ExtendedBoard) -> u32 {
    let source = board.words();
    let full_row = low_bits(usize::from(width));
    let mut completed = 0;
    for row in 0..height {
        if row_word(&source, width, row) == full_row {
            completed |= 1_u32 << row;
        }
    }
    completed
}

pub(super) fn place_and_clear(
    width: u8,
    height: u8,
    board: ExtendedBoard,
) -> (ExtendedBoard, u32, u8) {
    let (compacted, deleted) = project_rows(width, height, board, None);
    (compacted, deleted, deleted.count_ones() as u8)
}

fn project_rows(
    width: u8,
    height: u8,
    board: ExtendedBoard,
    known_deleted: Option<u32>,
) -> (ExtendedBoard, u32) {
    let source = board.words();
    let full_row = low_bits(usize::from(width));
    let mut output = [0_u64; 4];
    let mut deleted = known_deleted.unwrap_or(0);
    let mut output_row = 0_u8;
    for input_row in 0..height {
        if deleted & (1_u32 << input_row) != 0 {
            continue;
        }
        let row = row_word(&source, width, input_row);
        if known_deleted.is_none() && row == full_row {
            deleted |= 1_u32 << input_row;
            continue;
        }
        let start = usize::from(output_row) * usize::from(width);
        let word = start / 64;
        let shift = start % 64;
        output[word] |= row << shift;
        if shift + usize::from(width) > 64 {
            output[word + 1] |= row >> (64 - shift);
        }
        output_row += 1;
    }
    (ExtendedBoard::from_words(output), deleted)
}

pub(super) fn row_bits(words: &[u64; 4], width: u8, row: u8) -> u16 {
    row_word(words, width, row) as u16
}

fn row_word(words: &[u64; 4], width: u8, row: u8) -> u64 {
    let start = usize::from(row) * usize::from(width);
    let word = start / 64;
    if word >= words.len() {
        return 0;
    }
    let shift = start % 64;
    let mut bits = words[word] >> shift;
    if shift + usize::from(width) > 64 && word + 1 < words.len() {
        bits |= words[word + 1] << (64 - shift);
    }
    bits & low_bits(usize::from(width))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_compact(
        width: u8,
        height: u8,
        board: ExtendedBoard,
        deleted: u32,
    ) -> ExtendedBoard {
        let mut result = ExtendedBoard::EMPTY;
        let mut output_row = 0_u16;
        for row in 0..height {
            if deleted & (1_u32 << row) != 0 {
                continue;
            }
            for x in 0..width {
                if board.contains(u16::from(row) * u16::from(width) + u16::from(x)) {
                    result.insert(output_row * u16::from(width) + u16::from(x));
                }
            }
            output_row += 1;
        }
        result
    }

    #[test]
    fn row_projection_matches_cell_reference_across_word_boundaries() {
        for width in 1..=10 {
            for height in [6, 7, 10, 18, 24] {
                let mut board = ExtendedBoard::EMPTY;
                for cell in 0..u16::from(width) * u16::from(height) {
                    if (cell * 37 + 13) % 11 < 6 {
                        board.insert(cell);
                    }
                }
                let all_rows = (1_u32 << height) - 1;
                for deleted in [0, 1, all_rows, 0x55_5555 & all_rows, 0xaa_aaaa & all_rows] {
                    assert_eq!(
                        compact_rows(width, height, board, deleted),
                        reference_compact(width, height, board, deleted),
                        "width={width}, height={height}, deleted={deleted:x}",
                    );
                    assert_eq!(
                        RowProjectionFrame::compile(width, height, deleted).compact(board),
                        reference_compact(width, height, board, deleted),
                    );
                }
            }
        }
    }

    #[test]
    fn compact_and_extended_carriers_share_all_subset_states() {
        let initial = ExtendedBoard::from_words([0x0033, 0, 0, 0]);
        let compact = [0x00cc, 0x3300, 0xcc00];
        let extended = compact.map(|cells| ExtendedBoard::from_words([cells, 0, 0, 0]));
        let target = ExtendedBoard::from_words([0xffff, 0, 0, 0]);
        let projection =
            CandidateRowProjection::compile(4, 4, target, CandidateCells::Compact(&compact));
        let extended_projection =
            CandidateRowProjection::compile(4, 4, target, CandidateCells::Extended(&extended));
        assert_eq!(
            projection.groups().collect::<Vec<_>>(),
            vec![(1, 3), (6, 12)]
        );
        for subset in 0..8 {
            let actual = projection.state(initial, CandidateCells::Compact(&compact), subset);
            let expected =
                extended_projection.state(initial, CandidateCells::Extended(&extended), subset);
            assert_eq!(actual, expected);
            assert_eq!(
                actual.0,
                reference_compact(
                    4,
                    4,
                    CandidateCells::Compact(&compact).union_selected(initial, subset),
                    actual.1
                )
            );
        }
    }

    #[test]
    fn compiled_frames_cover_every_short_mask_and_full_height_sparse_masks() {
        for width in 1..=10 {
            for height in (1..=8).chain([24]) {
                let mut board = ExtendedBoard::EMPTY;
                for cell in 0..u16::from(width) * u16::from(height) {
                    if (cell * 19 + 7) % 13 < 9 {
                        board.insert(cell);
                    }
                }
                let mut seed = 0x243f_6a88_u32;
                let mask_count = if height == 24 { 32 } else { 1_u32 << height };
                for mask in 0..mask_count {
                    let deleted = if height == 24 {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        seed & 0xff_ffff
                    } else {
                        mask
                    };
                    let expected = reference_compact(width, height, board, deleted);
                    assert_eq!(
                        RowProjectionFrame::compile(width, height, deleted).compact(board),
                        expected,
                        "width={width}, height={height}, deleted={deleted:x}",
                    );
                    assert_eq!(compact_rows(width, height, board, deleted), expected);
                }
            }
        }
    }

    #[test]
    fn contributors_preserve_operation_bits_above_fifteen() {
        let mut cells = [ExtendedBoard::EMPTY; 60];
        for (operation, cell) in cells.iter_mut().enumerate() {
            for offset in 0..4 {
                cell.insert((operation * 4 + offset) as u16);
            }
        }
        let target = ExtendedBoard::from_words([u64::MAX, u64::MAX, u64::MAX, (1 << 48) - 1]);
        let projection =
            CandidateRowProjection::compile(10, 24, target, CandidateCells::Extended(&cells));
        assert_eq!(projection.completed_rows(), (1 << 24) - 1);
        assert_eq!(projection.deleted_rows((1 << 60) - 1), (1 << 24) - 1);
        assert_eq!(projection.deleted_rows((1 << 15) - 1), (1 << 6) - 1);
        assert_eq!(
            projection.deleted_rows(((1 << 60) - 1) & !(1 << 59)),
            (1 << 23) - 1
        );
        assert!(projection.contributors(23) & (1 << 59) != 0);
    }

    #[test]
    fn simultaneous_clear_matches_cell_reference_for_all_field_dimensions() {
        for width in 1..=10 {
            for height in 1..=24 {
                let mut board = ExtendedBoard::EMPTY;
                let mut deleted = 0;
                for row in 0..height {
                    let complete = row % 3 == 0;
                    if complete {
                        deleted |= 1_u32 << row;
                    }
                    for x in 0..width {
                        if complete || x + 1 < width && (x + row) % 2 == 0 {
                            board.insert(u16::from(row) * u16::from(width) + u16::from(x));
                        }
                    }
                }
                let (compacted, actual_deleted, count) = place_and_clear(width, height, board);
                assert_eq!(actual_deleted, deleted);
                assert_eq!(completed_rows(width, height, board), deleted);
                assert_eq!(count, deleted.count_ones() as u8);
                assert_eq!(compacted, reference_compact(width, height, board, deleted));
            }
        }
    }

    #[test]
    fn generic_single_word_fields_keep_wide_rows_and_trim_unused_cells() {
        for width in [16, 17, 21, 32, 64] {
            let height = 64 / width;
            let mut board = ExtendedBoard::EMPTY;
            for cell in 0..u16::from(width) * u16::from(height) {
                if cell < u16::from(width) || cell % 3 == 0 {
                    board.insert(cell);
                }
            }
            let expected_deleted = 1_u32;
            let (actual, deleted, count) = place_and_clear(width, height, board);
            assert_eq!(deleted, expected_deleted);
            assert_eq!(count, 1);
            assert_eq!(
                actual,
                reference_compact(width, height, board, expected_deleted)
            );
            assert_eq!(
                RowProjectionFrame::compile(width, height, deleted).compact(board),
                actual,
            );
            let untrimmed = ExtendedBoard::from_words([u64::MAX; 4]);
            assert_eq!(
                compact_rows(width, height, untrimmed, 0),
                reference_compact(width, height, untrimmed, 0),
            );
        }
    }
}
