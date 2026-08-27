#[cfg(test)]
use core::convert::Infallible;
use core::fmt;

use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        NormalizedTilingSolutionKey, StandardBoard64ColoredTilingIdentity,
    },
};
use clearra_ctk3::{
    encode_ctk3_compact, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece, CTK3_BUNDLE_PREFIX,
    CTK3_MAX_BUNDLE_PAGES, CTK3_MAX_SEGMENT_PAGES, CTK3_PREFIX,
};
use clearra_fumen::{
    ColoredSolutionFumenExporter, ColoredSolutionPage, ColoredSolutionPlacement, FUMEN_MAX_PAGES,
};

use super::{
    solution_comment_layout::SolutionCommentLayout,
    solution_set_artifact::{SolutionArtifactEntry, SolutionSetArtifact},
};

const STANDARD_WIDTH: usize = 10;

#[cfg(test)]
pub(super) fn encode_ctk3_solution_set(
    artifact: &SolutionSetArtifact,
) -> Result<String, SolutionDocumentError> {
    let mut bytes = Vec::new();
    match encode_ctk3_solution_set_into(
        artifact,
        |chunk| {
            bytes.extend_from_slice(chunk);
            Ok::<_, Infallible>(())
        },
        |_| Ok::<_, Infallible>(()),
    ) {
        Ok(()) => String::from_utf8(bytes).map_err(|_| SolutionDocumentError::Ctk3EncodingFailed),
        Err(SolutionDocumentStreamError::Document(error)) => Err(error),
        Err(SolutionDocumentStreamError::Sink(error)) => match error {},
    }
}

/// Writes one logical CTK3 solution document while retaining at most one
/// 4,096-page segment. `emit` is deliberately caller-owned so an atomic file
/// sink can abort every byte if a later segment, cancellation checkpoint, or
/// codec validation fails.
pub(super) fn encode_ctk3_solution_set_into<E>(
    artifact: &SolutionSetArtifact,
    mut emit: impl FnMut(&[u8]) -> Result<(), E>,
    mut checkpoint: impl FnMut(usize) -> Result<(), E>,
) -> Result<(), SolutionDocumentStreamError<E>> {
    if artifact.entries().is_empty() {
        return Err(SolutionDocumentError::EmptySolutionSet.into());
    }
    if artifact.entries().len() > CTK3_MAX_BUNDLE_PAGES {
        return Err(SolutionDocumentError::Ctk3PageLimitExceeded.into());
    }

    let segment_count = artifact.entries().len().div_ceil(CTK3_MAX_SEGMENT_PAGES);
    if segment_count > 1 {
        emit(CTK3_BUNDLE_PREFIX.as_bytes()).map_err(SolutionDocumentStreamError::Sink)?;
    }

    for (segment_index, entries) in artifact
        .entries()
        .chunks(CTK3_MAX_SEGMENT_PAGES)
        .enumerate()
    {
        let mut pages = Vec::new();
        pages
            .try_reserve_exact(entries.len())
            .map_err(|_| SolutionDocumentError::CapacityExceeded)?;
        let completed_before = segment_index
            .checked_mul(CTK3_MAX_SEGMENT_PAGES)
            .ok_or(SolutionDocumentError::CapacityExceeded)?;
        for (page_offset, entry) in entries.iter().enumerate() {
            let completed = completed_before
                .checked_add(page_offset)
                .ok_or(SolutionDocumentError::CapacityExceeded)?;
            checkpoint(completed).map_err(SolutionDocumentStreamError::Sink)?;
            pages.push(ctk3_page(entry)?);
        }

        let segment = encode_ctk3_compact(&Ctk3Document::new(STANDARD_WIDTH, pages))
            .map_err(|_| SolutionDocumentError::Ctk3EncodingFailed)?;
        if segment_count == 1 {
            emit(segment.as_bytes()).map_err(SolutionDocumentStreamError::Sink)?;
        } else {
            if segment_index != 0 {
                emit(b".").map_err(SolutionDocumentStreamError::Sink)?;
            }
            let payload = segment
                .strip_prefix(CTK3_PREFIX)
                .ok_or(SolutionDocumentError::Ctk3EncodingFailed)?;
            emit(payload.as_bytes()).map_err(SolutionDocumentStreamError::Sink)?;
        }
    }
    checkpoint(artifact.entries().len()).map_err(SolutionDocumentStreamError::Sink)
}

