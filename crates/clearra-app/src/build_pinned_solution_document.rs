//! Required drawings are constraints on a separately regenerated source, not a
//! target document or a replacement candidate family. Mirrored drawings may
//! occupy different cells; exact membership is still owned by BuildCoverV2.

use clearra_core_domain::solution::StandardBoard64ColoredTilingIdentity;

use crate::{
    build_colored_target_document::decode_colored_pages, BuildColoredTargetDocumentError,
    FieldDocumentFormat,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildPinnedSolutionDocument {
    identities: Vec<StandardBoard64ColoredTilingIdentity>,
}

impl BuildPinnedSolutionDocument {
    pub fn decode(
        format: FieldDocumentFormat,
        source: &str,
    ) -> Result<Self, BuildColoredTargetDocumentError> {
        let (_, _, _, mut identities) = decode_colored_pages(format, source)?;
        let first = identities
            .first()
            .ok_or(BuildColoredTargetDocumentError::EmptyDocument)?;
        let initial = first.initial_board_mask();
        let pieces = first.placement_count();
        if pieces == 0 {
            return Err(BuildColoredTargetDocumentError::TargetEmpty);
        }
        for identity in &identities {
            if identity.initial_board_mask() != initial {
                return Err(BuildColoredTargetDocumentError::InitialBoardDiffers);
            }
            if identity.placement_count() != pieces {
                return Err(BuildColoredTargetDocumentError::TargetAreaDiffers);
            }
        }
        // Decoder construction checked width, height, garbage, colored area and
        // disjointness. Do not infer legal placements or coverage here: the full
        // producer must resolve every drawing to exactly one accepted identity.
        identities.sort_unstable();
        identities.dedup();
        Ok(Self { identities })
    }

    pub fn into_identities(self) -> Vec<StandardBoard64ColoredTilingIdentity> {
        self.identities
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BuildColoredTargetDocument;
    use clearra_ctk3::{encode_ctk3, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece};
    use fumen::{CellColor, Fumen, Page};

    fn page(mask: u64, initial: u64) -> Ctk3Page {
        Ctk3Page::new(
            1,
            (0..10)
                .map(|x| {
                    let bit = 1_u64 << x;
                    if initial & bit != 0 {
                        Ctk3Color::Gray
                    } else if mask & bit != 0 {
                        Ctk3Color::Piece(Ctk3Piece::I)
                    } else {
                        Ctk3Color::Empty
                    }
                })
                .collect(),
        )
    }

    #[test]
    fn mirrored_pins_are_not_a_single_target_document() {
        let source = encode_ctk3(&Ctk3Document::new(
            10,
            vec![page(0xf, 0), page(0x3c0, 0), page(0xf, 0)],
        ))
        .unwrap();
        let identities = BuildPinnedSolutionDocument::decode(FieldDocumentFormat::Ctk3, &source)
            .unwrap()
            .into_identities();
        assert_eq!(identities.len(), 2);
        assert_eq!(identities[0].piece_masks()[0], 0xf);
        assert_eq!(identities[1].piece_masks()[0], 0x3c0);
        // Normal target-search semantics are not relaxed to admit these pages.
        assert_eq!(
            BuildColoredTargetDocument::decode(FieldDocumentFormat::Ctk3, &source),
            Err(BuildColoredTargetDocumentError::TargetMaskDiffers)
        );
        let mut left = Page::default();
        left.field[0][0..4].fill(CellColor::I);
        let mut right = Page::default();
        right.field[0][6..10].fill(CellColor::I);
        let fumen = Fumen {
            pages: vec![left, right],
            guideline: true,
        }
        .encode();
        assert_eq!(
            BuildPinnedSolutionDocument::decode(FieldDocumentFormat::Fumen, &fumen)
                .unwrap()
                .into_identities(),
            identities
        );
    }

    #[test]
    fn pins_keep_initial_board_area_and_wire_validation() {
        for (pages, error) in [
            (
                vec![page(0xf, 0), page(0x3c0, 0x10)],
                BuildColoredTargetDocumentError::InitialBoardDiffers,
            ),
            (
                vec![page(0xf, 0), page(0xff, 0)],
                BuildColoredTargetDocumentError::TargetAreaDiffers,
            ),
            (
                vec![page(0, 0x10)],
                BuildColoredTargetDocumentError::TargetEmpty,
            ),
            (
                vec![page(0, 0)],
                BuildColoredTargetDocumentError::HeightInvalid,
            ),
            (
                vec![page(0x7, 0)],
                BuildColoredTargetDocumentError::ColoredAreaInvalid,
            ),
        ] {
            let source = encode_ctk3(&Ctk3Document::new(10, pages)).unwrap();
            assert_eq!(
                BuildPinnedSolutionDocument::decode(FieldDocumentFormat::Ctk3, &source),
                Err(error)
            );
        }
        assert!(BuildPinnedSolutionDocument::decode(FieldDocumentFormat::Ctk3, "ctk3_").is_err());
    }
}
