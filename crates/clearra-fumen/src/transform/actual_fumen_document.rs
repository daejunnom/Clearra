use core::fmt;

use fumen::{CellColor, Fumen, Page, Piece, PieceType, RotationState};

use crate::{
    codec::{source_fumen_diagram::decode_document, FUMEN_MAX_INPUT_BYTES, FUMEN_MAX_PAGES},
    SourceFumenDiagramError,
};

/// Typed, bounded transforms over real v115 Fumen pages.
///
/// The older `FumenLikeTrace` contract intentionally stores Clearra metadata in
/// page comments. Product document transforms must instead operate on the
/// decoded field, operation, flags, garbage row, and comment owned by the
/// `fumen` document. Keeping this authority separate prevents a marker-only
/// comment rewrite from being reported as a geometric transform.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ActualFumenDocumentTransform;

impl ActualFumenDocumentTransform {
    pub fn roundtrip(source: &str) -> Result<String, ActualFumenTransformError> {
        encode_checked(decode_bounded(source)?)
    }

    pub fn combine(sources: &[String]) -> Result<String, ActualFumenTransformError> {
        if sources.is_empty() {
            return Err(ActualFumenTransformError::EmptyDocumentSet);
        }
        let total_input_bytes = sources.iter().try_fold(0_usize, |total, source| {
            total
                .checked_add(source.len())
                .ok_or(ActualFumenTransformError::InputTooLarge {
                    length: usize::MAX,
                    maximum: FUMEN_MAX_INPUT_BYTES,
                })
        })?;
        if total_input_bytes > FUMEN_MAX_INPUT_BYTES {
            return Err(ActualFumenTransformError::InputTooLarge {
                length: total_input_bytes,
                maximum: FUMEN_MAX_INPUT_BYTES,
            });
        }
        let mut combined = Fumen::default();
        for source in sources {
            let mut document = decode_bounded(source)?;
            let combined_len = combined
                .pages
                .len()
                .checked_add(document.pages.len())
                .ok_or(ActualFumenTransformError::TooManyPages {
                    length: usize::MAX,
                    maximum: FUMEN_MAX_PAGES,
                })?;
            if combined_len > FUMEN_MAX_PAGES {
                return Err(ActualFumenTransformError::TooManyPages {
                    length: combined_len,
                    maximum: FUMEN_MAX_PAGES,
                });
            }
            combined.pages.append(&mut document.pages);
        }
        encode_checked(combined)
    }

    pub fn split(source: &str) -> Result<Vec<String>, ActualFumenTransformError> {
        let document = decode_bounded(source)?;
        let mut encoded_pages = Vec::new();
        encoded_pages
            .try_reserve(document.pages.len())
            .map_err(|_| ActualFumenTransformError::CapacityExceeded)?;
        let mut total_output_bytes = 0_usize;
        for page in document.pages {
            let encoded = encode_checked(Fumen {
                pages: vec![page],
                guideline: document.guideline,
            })?;
            total_output_bytes = total_output_bytes.checked_add(encoded.len()).ok_or(
                ActualFumenTransformError::OutputTooLarge {
                    length: usize::MAX,
                    maximum: FUMEN_MAX_INPUT_BYTES,
                },
            )?;
            if total_output_bytes > FUMEN_MAX_INPUT_BYTES {
                return Err(ActualFumenTransformError::OutputTooLarge {
                    length: total_output_bytes,
                    maximum: FUMEN_MAX_INPUT_BYTES,
                });
            }
            encoded_pages.push(encoded);
        }
        Ok(encoded_pages)
    }

    pub fn get_page(source: &str, page_index: usize) -> Result<String, ActualFumenTransformError> {
        let mut document = decode_bounded(source)?;
        if page_index >= document.pages.len() {
            return Err(ActualFumenTransformError::PageIndexOutOfRange {
                page_index,
                page_count: document.pages.len(),
            });
        }
        let page = document.pages.remove(page_index);
        encode_checked(Fumen {
            pages: vec![page],
            guideline: document.guideline,
        })
    }

    pub fn page_shift(source: &str, offset: isize) -> Result<String, ActualFumenTransformError> {
        let mut document = decode_bounded(source)?;
        let amount = offset.rem_euclid(document.pages.len() as isize) as usize;
        document.pages.rotate_left(amount);
        encode_checked(document)
    }

    pub fn clean_comments(source: &str) -> Result<String, ActualFumenTransformError> {
        let mut document = decode_bounded(source)?;
        for page in &mut document.pages {
            page.comment = None;
        }
        encode_checked(document)
    }