#[cfg(test)]
pub(super) fn encode_fumen_solution_set(
    artifact: &SolutionSetArtifact,
) -> Result<String, SolutionDocumentError> {
    match encode_fumen_solution_set_checked(artifact, |_| Ok::<_, Infallible>(())) {
        Ok(document) => Ok(document),
        Err(SolutionDocumentStreamError::Document(error)) => Err(error),
        Err(SolutionDocumentStreamError::Sink(error)) => match error {},
    }
}

pub(super) fn encode_fumen_solution_set_checked<E>(
    artifact: &SolutionSetArtifact,
    mut checkpoint: impl FnMut(usize) -> Result<(), E>,
) -> Result<String, SolutionDocumentStreamError<E>> {
    if artifact.entries().is_empty() {
        return Err(SolutionDocumentError::EmptySolutionSet.into());
    }
    if artifact.entries().len() > FUMEN_MAX_PAGES {
        return Err(SolutionDocumentError::FumenPageLimitExceeded.into());
    }
    let mut pages = Vec::new();
    pages
        .try_reserve_exact(artifact.entries().len())
        .map_err(|_| SolutionDocumentError::CapacityExceeded)?;
    for (index, entry) in artifact.entries().iter().enumerate() {
        checkpoint(index).map_err(SolutionDocumentStreamError::Sink)?;
        pages.push(fumen_page(entry)?);
    }
    let document = ColoredSolutionFumenExporter::encode(&pages)
        .map_err(|_| SolutionDocumentError::FumenEncodingFailed)?;
    checkpoint(artifact.entries().len()).map_err(SolutionDocumentStreamError::Sink)?;
    Ok(document)
}

fn ctk3_page(entry: &SolutionArtifactEntry) -> Result<Ctk3Page, SolutionDocumentError> {
    let identity = canonical_colored_identity(entry)?;
    let piece_masks = identity.piece_masks();
    let height = semantic_height(identity.initial_board_mask(), &piece_masks);
    let mut cells = vec![Ctk3Color::Empty; height * STANDARD_WIDTH];
    paint_ctk3_mask(&mut cells, identity.initial_board_mask(), Ctk3Color::Gray);
    for (piece, cells_mask) in PieceKind::STANDARD_TETROMINOES
        .iter()
        .copied()
        .zip(piece_masks)
        .filter(|(_, cells_mask)| *cells_mask != 0)
    {
        paint_ctk3_mask(&mut cells, cells_mask, Ctk3Color::Piece(ctk3_piece(piece)));
    }
    let mut page = Ctk3Page::new(height, cells);
    if let Some(comment) = SolutionCommentLayout::render(entry.annotation()) {
        page = page.with_comment(comment);
    }
    Ok(page)
}

fn fumen_page(entry: &SolutionArtifactEntry) -> Result<ColoredSolutionPage, SolutionDocumentError> {
    let identity = canonical_colored_identity(entry)?;
    let piece_masks = identity.piece_masks();
    let height = semantic_height(identity.initial_board_mask(), &piece_masks).max(1);
    let placements = PieceKind::STANDARD_TETROMINOES
        .iter()
        .copied()
        .zip(piece_masks)
        .filter(|(_, cells_mask)| *cells_mask != 0)
        .map(|(piece, cells_mask)| ColoredSolutionPlacement::new(piece, cells_mask))
        .collect();
    let mut page = ColoredSolutionPage::new(
        STANDARD_WIDTH as u8,
        height as u8,
        identity.initial_board_mask(),
        placements,
    )
    .map_err(|_| SolutionDocumentError::FumenEncodingFailed)?;
    if let Some(comment) = SolutionCommentLayout::render(entry.annotation()) {
        page = page.with_comment(comment);
    }
    Ok(page)
}

fn canonical_colored_identity(
    entry: &SolutionArtifactEntry,
) -> Result<StandardBoard64ColoredTilingIdentity, SolutionDocumentError> {
    if let Ok(identity) = NormalizedTilingSolutionKey::parse_canonical(entry.key())
        .and_then(|key| key.standard_board64_identity())
    {
        return Ok(StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(identity));
    }
    parse_colored_field_key(entry.key())
}

