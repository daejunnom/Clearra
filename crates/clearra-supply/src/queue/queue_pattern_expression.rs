use std::{borrow::Cow, collections::BTreeSet, fmt};

use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueuePatternExpression {
    source: String,
    sequences: QueuePatternSequenceStorage,
    sequence_len: usize,
}

const FACTORIZED_PATTERN_THRESHOLD: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
enum QueuePatternSequenceStorage {
    Explicit(Vec<Vec<PieceKind>>),
    Factorized(FactorizedQueuePatternSpace),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FactorizedQueuePatternSpace {
    atoms: Vec<FactorizedPatternAtom>,
    pattern_count: usize,
    full_sequence_len: usize,
    visible_sequence_len: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FactorizedPatternAtom {
    choices: Vec<PieceKind>,
    draw_count: usize,
    variant_count: usize,
}

impl QueuePatternExpression {
    pub fn parse(source: &str, max_patterns: usize) -> Result<Self, QueuePatternParseError> {
        let normalized = source
            .chars()
            .filter(|character| !character.is_whitespace() && *character != ',')
            .flat_map(char::to_uppercase)
            .collect::<String>();
        if normalized.is_empty() {
            return Err(QueuePatternParseError::Empty);
        }

        let limit = if max_patterns == 0 {
            usize::MAX
        } else {
            max_patterns
        };
        let alternatives = split_alternatives(&normalized)?;
        let (sequences, sequence_len) = if alternatives.len() == 1 {
            let factorized = FactorizedQueuePatternSpace::parse(alternatives[0], limit)?;
            let sequence_len = factorized.visible_sequence_len;
            let sequences = if factorized.pattern_count > FACTORIZED_PATTERN_THRESHOLD {
                QueuePatternSequenceStorage::Factorized(factorized)
            } else {
                QueuePatternSequenceStorage::Explicit(
                    (0..factorized.pattern_count)
                        .map(|index| factorized.sequence(index))
                        .collect(),
                )
            };
            (sequences, sequence_len)
        } else {
            let mut unique = BTreeSet::new();
            for alternative in alternatives {
                let expanded = expand_alternative(alternative, limit)?;
                for sequence in expanded {
                    unique.insert(sequence);
                    if unique.len() > limit {
                        return Err(QueuePatternParseError::PatternLimitExceeded { limit });
                    }
                }
            }
            let sequences = unique.into_iter().collect::<Vec<_>>();
            let sequence_len = sequences
                .first()
                .map(Vec::len)
                .ok_or(QueuePatternParseError::Empty)?;
            (
                QueuePatternSequenceStorage::Explicit(sequences),
                sequence_len,
            )
        };
        if sequence_len == 0 {
            return Err(QueuePatternParseError::EmptySequence);
        }
        if sequences.has_mixed_sequence_lengths(sequence_len) {
            return Err(QueuePatternParseError::MixedSequenceLengths);
        }

        Ok(Self {
            source: normalized,
            sequences,
            sequence_len,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn sequence_at(&self, index: usize) -> Cow<'_, [PieceKind]> {
        self.sequences.sequence_at(index)
    }

    pub fn write_sequence_at(&self, index: usize, output: &mut Vec<PieceKind>) {
        self.sequences.write_sequence_at(index, output);
    }

    pub fn first_sequence(&self) -> Cow<'_, [PieceKind]> {
        self.sequence_at(0)
    }

    pub fn explicit_sequences(&self) -> Option<&[Vec<PieceKind>]> {
        match &self.sequences {
            QueuePatternSequenceStorage::Explicit(sequences) => Some(sequences),
            QueuePatternSequenceStorage::Factorized(_) => None,
        }
    }

    pub const fn is_factorized(&self) -> bool {
        matches!(self.sequences, QueuePatternSequenceStorage::Factorized(_))
    }

    pub const fn sequence_len(&self) -> usize {
        self.sequence_len
    }

    pub fn pattern_count(&self) -> usize {
        self.sequences.len()
    }

    pub fn prefix(&self, sequence_len: usize) -> Self {
        let sequence_len = sequence_len.min(self.sequence_len);
        let sequences = match &self.sequences {
            QueuePatternSequenceStorage::Explicit(sequences) => {
                QueuePatternSequenceStorage::Explicit(
                    sequences
                        .iter()
                        .map(|sequence| sequence[..sequence_len].to_vec())
                        .collect(),
                )
            }
            QueuePatternSequenceStorage::Factorized(space) => {
                QueuePatternSequenceStorage::Factorized(space.prefix(sequence_len))
            }
        };
        Self {
            source: self.source.clone(),
            sequences,
            sequence_len,
        }
    }

    pub fn standard_7_bag_draw_count(source: &str) -> Option<usize> {
        let (prefix, count) = Self::standard_7_bag_with_optional_leading_piece(source)?;
        prefix.is_none().then_some(count)
    }

    pub fn standard_7_bag_with_optional_leading_piece(
        source: &str,
    ) -> Option<(Option<PieceKind>, usize)> {
        let normalized = source
            .chars()
            .filter(|character| !character.is_whitespace() && *character != ',')
            .flat_map(char::to_uppercase)
            .collect::<Vec<_>>();
        if normalized.is_empty() {
            return None;
        }

        let mut cursor = 0usize;
        let leading_piece = if normalized[cursor] == 'P' {
            None
        } else {
            let piece = PieceKind::from_ascii(normalized[cursor]).ok()?;
            cursor += 1;
            Some(piece)
        };
        let mut counts = Vec::new();
        while cursor < normalized.len() {
            if normalized[cursor] != 'P' {
                return None;
            }
            cursor += 1;
            let start = cursor;
            while normalized.get(cursor).is_some_and(char::is_ascii_digit) {
                cursor += 1;
            }
            if start == cursor {
                return None;
            }
            let count = normalized[start..cursor]
                .iter()
                .collect::<String>()
                .parse::<usize>()
                .ok()?;
            if !(1..=7).contains(&count) {
                return None;
            }
            counts.push(count);
        }
        if counts.is_empty() {
            return None;
        }
        if counts[..counts.len().saturating_sub(1)]
            .iter()
            .any(|count| *count != 7)
        {
            return None;
        }
        Some((leading_piece, counts.into_iter().sum()))
    }
}

impl QueuePatternSequenceStorage {
    fn len(&self) -> usize {
        match self {
            Self::Explicit(sequences) => sequences.len(),
            Self::Factorized(space) => space.pattern_count,
        }
    }

    fn sequence_at(&self, index: usize) -> Cow<'_, [PieceKind]> {
        match self {
            Self::Explicit(sequences) => Cow::Borrowed(&sequences[index]),
            Self::Factorized(space) => Cow::Owned(space.sequence(index)),
        }
    }

    fn write_sequence_at(&self, index: usize, output: &mut Vec<PieceKind>) {
        output.clear();
        match self {
            Self::Explicit(sequences) => output.extend_from_slice(&sequences[index]),
            Self::Factorized(space) => space.write_sequence(index, output),
        }
    }

    fn has_mixed_sequence_lengths(&self, expected: usize) -> bool {
        match self {
            Self::Explicit(sequences) => {
                sequences.iter().any(|sequence| sequence.len() != expected)
            }
            Self::Factorized(space) => space.visible_sequence_len != expected,
        }
    }
}

impl FactorizedQueuePatternSpace {
    fn parse(source: &str, limit: usize) -> Result<Self, QueuePatternParseError> {
        let characters = source.chars().collect::<Vec<_>>();
        let mut cursor = 0usize;
        let mut atoms = Vec::new();
        let mut pattern_count = 1usize;
        let mut sequence_len = 0usize;
        while cursor < characters.len() {
            let atom = parse_factorized_atom(&characters, &mut cursor)?;
            pattern_count = pattern_count
                .checked_mul(atom.variant_count)
                .ok_or(QueuePatternParseError::PatternLimitExceeded { limit })?;
            if pattern_count > limit {
                return Err(QueuePatternParseError::PatternLimitExceeded { limit });
            }
            sequence_len = sequence_len
                .checked_add(atom.draw_count)
                .ok_or(QueuePatternParseError::PatternLimitExceeded { limit })?;
            atoms.push(atom);
        }
        if atoms.is_empty() {
            return Err(QueuePatternParseError::EmptySequence);
        }
        Ok(Self {
            atoms,
            pattern_count,
            full_sequence_len: sequence_len,
            visible_sequence_len: sequence_len,
        })
    }

    fn prefix(&self, sequence_len: usize) -> Self {
        Self {
            atoms: self.atoms.clone(),
            pattern_count: self.pattern_count,
            full_sequence_len: self.full_sequence_len,
            visible_sequence_len: sequence_len.min(self.full_sequence_len),
        }
    }

    fn sequence(&self, index: usize) -> Vec<PieceKind> {
        let mut sequence = Vec::with_capacity(self.visible_sequence_len);
        self.write_sequence(index, &mut sequence);
        sequence
    }

    fn write_sequence(&self, index: usize, output: &mut Vec<PieceKind>) {
        assert!(
            index < self.pattern_count,
            "pattern index belongs to expression"
        );
        output.clear();
        output.reserve(self.visible_sequence_len);
        let mut remaining_index = index;
        let mut suffix_count = self.pattern_count;
        for atom in &self.atoms {
            suffix_count /= atom.variant_count;
            let variant = remaining_index / suffix_count;
            remaining_index %= suffix_count;
            atom.append_variant(variant, output);
            if output.len() >= self.visible_sequence_len {
                output.truncate(self.visible_sequence_len);
                break;
            }
        }
    }
}

impl FactorizedPatternAtom {
    fn new(
        mut choices: Vec<PieceKind>,
        draw_count: usize,
        index: usize,
    ) -> Result<Self, QueuePatternParseError> {
        choices.sort_unstable();
        choices.dedup();
        if draw_count == 0 || draw_count > choices.len() {
            return Err(QueuePatternParseError::PermutationCountOutOfRange {
                index,
                count: draw_count,
                available: choices.len(),
            });
        }
        let variant_count = falling_factorial_checked(choices.len(), draw_count)
            .ok_or(QueuePatternParseError::PatternLimitExceeded { limit: usize::MAX })?;
        Ok(Self {
            choices,
            draw_count,
            variant_count,
        })
    }

    fn append_variant(&self, mut rank: usize, output: &mut Vec<PieceKind>) {
        let mut available = [PieceKind::I; PieceKind::STANDARD_TETROMINOES.len()];
        available[..self.choices.len()].copy_from_slice(&self.choices);
        let mut available_len = self.choices.len();
        for position in 0..self.draw_count {
            let remaining = self.draw_count - position - 1;
            let branch_size = falling_factorial_checked(available_len - 1, remaining)
                .expect("validated atom variant count fits usize");
            let selected = rank / branch_size;
            rank %= branch_size;
            output.push(available[selected]);
            available.copy_within(selected + 1..available_len, selected);
            available_len -= 1;
        }
    }
}

fn split_alternatives(source: &str) -> Result<Vec<&str>, QueuePatternParseError> {
    let mut depth = 0usize;
    for (index, character) in source.char_indices() {
        match character {
            '[' => depth = depth.saturating_add(1),
            ']' if depth == 0 => {
                return Err(QueuePatternParseError::UnexpectedCharacter { index, value: ']' })
            }
            ']' => depth -= 1,
            _ => {}
        }
    }
    if depth != 0 {
        return Err(QueuePatternParseError::UnclosedGroup);
    }
    let alternatives = source.split(';').collect::<Vec<_>>();
    if alternatives
        .iter()
        .any(|alternative| alternative.is_empty())
    {
        return Err(QueuePatternParseError::EmptyAlternative);
    }
    Ok(alternatives)
}

fn expand_alternative(
    source: &str,
    limit: usize,
) -> Result<Vec<Vec<PieceKind>>, QueuePatternParseError> {
    let characters = source.chars().collect::<Vec<_>>();
    let mut cursor = 0usize;
    let mut sequences = vec![Vec::new()];
    while cursor < characters.len() {
        let atom = parse_atom(&characters, &mut cursor)?;
        let mut next = Vec::new();
        for prefix in &sequences {
            for suffix in &atom {
                if next.len() >= limit {
                    return Err(QueuePatternParseError::PatternLimitExceeded { limit });
                }
                let mut sequence = Vec::with_capacity(prefix.len() + suffix.len());
                sequence.extend_from_slice(prefix);
                sequence.extend_from_slice(suffix);
                next.push(sequence);
            }
        }
        sequences = next;
    }
    Ok(sequences)
}

fn parse_factorized_atom(
    characters: &[char],
    cursor: &mut usize,
) -> Result<FactorizedPatternAtom, QueuePatternParseError> {
    let start = *cursor;
    let value = characters[start];
    if let Ok(piece) = PieceKind::from_ascii(value) {
        *cursor += 1;
        return FactorizedPatternAtom::new(vec![piece], 1, start);
    }
    if value == 'P' {
        *cursor += 1;
        let count = parse_count(characters, cursor, start)?;
        return FactorizedPatternAtom::new(PieceKind::STANDARD_TETROMINOES.to_vec(), count, start);
    }

    let (choices, accepts_count_suffix) = match value {
        '*' => {
            *cursor += 1;
            (PieceKind::STANDARD_TETROMINOES.to_vec(), false)
        }
        '[' => (parse_group(characters, cursor)?, true),
        _ => {
            return Err(QueuePatternParseError::UnexpectedCharacter {
                index: start,
                value,
            })
        }
    };
    let (draw_count, has_explicit_suffix) = if characters.get(*cursor) == Some(&'!') {
        *cursor += 1;
        (choices.len(), true)
    } else if accepts_count_suffix && characters.get(*cursor).is_some_and(char::is_ascii_digit) {
        (parse_count(characters, cursor, start)?, true)
    } else {
        (1, false)
    };
    if !has_explicit_suffix && characters.get(*cursor) == Some(&'P') {
        return Err(QueuePatternParseError::UnexpectedCharacter {
            index: *cursor,
            value: 'P',
        });
    }
    FactorizedPatternAtom::new(choices, draw_count, start)
}

fn falling_factorial_checked(value: usize, count: usize) -> Option<usize> {
    let mut product = 1usize;
    for offset in 0..count {
        product = product.checked_mul(value - offset)?;
    }
    Some(product)
}

fn parse_atom(
    characters: &[char],
    cursor: &mut usize,
) -> Result<Vec<Vec<PieceKind>>, QueuePatternParseError> {
    let start = *cursor;
    let value = characters[start];
    if let Ok(piece) = PieceKind::from_ascii(value) {
        *cursor += 1;
        return Ok(vec![vec![piece]]);
    }

    if value == 'P' {
        *cursor += 1;
        let count = parse_count(characters, cursor, start)?;
        return permutations(&PieceKind::STANDARD_TETROMINOES, count, start);
    }

    let (choices, accepts_count_suffix) = match value {
        '*' => {
            *cursor += 1;
            (PieceKind::STANDARD_TETROMINOES.to_vec(), false)
        }
        '[' => (parse_group(characters, cursor)?, true),
        _ => {
            return Err(QueuePatternParseError::UnexpectedCharacter {
                index: start,
                value,
            })
        }
    };

    if characters.get(*cursor) == Some(&'!') {
        *cursor += 1;
        return permutations(&choices, choices.len(), start);
    }
    if accepts_count_suffix && characters.get(*cursor).is_some_and(char::is_ascii_digit) {
        let count = parse_count(characters, cursor, start)?;
        return permutations(&choices, count, start);
    }
    if characters.get(*cursor) == Some(&'P') {
        return Err(QueuePatternParseError::UnexpectedCharacter {
            index: *cursor,
            value: 'P',
        });
    }
    Ok(choices.into_iter().map(|piece| vec![piece]).collect())
}

fn parse_group(
    characters: &[char],
    cursor: &mut usize,
) -> Result<Vec<PieceKind>, QueuePatternParseError> {
    let start = *cursor;
    *cursor += 1;
    let complement = characters.get(*cursor) == Some(&'^');
    if complement {
        *cursor += 1;
    }
    let mut choices = Vec::new();
    while let Some(&value) = characters.get(*cursor) {
        if value == ']' {
            *cursor += 1;
            break;
        }
        let piece = PieceKind::from_ascii(value).map_err(|_| {
            QueuePatternParseError::UnexpectedCharacter {
                index: *cursor,
                value,
            }
        })?;
        if !choices.contains(&piece) {
            choices.push(piece);
        }
        *cursor += 1;
    }
    if *cursor > characters.len() || characters.get(cursor.saturating_sub(1)) != Some(&']') {
        return Err(QueuePatternParseError::UnclosedGroup);
    }
    if complement {
        choices = PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .filter(|piece| !choices.contains(piece))
            .collect();
    }
    choices.sort_unstable();
    if choices.is_empty() {
        return Err(QueuePatternParseError::EmptyGroup { index: start });
    }
    Ok(choices)
}

fn parse_count(
    characters: &[char],
    cursor: &mut usize,
    index: usize,
) -> Result<usize, QueuePatternParseError> {
    let start = *cursor;
    while characters.get(*cursor).is_some_and(char::is_ascii_digit) {
        *cursor += 1;
    }
    if start == *cursor {
        return Err(QueuePatternParseError::MissingPermutationCount { index });
    }
    characters[start..*cursor]
        .iter()
        .collect::<String>()
        .parse::<usize>()
        .map_err(|_| QueuePatternParseError::InvalidPermutationCount { index })
}

fn permutations(
    choices: &[PieceKind],
    count: usize,
    index: usize,
) -> Result<Vec<Vec<PieceKind>>, QueuePatternParseError> {
    if count == 0 || count > choices.len() {
        return Err(QueuePatternParseError::PermutationCountOutOfRange {
            index,
            count,
            available: choices.len(),
        });
    }
    let mut output = Vec::new();
    let mut prefix = Vec::with_capacity(count);
    let mut used = vec![false; choices.len()];
    append_permutations(choices, count, &mut prefix, &mut used, &mut output);
    Ok(output)
}

fn append_permutations(
    choices: &[PieceKind],
    count: usize,
    prefix: &mut Vec<PieceKind>,
    used: &mut [bool],
    output: &mut Vec<Vec<PieceKind>>,
) {
    if prefix.len() == count {
        output.push(prefix.clone());
        return;
    }
    for index in 0..choices.len() {
        if used[index] {
            continue;
        }
        used[index] = true;
        prefix.push(choices[index]);
        append_permutations(choices, count, prefix, used, output);
        prefix.pop();
        used[index] = false;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum QueuePatternParseError {
    Empty,
    EmptyAlternative,
    EmptySequence,
    EmptyGroup {
        index: usize,
    },
    UnclosedGroup,
    UnexpectedCharacter {
        index: usize,
        value: char,
    },
    MissingPermutationCount {
        index: usize,
    },
    InvalidPermutationCount {
        index: usize,
    },
    PermutationCountOutOfRange {
        index: usize,
        count: usize,
        available: usize,
    },
    MixedSequenceLengths,
    PatternLimitExceeded {
        limit: usize,
    },
}

impl fmt::Display for QueuePatternParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("queue pattern is empty"),
            Self::EmptyAlternative => {
                formatter.write_str("queue pattern contains an empty alternative")
            }
            Self::EmptySequence => formatter.write_str("queue pattern produces an empty sequence"),
            Self::EmptyGroup { index } => {
                write!(formatter, "queue pattern group at {index} is empty")
            }
            Self::UnclosedGroup => formatter.write_str("queue pattern group is not closed"),
            Self::UnexpectedCharacter { index, value } => {
                write!(
                    formatter,
                    "unexpected queue pattern character '{value}' at {index}"
                )
            }
            Self::MissingPermutationCount { index } => {
                write!(
                    formatter,
                    "queue permutation at {index} is missing its count"
                )
            }
            Self::InvalidPermutationCount { index } => {
                write!(
                    formatter,
                    "queue permutation at {index} has an invalid count"
                )
            }
            Self::PermutationCountOutOfRange {
                count, available, ..
            } => write!(
                formatter,
                "queue permutation count {count} exceeds the {available} available pieces"
            ),
            Self::MixedSequenceLengths => {
                formatter.write_str("queue pattern alternatives must have the same length")
            }
            Self::PatternLimitExceeded { limit } => {
                write!(
                    formatter,
                    "queue pattern expands beyond the limit of {limit}"
                )
            }
        }
    }
}

impl std::error::Error for QueuePatternParseError {}

#[cfg(test)]
mod tests {
    use super::{
        expand_alternative, FactorizedQueuePatternSpace, QueuePatternExpression,
        QueuePatternParseError,
    };