    pub fn preserve_comments(source: &str) -> Result<String, ActualFumenTransformError> {
        Self::roundtrip(source)
    }

    pub fn to_gray(source: &str) -> Result<String, ActualFumenTransformError> {
        let mut document = decode_bounded(source)?;
        for page in &mut document.pages {
            for row in &mut page.field {
                for cell in row {
                    *cell = gray(*cell);
                }
            }
            for cell in &mut page.garbage_row {
                *cell = gray(*cell);
            }
        }
        encode_checked(document)
    }

    /// Mirrors every absolute field and concrete operation horizontally.
    /// Comments and page flags are retained. Tetromino colors and operation
    /// kinds map J/L and S/Z, and true rotations are reflected fieldwise.
    pub fn mirror(source: &str) -> Result<String, ActualFumenTransformError> {
        let mut document = decode_bounded(source)?;
        for page in &mut document.pages {
            mirror_page(page)?;
        }
        encode_checked(document)
    }

    /// Creates real Fumen comment pages from bounded text. This is the
    /// compatibility authority for the legacy text-to-fumen preset; text is
    /// metadata only and never interpreted as field or operation evidence.
    pub fn text_to_fumen(comments: &[String]) -> Result<String, ActualFumenTransformError> {
        if comments.is_empty() {
            return Err(ActualFumenTransformError::EmptyDocumentSet);
        }
        if comments.len() > FUMEN_MAX_PAGES {
            return Err(ActualFumenTransformError::TooManyPages {
                length: comments.len(),
                maximum: FUMEN_MAX_PAGES,
            });
        }
        let total_comment_bytes = comments.iter().try_fold(0_usize, |total, comment| {
            total
                .checked_add(comment.len())
                .ok_or(ActualFumenTransformError::InputTooLarge {
                    length: usize::MAX,
                    maximum: FUMEN_MAX_INPUT_BYTES,
                })
        })?;
        if total_comment_bytes > FUMEN_MAX_INPUT_BYTES {
            return Err(ActualFumenTransformError::InputTooLarge {
                length: total_comment_bytes,
                maximum: FUMEN_MAX_INPUT_BYTES,
            });
        }
        let mut document = Fumen::default();
        document
            .pages
            .try_reserve(comments.len())
            .map_err(|_| ActualFumenTransformError::CapacityExceeded)?;
        for comment in comments {
            let page = Page {
                comment: Some(comment.clone()),
                ..Page::default()
            };
            document.pages.push(page);
        }
        encode_checked(document)
    }
}

fn decode_bounded(source: &str) -> Result<Fumen, ActualFumenTransformError> {
    if source.len() > FUMEN_MAX_INPUT_BYTES {
        return Err(ActualFumenTransformError::InputTooLarge {
            length: source.len(),
            maximum: FUMEN_MAX_INPUT_BYTES,
        });
    }
    let document = decode_document(source).map_err(ActualFumenTransformError::Decode)?;
    if document.pages.is_empty() {
        return Err(ActualFumenTransformError::EmptyDocument);
    }
    if document.pages.len() > FUMEN_MAX_PAGES {
        return Err(ActualFumenTransformError::TooManyPages {
            length: document.pages.len(),
            maximum: FUMEN_MAX_PAGES,
        });
    }
    Ok(document)
}

fn encode_checked(document: Fumen) -> Result<String, ActualFumenTransformError> {
    if document.pages.is_empty() {
        return Err(ActualFumenTransformError::EmptyDocument);
    }
    if document.pages.len() > FUMEN_MAX_PAGES {
        return Err(ActualFumenTransformError::TooManyPages {
            length: document.pages.len(),
            maximum: FUMEN_MAX_PAGES,
        });
    }
    let expected = document.clone();
    let encoded = document.encode();
    if encoded.len() > FUMEN_MAX_INPUT_BYTES {
        return Err(ActualFumenTransformError::OutputTooLarge {
            length: encoded.len(),
            maximum: FUMEN_MAX_INPUT_BYTES,
        });
    }
    let decoded =
        Fumen::decode(&encoded).map_err(|_| ActualFumenTransformError::RoundTripDecode)?;
    if decoded != expected {
        return Err(ActualFumenTransformError::RoundTripMismatch);
    }
    Ok(encoded)
}

fn mirror_page(page: &mut Page) -> Result<(), ActualFumenTransformError> {
    for row in &mut page.field {
        row.reverse();
        for cell in row {
            *cell = mirror_color(*cell);
        }
    }
    page.garbage_row.reverse();
    for cell in &mut page.garbage_row {
        *cell = mirror_color(*cell);
    }
    if let Some(piece) = page.piece {
        page.piece = Some(mirror_piece(piece)?);
    }
    Ok(())
}

