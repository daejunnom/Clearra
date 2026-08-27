use core::fmt;

use crate::geometry::{operation_cells, operation_from_cells};
use crate::{
    decode_ctk3_exact, encode_ctk3, Ctk3CodecError, Ctk3Color, Ctk3Document, Ctk3Operation,
    Ctk3Piece, CTK3_MAX_SEGMENT_PAGES, MAX_PAYLOAD_BYTES,
};

const MAX_TYPED_CTK3_TEXT_BYTES: usize = MAX_PAYLOAD_BYTES * 2;

/// Bounded, lossless transforms over decoded native CTK3 documents.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TypedCtk3DocumentTransform;

impl TypedCtk3DocumentTransform {
    pub fn roundtrip(source: &str) -> Result<String, TypedCtk3TransformError> {
        encode_checked(decode_bounded(source)?)
    }

    pub fn combine(sources: &[String]) -> Result<String, TypedCtk3TransformError> {
        if sources.is_empty() {
            return Err(TypedCtk3TransformError::EmptyDocumentSet);
        }
        let total_input_bytes = sources.iter().try_fold(0_usize, |total, source| {
            total
                .checked_add(source.len())
                .ok_or(TypedCtk3TransformError::InputTooLarge {
                    length: usize::MAX,
                    maximum: MAX_TYPED_CTK3_TEXT_BYTES,
                })
        })?;
        if total_input_bytes > MAX_TYPED_CTK3_TEXT_BYTES {
            return Err(TypedCtk3TransformError::InputTooLarge {
                length: total_input_bytes,
                maximum: MAX_TYPED_CTK3_TEXT_BYTES,
            });
        }

        let mut combined = decode_bounded(&sources[0])?;
        for source in &sources[1..] {
            let mut document = decode_bounded(source)?;
            if document.width != combined.width {
                return Err(TypedCtk3TransformError::WidthMismatch {
                    expected: combined.width,
                    actual: document.width,
                });
            }
            let page_count = combined
                .pages
                .len()
                .checked_add(document.pages.len())
                .ok_or(TypedCtk3TransformError::TooManyPages {
                    length: usize::MAX,
                    maximum: CTK3_MAX_SEGMENT_PAGES,
                })?;
            if page_count > CTK3_MAX_SEGMENT_PAGES {
                return Err(TypedCtk3TransformError::TooManyPages {
                    length: page_count,
                    maximum: CTK3_MAX_SEGMENT_PAGES,
                });
            }
            combined.pages.append(&mut document.pages);
        }
        encode_checked(combined)
    }

    pub fn split(source: &str) -> Result<Vec<String>, TypedCtk3TransformError> {
        let document = decode_bounded(source)?;
        let mut encoded_pages = Vec::new();
        encoded_pages
            .try_reserve(document.pages.len())
            .map_err(|_| TypedCtk3TransformError::CapacityExceeded)?;
        let mut total_output_bytes = 0_usize;
        for page in document.pages {
            let encoded = encode_checked(Ctk3Document::new(document.width, vec![page]))?;
            total_output_bytes = total_output_bytes.checked_add(encoded.len()).ok_or(
                TypedCtk3TransformError::OutputTooLarge {
                    length: usize::MAX,
                    maximum: MAX_TYPED_CTK3_TEXT_BYTES,
                },
            )?;
            if total_output_bytes > MAX_TYPED_CTK3_TEXT_BYTES {
                return Err(TypedCtk3TransformError::OutputTooLarge {
                    length: total_output_bytes,
                    maximum: MAX_TYPED_CTK3_TEXT_BYTES,
                });
            }
            encoded_pages.push(encoded);
        }
        Ok(encoded_pages)
    }

    pub fn get_page(source: &str, page_index: usize) -> Result<String, TypedCtk3TransformError> {
        let mut document = decode_bounded(source)?;
        if page_index >= document.pages.len() {
            return Err(TypedCtk3TransformError::PageIndexOutOfRange {
                page_index,
                page_count: document.pages.len(),
            });
        }
        let page = document.pages.remove(page_index);
        encode_checked(Ctk3Document::new(document.width, vec![page]))
    }

    pub fn page_shift(source: &str, offset: isize) -> Result<String, TypedCtk3TransformError> {
        let mut document = decode_bounded(source)?;
        let amount = offset.rem_euclid(document.pages.len() as isize) as usize;
        document.pages.rotate_left(amount);
        encode_checked(document)
    }

    pub fn clean_comments(source: &str) -> Result<String, TypedCtk3TransformError> {
        let mut document = decode_bounded(source)?;
        for page in &mut document.pages {
            page.comment.clear();
        }
        encode_checked(document)
    }

    pub fn preserve_comments(source: &str) -> Result<String, TypedCtk3TransformError> {
        Self::roundtrip(source)
    }

