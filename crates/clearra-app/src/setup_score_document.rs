use clearra_core_domain::solution::StandardBoard64ColoredTilingIdentity;
use clearra_ctk3::{decode_ctk3_exact, encode_ctk3, Ctk3Color, Ctk3Piece};
use clearra_fumen::{ActualFumenDocumentTransform, SourceFumenColoredFieldSet};
use sha2::{Digest, Sha256};

use crate::{
    build_solution_probability_result::build_v2_supplied_result::colored_candidate_key,
    FieldDocumentFormat,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupScoreDocumentError {
    DecodeFailed,
    EmptyDocument,
    WidthInvalid,
    HeightInvalid,
    PendingGarbageUnsupported,
    InitialBoardDiffers,
    SetupPieceCountDiffers,
    SetupEmpty,
    ColoredAreaInvalid,
}

/// Canonical, deduplicated Setup score document.
///
/// Each colored page is one Setup candidate. Pages may occupy different cells,
/// but they must describe the same initial board and the same number of Setup
/// pieces so one Setup queue universe remains authoritative for the ranking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupScoreDocumentV1 {
    format: FieldDocumentFormat,
    document_hash: String,
    source_page_count: usize,
    visible_height: u8,
    setup_piece_count: usize,
    candidates: Vec<SetupScoreDocumentCandidateV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupScoreDocumentCandidateV1 {
    candidate_id: String,
    identity: StandardBoard64ColoredTilingIdentity,
    target_cells_mask: u64,
}

impl SetupScoreDocumentV1 {
    pub fn decode(
        format: FieldDocumentFormat,
        source: &str,
    ) -> Result<Self, SetupScoreDocumentError> {
        let (source_page_count, visible_height, canonical, identities) = match format {
            FieldDocumentFormat::Ctk3 => decode_ctk3(source)?,
            FieldDocumentFormat::Fumen => decode_fumen(source)?,
        };
        validate_document(
            format,
            source_page_count,
            visible_height,
            canonical,
            identities,
        )
    }

    pub const fn format(&self) -> FieldDocumentFormat {
        self.format
    }

    pub fn document_hash(&self) -> &str {
        &self.document_hash
    }

    pub const fn source_page_count(&self) -> usize {
        self.source_page_count
    }

    pub const fn visible_height(&self) -> u8 {
        self.visible_height
    }

    pub const fn setup_piece_count(&self) -> usize {
        self.setup_piece_count
    }

    pub fn candidates(&self) -> &[SetupScoreDocumentCandidateV1] {
        &self.candidates
    }
}

impl SetupScoreDocumentCandidateV1 {
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub const fn identity(&self) -> StandardBoard64ColoredTilingIdentity {
        self.identity
    }

    pub const fn initial_board_mask(&self) -> u64 {
        self.identity.initial_board_mask()
    }

    pub const fn target_cells_mask(&self) -> u64 {
        self.target_cells_mask
    }

    pub const fn completed_board_mask(&self) -> u64 {
        self.identity.initial_board_mask() | self.target_cells_mask
    }
}

type Decoded = (usize, u8, String, Vec<StandardBoard64ColoredTilingIdentity>);

fn decode_ctk3(source: &str) -> Result<Decoded, SetupScoreDocumentError> {
    let document = decode_ctk3_exact(source).map_err(|_| SetupScoreDocumentError::DecodeFailed)?;
    if document.width != 10 {
        return Err(SetupScoreDocumentError::WidthInvalid);
    }
    if document.pages.is_empty() {
        return Err(SetupScoreDocumentError::EmptyDocument);
    }
    let source_page_count = document.pages.len();
    let mut visible_height = 0_u8;
    let mut identities = Vec::with_capacity(source_page_count);
    for page in &document.pages {
        if !(1..=6).contains(&page.height) || page.cells.len() != page.height * 10 {
            return Err(SetupScoreDocumentError::HeightInvalid);
        }
        if page.garbage.is_some() {
            return Err(SetupScoreDocumentError::PendingGarbageUnsupported);
        }
        visible_height = visible_height.max(page.height as u8);
        let mut initial = 0_u64;
        let mut pieces = [0_u64; 7];
        for (index, color) in page.cells.iter().copied().enumerate() {
            let bit = 1_u64 << index;
            match color {
                Ctk3Color::Empty => {}
                Ctk3Color::Gray => initial |= bit,
                Ctk3Color::Piece(piece) => pieces[piece_index(piece)] |= bit,
            }
        }
        identities.push(
            StandardBoard64ColoredTilingIdentity::from_piece_masks(initial, pieces)
                .map_err(|_| SetupScoreDocumentError::ColoredAreaInvalid)?,
        );
    }
    let canonical = encode_ctk3(&document).map_err(|_| SetupScoreDocumentError::DecodeFailed)?;
    Ok((source_page_count, visible_height, canonical, identities))
}

fn decode_fumen(source: &str) -> Result<Decoded, SetupScoreDocumentError> {
    let decoded = SourceFumenColoredFieldSet::decode(source)
        .map_err(|_| SetupScoreDocumentError::DecodeFailed)?;
    if decoded.page_count() == 0 {
        return Err(SetupScoreDocumentError::EmptyDocument);
    }
    if !(1..=6).contains(&decoded.visible_height()) {
        return Err(SetupScoreDocumentError::HeightInvalid);
    }
    let canonical = ActualFumenDocumentTransform::roundtrip(source)
        .map_err(|_| SetupScoreDocumentError::DecodeFailed)?;
    Ok((
        decoded.page_count(),
        decoded.visible_height(),
        canonical,
        decoded.identities().to_vec(),
    ))
}

fn validate_document(
    format: FieldDocumentFormat,
    source_page_count: usize,
    visible_height: u8,
    canonical: String,
    mut identities: Vec<StandardBoard64ColoredTilingIdentity>,
) -> Result<SetupScoreDocumentV1, SetupScoreDocumentError> {
    if source_page_count == 0 || identities.is_empty() {
        return Err(SetupScoreDocumentError::EmptyDocument);
    }
    let initial_board_mask = identities[0].initial_board_mask();
    let setup_piece_count = identities[0].placement_count();
    if setup_piece_count == 0 {
        return Err(SetupScoreDocumentError::SetupEmpty);
    }
    let board_limit = if visible_height == 6 {
        (1_u64 << 60) - 1
    } else {
        (1_u64 << (usize::from(visible_height) * 10)) - 1
    };
    for identity in &identities {
        let target = colored_union(*identity);
        if identity.initial_board_mask() != initial_board_mask {
            return Err(SetupScoreDocumentError::InitialBoardDiffers);
        }
        if identity.placement_count() != setup_piece_count {
            return Err(SetupScoreDocumentError::SetupPieceCountDiffers);
        }
        if target == 0 {
            return Err(SetupScoreDocumentError::SetupEmpty);
        }
        if (identity.initial_board_mask() | target) & !board_limit != 0 {
            return Err(SetupScoreDocumentError::HeightInvalid);
        }
    }
    identities.sort_unstable();
    identities.dedup();
    let candidates = identities
        .into_iter()
        .map(|identity| SetupScoreDocumentCandidateV1 {
            candidate_id: colored_candidate_key(identity),
            target_cells_mask: colored_union(identity),
            identity,
        })
        .collect();
    Ok(SetupScoreDocumentV1 {
        format,
        document_hash: format!("{:x}", Sha256::digest(canonical.as_bytes())),
        source_page_count,
        visible_height,
        setup_piece_count,
        candidates,
    })
}

fn colored_union(identity: StandardBoard64ColoredTilingIdentity) -> u64 {
    identity
        .piece_masks()
        .into_iter()
        .fold(0_u64, |union, mask| union | mask)
}

const fn piece_index(piece: Ctk3Piece) -> usize {
    match piece {
        Ctk3Piece::I => 0,
        Ctk3Piece::O => 1,
        Ctk3Piece::T => 2,
        Ctk3Piece::S => 3,
        Ctk3Piece::Z => 4,
        Ctk3Piece::J => 5,
        Ctk3Piece::L => 6,
    }
}

#[cfg(test)]
mod tests {
    use clearra_ctk3::{Ctk3Document, Ctk3Page};

    use super::*;

    fn page(offset: usize) -> Ctk3Page {
        let mut cells = vec![Ctk3Color::Empty; 20];
        for cell in &mut cells[offset..offset + 4] {
            *cell = Ctk3Color::Piece(Ctk3Piece::I);
        }
        Ctk3Page::new(2, cells)
    }

    #[test]
    fn accepts_distinct_setup_cells_and_deduplicates_equal_pages() {
        let source = encode_ctk3(&Ctk3Document::new(10, vec![page(0), page(10), page(0)])).unwrap();
        let document = SetupScoreDocumentV1::decode(FieldDocumentFormat::Ctk3, &source).unwrap();
        assert_eq!(document.source_page_count(), 3);
        assert_eq!(document.candidates().len(), 2);
        assert_eq!(document.setup_piece_count(), 1);
        assert_eq!(document.document_hash().len(), 64);
        assert!(document.candidates()[0].candidate_id() < document.candidates()[1].candidate_id());
    }

    #[test]
    fn rejects_mixed_setup_piece_counts() {
        let mut second = page(0);
        second.cells[10..14].fill(Ctk3Color::Piece(Ctk3Piece::O));
        let source = encode_ctk3(&Ctk3Document::new(10, vec![page(0), second])).unwrap();
        assert_eq!(
            SetupScoreDocumentV1::decode(FieldDocumentFormat::Ctk3, &source),
            Err(SetupScoreDocumentError::SetupPieceCountDiffers)
        );
    }
}