fn mirror_piece(piece: Piece) -> Result<Piece, ActualFumenTransformError> {
    let mirrored_x = match piece.kind {
        PieceType::O => match piece.rotation {
            RotationState::South | RotationState::East => 8_i64 - i64::from(piece.x),
            RotationState::North | RotationState::West => 10_i64 - i64::from(piece.x),
        },
        _ => 9_i64 - i64::from(piece.x),
    };
    let x = u32::try_from(mirrored_x)
        .map_err(|_| ActualFumenTransformError::MirroredOperationOutsideField { x: mirrored_x })?;
    Ok(Piece {
        kind: mirror_piece_type(piece.kind),
        rotation: mirror_rotation(piece.kind, piece.rotation),
        x,
        y: piece.y,
    })
}

const fn mirror_rotation(piece: PieceType, rotation: RotationState) -> RotationState {
    match piece {
        PieceType::O => rotation,
        PieceType::I => match rotation {
            RotationState::South => RotationState::North,
            RotationState::East => RotationState::East,
            RotationState::North => RotationState::South,
            RotationState::West => RotationState::West,
        },
        PieceType::T | PieceType::S | PieceType::Z | PieceType::J | PieceType::L => {
            match rotation {
                RotationState::South => RotationState::South,
                RotationState::East => RotationState::West,
                RotationState::North => RotationState::North,
                RotationState::West => RotationState::East,
            }
        }
    }
}

const fn mirror_piece_type(piece: PieceType) -> PieceType {
    match piece {
        PieceType::J => PieceType::L,
        PieceType::L => PieceType::J,
        PieceType::S => PieceType::Z,
        PieceType::Z => PieceType::S,
        PieceType::I | PieceType::O | PieceType::T => piece,
    }
}

const fn gray(color: CellColor) -> CellColor {
    match color {
        CellColor::Empty => CellColor::Empty,
        CellColor::I
        | CellColor::O
        | CellColor::T
        | CellColor::S
        | CellColor::Z
        | CellColor::J
        | CellColor::L
        | CellColor::Grey => CellColor::Grey,
    }
}