    pub fn to_gray(source: &str) -> Result<String, TypedCtk3TransformError> {
        let mut document = decode_bounded(source)?;
        for page in &mut document.pages {
            for cell in &mut page.cells {
                if *cell != Ctk3Color::Empty {
                    *cell = Ctk3Color::Gray;
                }
            }
            if let Some(garbage) = &mut page.garbage {
                for cell in garbage {
                    if *cell != Ctk3Color::Empty {
                        *cell = Ctk3Color::Gray;
                    }
                }
            }
        }
        encode_checked(document)
    }

    pub fn mirror(source: &str) -> Result<String, TypedCtk3TransformError> {
        let mut document = decode_bounded(source)?;
        for page in &mut document.pages {
            for row in page.cells.chunks_exact_mut(document.width) {
                row.reverse();
                for cell in row {
                    *cell = mirror_color(*cell);
                }
            }
            if let Some(garbage) = &mut page.garbage {
                garbage.reverse();
                for cell in garbage {
                    *cell = mirror_color(*cell);
                }
            }
            if let Some(operation) = page.operation {
                page.operation = Some(mirror_operation(document.width, operation)?);
            }
        }
        encode_checked(document)
    }
}

fn decode_bounded(source: &str) -> Result<Ctk3Document, TypedCtk3TransformError> {
    if source.len() > MAX_TYPED_CTK3_TEXT_BYTES {
        return Err(TypedCtk3TransformError::InputTooLarge {
            length: source.len(),
            maximum: MAX_TYPED_CTK3_TEXT_BYTES,
        });
    }
    let document = decode_ctk3_exact(source).map_err(TypedCtk3TransformError::Codec)?;
    if document.pages.is_empty() {
        return Err(TypedCtk3TransformError::EmptyDocument);
    }
    if document.pages.len() > CTK3_MAX_SEGMENT_PAGES {
        return Err(TypedCtk3TransformError::TooManyPages {
            length: document.pages.len(),
            maximum: CTK3_MAX_SEGMENT_PAGES,
        });
    }
    Ok(document)
}

fn encode_checked(document: Ctk3Document) -> Result<String, TypedCtk3TransformError> {
    if document.pages.is_empty() {
        return Err(TypedCtk3TransformError::EmptyDocument);
    }
    if document.pages.len() > CTK3_MAX_SEGMENT_PAGES {
        return Err(TypedCtk3TransformError::TooManyPages {
            length: document.pages.len(),
            maximum: CTK3_MAX_SEGMENT_PAGES,
        });
    }
    let encoded = encode_ctk3(&document).map_err(TypedCtk3TransformError::Codec)?;
    if encoded.len() > MAX_TYPED_CTK3_TEXT_BYTES {
        return Err(TypedCtk3TransformError::OutputTooLarge {
            length: encoded.len(),
            maximum: MAX_TYPED_CTK3_TEXT_BYTES,
        });
    }
    let decoded = decode_ctk3_exact(&encoded).map_err(TypedCtk3TransformError::Codec)?;
    if decoded != document {
        return Err(TypedCtk3TransformError::RoundTripMismatch);
    }
    Ok(encoded)
}

fn mirror_operation(
    width: usize,
    operation: Ctk3Operation,
) -> Result<Ctk3Operation, TypedCtk3TransformError> {
    let width = i64::try_from(width).map_err(|_| TypedCtk3TransformError::CapacityExceeded)?;
    let target = operation_cells(operation).map(|(x, y)| (width - 1 - x, y));
    operation_from_cells(mirror_piece(operation.piece), target)
        .ok_or(TypedCtk3TransformError::OperationMirrorUnavailable)
}

const fn mirror_piece(piece: Ctk3Piece) -> Ctk3Piece {
    match piece {
        Ctk3Piece::J => Ctk3Piece::L,
        Ctk3Piece::L => Ctk3Piece::J,
        Ctk3Piece::S => Ctk3Piece::Z,
        Ctk3Piece::Z => Ctk3Piece::S,
        Ctk3Piece::I | Ctk3Piece::O | Ctk3Piece::T => piece,
    }
}

