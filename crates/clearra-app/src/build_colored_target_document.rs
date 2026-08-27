use clearra_core_domain::solution::StandardBoard64ColoredTilingIdentity;
use clearra_ctk3::{decode_ctk3_exact, encode_ctk3, Ctk3Color, Ctk3Piece};
use clearra_fumen::{ActualFumenDocumentTransform, SourceFumenColoredFieldSet};
use sha2::{Digest, Sha256};

use crate::{
    build_solution_probability_result::build_v2_facade::BuildColoredTargetSetV1,
    FieldDocumentFormat,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildColoredTargetDocumentError {
    DecodeFailed,
    EmptyDocument,
    WidthInvalid,
    HeightInvalid,
    PendingGarbageUnsupported,
    InitialBoardDiffers,
    TargetMaskDiffers,
    TargetEmpty,
    InitialTargetOverlap,
    ColoredAreaInvalid,
    TargetRejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildColoredTargetDocument {
    target: BuildColoredTargetSetV1,
    target_cells_mask: u64,
    source_piece_count: usize,
}

impl BuildColoredTargetDocument {
    pub fn decode(
        format: FieldDocumentFormat,
        source: &str,
    ) -> Result<Self, BuildColoredTargetDocumentError> {
        let (page_count, visible_height, canonical, identities) = match format {
            FieldDocumentFormat::Ctk3 => decode_ctk3(source)?,
            FieldDocumentFormat::Fumen => decode_fumen(source)?,
        };
        validate_and_build(page_count, visible_height, canonical, identities)
    }

    pub const fn target(&self) -> &BuildColoredTargetSetV1 {
        &self.target
    }

    pub const fn target_cells_mask(&self) -> u64 {
        self.target_cells_mask
    }

    pub const fn source_piece_count(&self) -> usize {
        self.source_piece_count
    }

    pub fn into_target(self) -> BuildColoredTargetSetV1 {
        self.target
    }
}

type Decoded = (usize, u8, String, Vec<StandardBoard64ColoredTilingIdentity>);

fn decode_ctk3(source: &str) -> Result<Decoded, BuildColoredTargetDocumentError> {
    let document =
        decode_ctk3_exact(source).map_err(|_| BuildColoredTargetDocumentError::DecodeFailed)?;
    if document.width != 10 {
        return Err(BuildColoredTargetDocumentError::WidthInvalid);
    }
    if document.pages.is_empty() {
        return Err(BuildColoredTargetDocumentError::EmptyDocument);
    }
    let page_count = document.pages.len();
    let mut visible_height = 0_u8;
    let mut identities = Vec::with_capacity(page_count);
    for page in &document.pages {
        if !(1..=6).contains(&page.height) || page.cells.len() != page.height * 10 {
            return Err(BuildColoredTargetDocumentError::HeightInvalid);
        }
        if page.garbage.is_some() {
            return Err(BuildColoredTargetDocumentError::PendingGarbageUnsupported);
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
                .map_err(|_| BuildColoredTargetDocumentError::ColoredAreaInvalid)?,
        );
    }
    let canonical =
        encode_ctk3(&document).map_err(|_| BuildColoredTargetDocumentError::DecodeFailed)?;
    Ok((page_count, visible_height, canonical, identities))
}

fn decode_fumen(source: &str) -> Result<Decoded, BuildColoredTargetDocumentError> {
    let decoded = SourceFumenColoredFieldSet::decode(source)
        .map_err(|_| BuildColoredTargetDocumentError::DecodeFailed)?;
    if !(1..=6).contains(&decoded.visible_height()) {
        return Err(BuildColoredTargetDocumentError::HeightInvalid);
    }
    let canonical = ActualFumenDocumentTransform::roundtrip(source)
        .map_err(|_| BuildColoredTargetDocumentError::DecodeFailed)?;
    Ok((
        decoded.page_count(),
        decoded.visible_height(),
        canonical,
        decoded.identities().to_vec(),
    ))
}

fn validate_and_build(
    page_count: usize,
    visible_height: u8,
    canonical: String,
    mut identities: Vec<StandardBoard64ColoredTilingIdentity>,
) -> Result<BuildColoredTargetDocument, BuildColoredTargetDocumentError> {
    if page_count == 0 || identities.is_empty() {
        return Err(BuildColoredTargetDocumentError::EmptyDocument);
    }
    let initial = identities[0].initial_board_mask();
    let target = colored_union(identities[0]);
    if target == 0 {
        return Err(BuildColoredTargetDocumentError::TargetEmpty);
    }
    if initial & target != 0 {
        return Err(BuildColoredTargetDocumentError::InitialTargetOverlap);
    }
    for identity in &identities {
        if identity.initial_board_mask() != initial {
            return Err(BuildColoredTargetDocumentError::InitialBoardDiffers);
        }
        if colored_union(*identity) != target {
            return Err(BuildColoredTargetDocumentError::TargetMaskDiffers);
        }
        if identity
            .piece_masks()
            .iter()
            .any(|mask| mask.count_ones() % 4 != 0)
        {
            return Err(BuildColoredTargetDocumentError::ColoredAreaInvalid);
        }
    }
    identities.sort_unstable();
    identities.dedup();
    let document_hash = format!("{:x}", Sha256::digest(canonical.as_bytes()));
    let source_piece_count = target.count_ones() as usize / 4;
    let target_owner =
        BuildColoredTargetSetV1::new(visible_height, page_count, document_hash, identities)
            .map_err(|_| BuildColoredTargetDocumentError::TargetRejected)?;
    Ok(BuildColoredTargetDocument {
        target: target_owner,
        target_cells_mask: target,
        source_piece_count,
    })
}

fn colored_union(identity: StandardBoard64ColoredTilingIdentity) -> u64 {
    identity
        .piece_masks()
        .into_iter()
        .fold(0, |all, mask| all | mask)
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
    use super::*;
    use clearra_ctk3::{Ctk3Document, Ctk3Page};
    use fumen::{CellColor, Fumen, Page};

    #[test]
    fn ctk3_preserves_original_pages_but_deduplicates_canonical_candidates() {
        let mut cells = vec![Ctk3Color::Empty; 20];
        cells[10] = Ctk3Color::Gray;
        for cell in &mut cells[0..4] {
            *cell = Ctk3Color::Piece(Ctk3Piece::I);
        }
        let document = Ctk3Document::new(
            10,
            vec![Ctk3Page::new(2, cells.clone()), Ctk3Page::new(2, cells)],
        );
        let source = encode_ctk3(&document).unwrap();
        let decoded =
            BuildColoredTargetDocument::decode(FieldDocumentFormat::Ctk3, &source).unwrap();
        assert_eq!(decoded.target().page_count(), 2);
        assert_eq!(decoded.target().identities().len(), 1);
        assert_eq!(decoded.target_cells_mask(), 0xf);
        assert_eq!(decoded.source_piece_count(), 1);
        assert_eq!(decoded.target().document_hash().len(), 64);
    }

    #[test]
    fn rejects_page_target_drift_and_pending_garbage() {
        let page = |offset| {
            let mut cells = vec![Ctk3Color::Empty; 10];
            for cell in &mut cells[offset..offset + 4] {
                *cell = Ctk3Color::Piece(Ctk3Piece::I);
            }
            Ctk3Page::new(1, cells)
        };
        let source = encode_ctk3(&Ctk3Document::new(10, vec![page(0), page(4)])).unwrap();
        assert_eq!(
            BuildColoredTargetDocument::decode(FieldDocumentFormat::Ctk3, &source),
            Err(BuildColoredTargetDocumentError::TargetMaskDiffers)
        );
    }

    #[test]
    fn fumen_preserves_flags_in_document_identity_and_rejects_pending_garbage() {
        let mut page = Page::default();
        page.field[0][0..4].fill(CellColor::I);
        page.field[1][0] = CellColor::Grey;
        page.comment = Some("identity evidence".to_owned());
        let source = Fumen {
            pages: vec![page.clone()],
            guideline: true,
        }
        .encode();
        let decoded = BuildColoredTargetDocument::decode(FieldDocumentFormat::Fumen, &source)
            .expect("colored Fumen target");
        assert_eq!(decoded.target_cells_mask(), 0xf);
        assert_eq!(decoded.target().page_count(), 1);
        assert_eq!(decoded.target().document_hash().len(), 64);

        page.garbage_row[0] = CellColor::Grey;
        let pending = Fumen {
            pages: vec![page],
            guideline: true,
        }
        .encode();
        assert_eq!(
            BuildColoredTargetDocument::decode(FieldDocumentFormat::Fumen, &pending),
            Err(BuildColoredTargetDocumentError::DecodeFailed)
        );
    }
}