/// Parses the canonical static colored-field key used by Build producer and
/// supplied-solution reports. It deliberately retains only the observable
/// per-piece color unions: a `cfk1` key has no placement ordering or boundary
/// information, so document publication must not invent either.
fn parse_colored_field_key(
    key: &str,
) -> Result<StandardBoard64ColoredTilingIdentity, SolutionDocumentError> {
    let body = key
        .strip_prefix("cfk1|initial=")
        .ok_or(SolutionDocumentError::InvalidCanonicalKey)?;
    let (initial, colors) = body
        .split_once("|colors=")
        .ok_or(SolutionDocumentError::InvalidCanonicalKey)?;
    let initial_board_mask = parse_canonical_hex_mask(initial)?;
    if colors.is_empty() {
        return Err(SolutionDocumentError::InvalidCanonicalKey);
    }

    let mut piece_masks = [0_u64; 7];
    let mut previous_index = None;
    for component in colors.split(',') {
        let (piece, mask) = component
            .split_once(':')
            .ok_or(SolutionDocumentError::InvalidCanonicalKey)?;
        let index =
            canonical_piece_index(piece).ok_or(SolutionDocumentError::InvalidCanonicalKey)?;
        if previous_index.is_some_and(|previous| previous >= index) {
            return Err(SolutionDocumentError::InvalidCanonicalKey);
        }
        let mask = parse_canonical_hex_mask(mask)?;
        if mask == 0 {
            return Err(SolutionDocumentError::InvalidCanonicalKey);
        }
        piece_masks[index] = mask;
        previous_index = Some(index);
    }

    StandardBoard64ColoredTilingIdentity::from_piece_masks(initial_board_mask, piece_masks)
        .map_err(|_| SolutionDocumentError::InvalidCanonicalKey)
}

fn parse_canonical_hex_mask(value: &str) -> Result<u64, SolutionDocumentError> {
    if value.len() != 16
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(SolutionDocumentError::InvalidCanonicalKey);
    }
    u64::from_str_radix(value, 16).map_err(|_| SolutionDocumentError::InvalidCanonicalKey)
}

const fn canonical_piece_index(value: &str) -> Option<usize> {
    match value.as_bytes() {
        b"I" => Some(0),
        b"O" => Some(1),
        b"T" => Some(2),
        b"S" => Some(3),
        b"Z" => Some(4),
        b"J" => Some(5),
        b"L" => Some(6),
        _ => None,
    }
}

fn semantic_height(initial: u64, placements: &[u64]) -> usize {
    let occupied = placements
        .iter()
        .copied()
        .fold(initial, |mask, placement| mask | placement);
    let cells = u64::BITS as usize - occupied.leading_zeros() as usize;
    cells.div_ceil(STANDARD_WIDTH)
}

fn paint_ctk3_mask(cells: &mut [Ctk3Color], mut mask: u64, color: Ctk3Color) {
    while mask != 0 {
        let bit = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        if let Some(cell) = cells.get_mut(bit) {
            *cell = color;
        }
    }
}

const fn ctk3_piece(piece: PieceKind) -> Ctk3Piece {
    match piece {
        PieceKind::I => Ctk3Piece::I,
        PieceKind::O => Ctk3Piece::O,
        PieceKind::T => Ctk3Piece::T,
        PieceKind::S => Ctk3Piece::S,
        PieceKind::Z => Ctk3Piece::Z,
        PieceKind::J => Ctk3Piece::J,
        PieceKind::L => Ctk3Piece::L,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SolutionDocumentError {
    EmptySolutionSet,
    InvalidCanonicalKey,
    CapacityExceeded,
    Ctk3EncodingFailed,
    Ctk3PageLimitExceeded,
    FumenEncodingFailed,
    FumenPageLimitExceeded,
}

impl fmt::Display for SolutionDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptySolutionSet => "empty solution sets have no document pages",
            Self::InvalidCanonicalKey => "solution key is not a canonical colored tiling",
            Self::CapacityExceeded => "solution document capacity is exceeded",
            Self::Ctk3EncodingFailed => "native CTK3 encoding failed",
            Self::Ctk3PageLimitExceeded => "native CTK3 logical page limit exceeded",
            Self::FumenEncodingFailed => "native Fumen encoding failed",
            Self::FumenPageLimitExceeded => "native Fumen page limit exceeded",
        })
    }
}

#[derive(Debug)]
pub(super) enum SolutionDocumentStreamError<E> {
    Document(SolutionDocumentError),
    Sink(E),
}