const fn mirror_color(color: Ctk3Color) -> Ctk3Color {
    match color {
        Ctk3Color::Piece(piece) => Ctk3Color::Piece(mirror_piece(piece)),
        Ctk3Color::Empty | Ctk3Color::Gray => color,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedCtk3TransformError {
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
    WidthMismatch {
        expected: usize,
        actual: usize,
    },
    PageIndexOutOfRange {
        page_index: usize,
        page_count: usize,
    },
    OperationMirrorUnavailable,
    CapacityExceeded,
    Codec(Ctk3CodecError),
    RoundTripMismatch,
}

impl fmt::Display for TypedCtk3TransformError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for TypedCtk3TransformError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::operation_rotations;
    use crate::{Ctk3Page, Ctk3PageFlags, Ctk3Rotation};

    fn source() -> String {
        let mut first = Ctk3Page::new(
            2,
            vec![
                Ctk3Color::Piece(Ctk3Piece::J),
                Ctk3Color::Empty,
                Ctk3Color::Empty,
                Ctk3Color::Piece(Ctk3Piece::S),
                Ctk3Color::Empty,
                Ctk3Color::Piece(Ctk3Piece::I),
                Ctk3Color::Empty,
                Ctk3Color::Empty,
            ],
        )
        .with_comment("first");
        first.operation = Some(Ctk3Operation {
            piece: Ctk3Piece::J,
            rotation: Ctk3Rotation::Right,
            x: 1,
            y: 1,
        });
        first.flags = Ctk3PageFlags {
            mirror: true,
            ..Ctk3PageFlags::default()
        };
        first.garbage = Some(vec![
            Ctk3Color::Piece(Ctk3Piece::L),
            Ctk3Color::Empty,
            Ctk3Color::Empty,
            Ctk3Color::Empty,
        ]);
        let second = Ctk3Page::new(
            1,
            vec![
                Ctk3Color::Empty,
                Ctk3Color::Piece(Ctk3Piece::T),
                Ctk3Color::Empty,
                Ctk3Color::Empty,
            ],
        )
        .with_comment("second");
        encode_ctk3(&Ctk3Document::new(4, vec![first, second])).unwrap()
    }

    #[test]
    fn typed_ctk3_roundtrip_combine_split_and_page_selection_are_lossless() {
        let source = source();
        assert_eq!(
            decode_ctk3_exact(&TypedCtk3DocumentTransform::roundtrip(&source).unwrap()),
            decode_ctk3_exact(&source)
        );
        let split = TypedCtk3DocumentTransform::split(&source).unwrap();
        assert_eq!(split.len(), 2);
        let combined = TypedCtk3DocumentTransform::combine(&split).unwrap();
        assert_eq!(decode_ctk3_exact(&combined), decode_ctk3_exact(&source));
        let selected = TypedCtk3DocumentTransform::get_page(&source, 1).unwrap();
        assert_eq!(
            decode_ctk3_exact(&selected).unwrap().pages[0].comment,
            "second"
        );
    }

    #[test]
    fn typed_ctk3_gray_preserves_operation_flags_comment_and_shape() {
        let source = source();
        let before = decode_ctk3_exact(&source).unwrap();
        let gray = TypedCtk3DocumentTransform::to_gray(&source).unwrap();
        let after = decode_ctk3_exact(&gray).unwrap();
        assert!(after.pages[0]
            .cells
            .iter()
            .all(|cell| matches!(cell, Ctk3Color::Empty | Ctk3Color::Gray)));
        assert_eq!(after.pages[0].operation, before.pages[0].operation);
        assert_eq!(after.pages[0].flags, before.pages[0].flags);
        assert_eq!(after.pages[0].comment, before.pages[0].comment);
        assert_eq!(after.pages[0].height, before.pages[0].height);
    }

    #[test]
    fn typed_ctk3_mirror_maps_colors_and_operations_and_is_an_involution() {
        let source = source();
        let once = TypedCtk3DocumentTransform::mirror(&source).unwrap();
        let mirrored = decode_ctk3_exact(&once).unwrap();
        assert_eq!(mirrored.pages[0].cells[3], Ctk3Color::Piece(Ctk3Piece::L));
        assert_eq!(mirrored.pages[0].cells[0], Ctk3Color::Piece(Ctk3Piece::Z));
        assert_eq!(
            mirrored.pages[0].garbage.as_ref().unwrap()[3],
            Ctk3Color::Piece(Ctk3Piece::J)
        );
        let twice = TypedCtk3DocumentTransform::mirror(&once).unwrap();
        assert_eq!(decode_ctk3_exact(&twice), decode_ctk3_exact(&source));
    }

    #[test]
    fn typed_ctk3_double_mirror_preserves_every_canonical_piece_rotation() {
        let mut pages = Vec::new();
        for piece in [
            Ctk3Piece::I,
            Ctk3Piece::O,
            Ctk3Piece::T,
            Ctk3Piece::S,
            Ctk3Piece::Z,
            Ctk3Piece::J,
            Ctk3Piece::L,
        ] {
            for rotation in operation_rotations(piece) {
                let mut page = Ctk3Page::new(8, vec![Ctk3Color::Empty; 80]);
                page.operation = Some(Ctk3Operation {
                    piece,
                    rotation: *rotation,
                    x: 4,
                    y: 4,
                });
                pages.push(page);
            }
        }
        let source = encode_ctk3(&Ctk3Document::new(10, pages)).unwrap();
        let once = TypedCtk3DocumentTransform::mirror(&source).unwrap();
        let twice = TypedCtk3DocumentTransform::mirror(&once).unwrap();
        assert_eq!(decode_ctk3_exact(&twice), decode_ctk3_exact(&source));
    }

    #[test]
    fn typed_ctk3_comment_and_page_order_transforms_preserve_other_fields() {
        let source = source();
        let shifted = TypedCtk3DocumentTransform::page_shift(&source, 1).unwrap();
        assert_eq!(
            decode_ctk3_exact(&shifted).unwrap().pages[0].comment,
            "second"
        );
        let cleaned = TypedCtk3DocumentTransform::clean_comments(&source).unwrap();
        assert!(decode_ctk3_exact(&cleaned)
            .unwrap()
            .pages
            .iter()
            .all(|page| page.comment.is_empty()));
    }
}