    #[test]
    fn permutation_count_is_not_a_set_suffix() {
        for source in ["[OISZ]p2", "*p4"] {
            assert!(matches!(
                QueuePatternExpression::parse(source, 1_000),
                Err(QueuePatternParseError::UnexpectedCharacter { value: 'P', .. })
            ));
        }
    }

    #[test]
    fn standalone_bag_permutation_and_group_all_orders_remain_available() {
        let bag = QueuePatternExpression::parse("P4", 1_000).expect("standalone P4");
        let group_count = QueuePatternExpression::parse("[OISZ]2", 1_000).expect("group count");
        let group = QueuePatternExpression::parse("[OISZ]!", 1_000).expect("group orders");

        assert_eq!(bag.sequence_len(), 4);
        assert_eq!(bag.pattern_count(), 840);
        assert_eq!(group_count.sequence_len(), 2);
        assert_eq!(group_count.pattern_count(), 12);
        assert_eq!(group.sequence_len(), 4);
        assert_eq!(group.pattern_count(), 24);
    }

    #[test]
    fn completed_group_delimits_a_following_bag_permutation() {
        let expression = QueuePatternExpression::parse("[^TSZ]!P4", 20_160)
            .expect("completed group followed by an independent bag atom");

        assert!(expression.is_factorized());
        assert_eq!(expression.sequence_len(), 8);
        assert_eq!(expression.pattern_count(), 20_160);
        assert_eq!(
            expression,
            QueuePatternExpression::parse("[^tsz]!p4", 20_160)
                .expect("lowercase independent bag atom")
        );
    }