const fn mirror_color(color: CellColor) -> CellColor {
    match color {
        CellColor::J => CellColor::L,
        CellColor::L => CellColor::J,
        CellColor::S => CellColor::Z,
        CellColor::Z => CellColor::S,
        CellColor::Empty | CellColor::I | CellColor::O | CellColor::T | CellColor::Grey => color,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActualFumenTransformError {
    InputTooLarge {
        length: usize,
        maximum: usize,
    },
    OutputTooLarge {
        length: usize,
        maximum: usize,
    },
    TooManyPages {
        length: usize,
        maximum: usize,
    },
    EmptyDocumentSet,
    EmptyDocument,
    PageIndexOutOfRange {
        page_index: usize,
        page_count: usize,
    },
    MirroredOperationOutsideField {
        x: i64,
    },
    CapacityExceeded,
    Decode(SourceFumenDiagramError),
    RoundTripDecode,
    RoundTripMismatch,
}

impl fmt::Display for ActualFumenTransformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ActualFumenTransformError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded_document() -> String {
        let mut first = Page::default();
        first.field[0][0] = CellColor::J;
        first.field[0][9] = CellColor::S;
        first.garbage_row[1] = CellColor::L;
        first.comment = Some("keep first".to_owned());
        first.piece = Some(Piece {
            kind: PieceType::J,
            rotation: RotationState::East,
            x: 4,
            y: 2,
        });
        let mut second = Page::default();
        second.field[1][2] = CellColor::I;
        second.comment = Some("keep second".to_owned());
        second.piece = Some(Piece {
            kind: PieceType::O,
            rotation: RotationState::South,
            x: 3,
            y: 1,
        });
        Fumen {
            pages: vec![first, second],
            guideline: true,
        }
        .encode()
    }

    #[test]
    fn real_v115_roundtrip_combine_split_and_page_selection_preserve_typed_pages() {
        let source = encoded_document();
        let roundtrip = ActualFumenDocumentTransform::roundtrip(&source).expect("roundtrip");
        assert_eq!(Fumen::decode(&roundtrip), Fumen::decode(&source));

        let split = ActualFumenDocumentTransform::split(&source).expect("split");
        assert_eq!(split.len(), 2);
        let combined = ActualFumenDocumentTransform::combine(&split).expect("combine");
        assert_eq!(Fumen::decode(&combined), Fumen::decode(&source));

        let selected = ActualFumenDocumentTransform::get_page(&source, 1).expect("page 1");
        assert_eq!(
            Fumen::decode(&selected).unwrap().pages[0]
                .comment
                .as_deref(),
            Some("keep second")
        );
        assert!(matches!(
            ActualFumenDocumentTransform::get_page(&source, 2),
            Err(ActualFumenTransformError::PageIndexOutOfRange { .. })
        ));
    }

    #[test]
    fn real_v115_gray_changes_only_occupancy_color() {
        let source = encoded_document();
        let before = Fumen::decode(&source).unwrap();
        let gray = ActualFumenDocumentTransform::to_gray(&source).expect("gray");
        let after = Fumen::decode(&gray).unwrap();
        assert_eq!(after.pages[0].field[0][0], CellColor::Grey);
        assert_eq!(after.pages[0].field[0][9], CellColor::Grey);
        assert_eq!(after.pages[0].garbage_row[1], CellColor::Grey);
        assert_eq!(after.pages[0].piece, before.pages[0].piece);
        assert_eq!(after.pages[0].comment, before.pages[0].comment);
        assert_eq!(after.pages[0].lock, before.pages[0].lock);
        assert_eq!(after.pages[0].rise, before.pages[0].rise);
        assert_eq!(after.pages[0].mirror, before.pages[0].mirror);
    }

    #[test]
    fn real_v115_mirror_transforms_field_color_operation_and_is_an_involution() {
        let source = encoded_document();
        let once = ActualFumenDocumentTransform::mirror(&source).expect("mirror");
        let mirrored = Fumen::decode(&once).unwrap();
        assert_eq!(mirrored.pages[0].field[0][9], CellColor::L);
        assert_eq!(mirrored.pages[0].field[0][0], CellColor::Z);
        assert_eq!(mirrored.pages[0].garbage_row[8], CellColor::J);
        assert_eq!(
            mirrored.pages[0].piece,
            Some(Piece {
                kind: PieceType::L,
                rotation: RotationState::West,
                x: 5,
                y: 2,
            })
        );
        assert_eq!(
            mirrored.pages[1].piece,
            Some(Piece {
                kind: PieceType::O,
                rotation: RotationState::South,
                x: 5,
                y: 1,
            })
        );
        let twice = ActualFumenDocumentTransform::mirror(&once).expect("double mirror");
        assert_eq!(Fumen::decode(&twice), Fumen::decode(&source));
    }

    #[test]
    fn real_v115_double_mirror_preserves_every_piece_and_true_rotation() {
        let mut pages = Vec::new();
        for kind in [
            PieceType::I,
            PieceType::O,
            PieceType::T,
            PieceType::S,
            PieceType::Z,
            PieceType::J,
            PieceType::L,
        ] {
            for rotation in [
                RotationState::South,
                RotationState::East,
                RotationState::North,
                RotationState::West,
            ] {
                let page = Page {
                    piece: Some(Piece {
                        kind,
                        rotation,
                        x: 4,
                        y: 4,
                    }),
                    ..Page::default()
                };
                pages.push(page);
            }
        }
        let source = Fumen {
            pages,
            guideline: true,
        }
        .encode();
        let once = ActualFumenDocumentTransform::mirror(&source).unwrap();
        let twice = ActualFumenDocumentTransform::mirror(&once).unwrap();
        assert_eq!(Fumen::decode(&twice), Fumen::decode(&source));
    }

    #[test]
    fn comments_and_page_order_are_typed_transforms() {
        let source = encoded_document();
        let shifted = ActualFumenDocumentTransform::page_shift(&source, 1).expect("shift");
        let shifted = Fumen::decode(&shifted).unwrap();
        assert_eq!(shifted.pages[0].comment.as_deref(), Some("keep second"));

        let cleaned = ActualFumenDocumentTransform::clean_comments(&source).expect("clean");
        assert!(Fumen::decode(&cleaned)
            .unwrap()
            .pages
            .iter()
            .all(|page| page.comment.is_none()));

        let text =
            ActualFumenDocumentTransform::text_to_fumen(&["alpha".to_owned(), "한글".to_owned()])
                .expect("text pages");
        let comments = Fumen::decode(&text)
            .unwrap()
            .pages
            .into_iter()
            .map(|page| page.comment)
            .collect::<Vec<_>>();
        assert_eq!(
            comments,
            vec![Some("alpha".to_owned()), Some("한글".to_owned())]
        );
    }
}