impl<E> From<SolutionDocumentError> for SolutionDocumentStreamError<E> {
    fn from(error: SolutionDocumentError) -> Self {
        Self::Document(error)
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::solution::normalized_tiling_solution::{
        NormalizedTilingSolutionSetHasher, NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
        NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
    };
    use clearra_fumen::SourceFumenDiagramSet;

    use super::*;
    use crate::artifact::{SolutionArtifactAnnotation, SolutionArtifactEntry};

    const SOURCE: &str = "normalized-tiling-set";

    fn artifact() -> SolutionSetArtifact {
        let key = NormalizedTilingSolutionKey::parse_canonical(
            "ctk1|initial=0000000000000300|placements=I:000000000000000f",
        )
        .expect("canonical key");
        let mut hasher = NormalizedTilingSolutionSetHasher::default();
        hasher.update_canonical_key(&key);
        SolutionSetArtifact::try_new(
            SOURCE,
            NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
            NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
            hasher.finish(),
            1,
            vec![SolutionArtifactEntry::try_new(
                key.as_str(),
                SolutionArtifactAnnotation::new()
                    .with_pc_probability("0.5")
                    .expect("annotation"),
            )
            .expect("entry")],
        )
        .expect("artifact")
    }

    fn colored_field_artifact() -> SolutionSetArtifact {
        SolutionSetArtifact::try_new(
            "colored-field-set",
            "clearra-colored-field-key-v1",
            "portfolio-page-identity-sha256.v1",
            "colored-page-1",
            1,
            vec![SolutionArtifactEntry::try_new(
                "cfk1|initial=0000000000000300|colors=I:00000000000000ff",
                SolutionArtifactAnnotation::new(),
            )
            .expect("colored entry")],
        )
        .expect("colored artifact")
    }

    #[test]
    fn native_ctk3_is_a_real_checksummed_document() {
        let encoded = encode_ctk3_solution_set(&artifact()).expect("ctk3");
        assert!(encoded.starts_with("ctk3_"));
    }

    #[test]
    fn native_fumen_roundtrips_the_exact_colored_field() {
        let encoded = encode_fumen_solution_set(&artifact()).expect("fumen");
        let decoded = SourceFumenDiagramSet::decode(&encoded).expect("decoded fumen");
        assert_eq!(decoded.page_count(), 1);
    }

    #[test]
    fn build_colored_field_key_encodes_as_native_ctk3_without_placement_invention() {
        let encoded = encode_ctk3_solution_set(&colored_field_artifact()).expect("ctk3");
        let decoded = clearra_ctk3::decode_ctk3_exact(&encoded).expect("decoded ctk3");
        assert_eq!(decoded.pages.len(), 1);
        assert_eq!(decoded.pages[0].height, 1);
        assert_eq!(
            decoded.pages[0].cells,
            vec![
                Ctk3Color::Piece(Ctk3Piece::I),
                Ctk3Color::Piece(Ctk3Piece::I),
                Ctk3Color::Piece(Ctk3Piece::I),
                Ctk3Color::Piece(Ctk3Piece::I),
                Ctk3Color::Piece(Ctk3Piece::I),
                Ctk3Color::Piece(Ctk3Piece::I),
                Ctk3Color::Piece(Ctk3Piece::I),
                Ctk3Color::Piece(Ctk3Piece::I),
                Ctk3Color::Gray,
                Ctk3Color::Gray,
            ]
        );
    }

    #[test]
    fn build_colored_field_key_roundtrips_through_native_fumen() {
        let artifact = colored_field_artifact();
        let encoded = encode_fumen_solution_set(&artifact).expect("fumen");
        let decoded = clearra_fumen::SourceFumenColoredFieldSet::decode(&encoded)
            .expect("decoded colored fumen");
        assert!(decoded.keys().contains(artifact.entries()[0].key()));
    }

    #[test]
    fn noncanonical_colored_field_keys_fail_closed() {
        for key in [
            "cfk1|initial=0000000000000000|colors=I:0000000000000000",
            "cfk1|initial=0000000000000000|colors=T:000000000000000f,I:00000000000000f0",
            "cfk1|initial=0000000000000000|colors=i:000000000000000f",
            "cfk1|initial=0000000000000000|colors=I:000000000000000F",
            "cfk1|initial=0000000000000001|colors=I:000000000000000f",
        ] {
            let artifact = SolutionSetArtifact::try_new(
                "colored-field-set",
                "clearra-colored-field-key-v1",
                "portfolio-page-identity-sha256.v1",
                "invalid-colored-page",
                1,
                vec![
                    SolutionArtifactEntry::try_new(key, SolutionArtifactAnnotation::new())
                        .expect("envelope accepts opaque key"),
                ],
            )
            .expect("opaque artifact");
            assert_eq!(
                encode_ctk3_solution_set(&artifact),
                Err(SolutionDocumentError::InvalidCanonicalKey),
                "{key}"
            );
            assert_eq!(
                encode_fumen_solution_set(&artifact),
                Err(SolutionDocumentError::InvalidCanonicalKey),
                "{key}"
            );
        }
    }
}