    #[test]
    fn piece_and_bag_letters_are_case_insensitive() {
        for (lowercase, uppercase) in [
            ("iotszjl", "IOTSZJL"),
            ("p4", "P4"),
            ("[oisz]2", "[OISZ]2"),
            ("[^tiz]!", "[^TIZ]!"),
        ] {
            assert_eq!(
                QueuePatternExpression::parse(lowercase, 10_000),
                QueuePatternExpression::parse(uppercase, 10_000)
            );
        }
    }

    #[test]
    fn large_single_alternative_uses_exact_factorized_sequence_space() {
        let expression = QueuePatternExpression::parse("P7[^T]4", 1_814_400)
            .expect("complete P7 followed by four non-T draws");

        assert!(expression.is_factorized());
        assert_eq!(expression.pattern_count(), 1_814_400);
        assert_eq!(expression.sequence_len(), 11);
        assert_eq!(expression.first_sequence().len(), 11);
        assert_eq!(expression.sequence_at(1_814_399).len(), 11);
    }

    #[test]
    fn factorized_rank_order_matches_explicit_cartesian_expansion() {
        let source = "P2[^T]2";
        let factorized =
            FactorizedQueuePatternSpace::parse(source, 2_000).expect("small factorized reference");
        let mut explicit = expand_alternative(source, 2_000).expect("explicit reference");
        explicit.sort_unstable();

        assert_eq!(factorized.pattern_count, explicit.len());
        for (index, expected) in explicit.into_iter().enumerate() {
            assert_eq!(factorized.sequence(index), expected);
        }
    }

    #[test]
    fn factorized_prefix_preserves_probability_multiplicity() {
        let expression = QueuePatternExpression::parse("P7[^T]4", 1_814_400)
            .expect("complete factorized expression");
        let prefix = expression.prefix(10);

        assert!(prefix.is_factorized());
        assert_eq!(prefix.pattern_count(), expression.pattern_count());
        assert_eq!(prefix.sequence_len(), 10);
        assert_eq!(prefix.first_sequence().len(), 10);
    }
}
