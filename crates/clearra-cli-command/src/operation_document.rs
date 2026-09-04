use clearra_app::{
    decode_ctk3_exact, ConcreteDocumentOperation, Ctk3Color, Ctk3Piece, Ctk3Rotation,
    OperationDocumentProblem,
};
use clearra_core_domain::{
    operation::operation::OperationId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_fumen::SourceFumenOperationDocument;
use clearra_rules::{kicks::KickTableProfileId, profile::rule_profile::RuleProfileId};

use crate::{WebCommandError, WebCommandErrorCode, WebCommandRequest};

pub fn sequence_dependencies_request_from_document(
    source: &str,
    rule_profile: Option<&str>,
    kick_profile: Option<&str>,
    timeout_seconds: Option<u16>,
) -> Result<WebCommandRequest, WebCommandError> {
    let problem = problem_from_document(
        source,
        rule_profile,
        kick_profile,
        timeout_seconds,
        "sequence-dependencies",
    )?;
    Ok(WebCommandRequest::sequence_dependencies(problem))
}

pub fn operation_sequence_request_from_document(
    source: &str,
    rule_profile: Option<&str>,
    kick_profile: Option<&str>,
    timeout_seconds: Option<u16>,
) -> Result<WebCommandRequest, WebCommandError> {
    let problem = problem_from_document(
        source,
        rule_profile,
        kick_profile,
        timeout_seconds,
        "sequence",
    )?;
    Ok(WebCommandRequest::operation_sequence(problem))
}

fn problem_from_document(
    source: &str,
    rule_profile: Option<&str>,
    kick_profile: Option<&str>,
    timeout_seconds: Option<u16>,
    command_name: &'static str,
) -> Result<OperationDocumentProblem, WebCommandError> {
    let rule_profile = rule_profile
        .map_or(Some(RuleProfileId::SrsPlus), RuleProfileId::parse)
        .ok_or_else(|| invalid(format!("unsupported {command_name} rule profile")))?;
    let kick_profile = kick_profile
        .map_or(Some(KickTableProfileId::SrsPlus), KickTableProfileId::parse)
        .ok_or_else(|| invalid(format!("unsupported {command_name} kick profile")))?;
    let timeout_seconds = timeout_seconds.unwrap_or(900);
    if !(1..=900).contains(&timeout_seconds) {
        return Err(invalid(format!(
            "{command_name} timeout-seconds must be in 1..=900"
        )));
    }
    let mut problem = if source.trim_start().starts_with("ctk3_")
        || source.trim_start().starts_with("ctk3@")
        || source.trim_start().starts_with("ctk3b_")
    {
        decode_ctk3_problem(source)?
    } else {
        decode_fumen_problem(source)?
    };
    problem.rule_profile = rule_profile;
    problem.kick_profile = kick_profile;
    problem.timeout_seconds = timeout_seconds;
    Ok(problem)
}

fn decode_ctk3_problem(source: &str) -> Result<OperationDocumentProblem, WebCommandError> {
    let document = decode_ctk3_exact(source)
        .map_err(|error| invalid(format!("invalid CTK3 operation document: {error}")))?;
    if document.width == 0 || document.pages.is_empty() {
        return Err(invalid("CTK3 operation document is empty"));
    }
    let width = u8::try_from(document.width).map_err(|_| invalid("CTK3 width exceeds Board64"))?;
    let height =
        u8::try_from(64 / document.width).map_err(|_| invalid("CTK3 height exceeds Board64"))?;
    let mut operations = Vec::new();
    let mut boards = Vec::new();
    operations
        .try_reserve(document.pages.len())
        .map_err(|_| invalid("CTK3 operation capacity exceeded"))?;
    boards
        .try_reserve(document.pages.len())
        .map_err(|_| invalid("CTK3 board capacity exceeded"))?;
    for (page_index, page) in document.pages.iter().enumerate() {
        if page.height > usize::from(height)
            || !page.flags.lock
            || page.flags.mirror
            || page.flags.rise
            || page.flags.quiz
            || page
                .garbage
                .as_ref()
                .is_some_and(|row| row.iter().any(|cell| *cell != Ctk3Color::Empty))
        {
            return Err(invalid(format!(
                "CTK3 page {page_index} has unsupported operation-document semantics"
            )));
        }
        let operation = page
            .operation
            .ok_or_else(|| invalid(format!("CTK3 page {page_index} has no concrete operation")))?;
        let board = page
            .cells
            .iter()
            .enumerate()
            .fold(0_u64, |mask, (index, cell)| {
                if *cell == Ctk3Color::Empty {
                    mask
                } else {
                    mask | (1_u64 << index)
                }
            });
        let operation_id = OperationId(
            u16::try_from(page_index).map_err(|_| invalid("operation id exceeds u16"))?,
        );
        let concrete = ConcreteDocumentOperation::from_centered(
            operation_id,
            piece(operation.piece),
            rotation(operation.rotation),
            operation.x,
            operation.y,
        )
        .ok_or_else(|| {
            invalid(format!(
                "CTK3 page {page_index} loses concrete operation coordinates"
            ))
        })?;
        boards.push(board);
        operations.push(concrete);
    }
    let initial_board = boards[0];
    let mut problem = OperationDocumentProblem::canonical(width, height, initial_board, operations);
    problem.document_boards = Some(boards);
    Ok(problem)
}

fn decode_fumen_problem(source: &str) -> Result<OperationDocumentProblem, WebCommandError> {
    let document = SourceFumenOperationDocument::decode(source)
        .map_err(|error| invalid(format!("invalid Fumen operation document: {error}")))?;
    let mut operations = Vec::new();
    let mut boards = Vec::new();
    for (page_index, operation) in document.operations.iter().enumerate() {
        let operation_id = OperationId(
            u16::try_from(page_index).map_err(|_| invalid("operation id exceeds u16"))?,
        );
        operations.push(
            ConcreteDocumentOperation::from_centered(
                operation_id,
                operation.piece,
                operation.rotation,
                operation.x,
                operation.y,
            )
            .ok_or_else(|| {
                invalid(format!(
                    "Fumen page {page_index} loses concrete operation coordinates"
                ))
            })?,
        );
        boards.push(operation.board_before);
    }
    let initial_board = *boards
        .first()
        .ok_or_else(|| invalid("Fumen has no concrete operation multiset"))?;
    let mut problem = OperationDocumentProblem::canonical(
        document.width,
        document.height,
        initial_board,
        operations,
    );
    problem.document_boards = Some(boards);
    Ok(problem)
}

const fn piece(value: Ctk3Piece) -> PieceKind {
    match value {
        Ctk3Piece::I => PieceKind::I,
        Ctk3Piece::O => PieceKind::O,
        Ctk3Piece::T => PieceKind::T,
        Ctk3Piece::S => PieceKind::S,
        Ctk3Piece::Z => PieceKind::Z,
        Ctk3Piece::J => PieceKind::J,
        Ctk3Piece::L => PieceKind::L,
    }
}
const fn rotation(value: Ctk3Rotation) -> RotationState {
    match value {
        Ctk3Rotation::Spawn => RotationState::Zero,
        Ctk3Rotation::Right => RotationState::Right,
        Ctk3Rotation::Reverse => RotationState::Two,
        Ctk3Rotation::Left => RotationState::Left,
    }
}
fn invalid(message: impl Into<String>) -> WebCommandError {
    WebCommandError::new(WebCommandErrorCode::InvalidValue, message)
}

#[cfg(test)]
mod tests {
    use clearra_app::{encode_ctk3_compact, Ctk3Document, Ctk3Operation, Ctk3Page, Ctk3PageFlags};

    use super::*;

    #[test]
    fn ctk3_document_lowers_without_queue_or_hold() {
        let mut page = Ctk3Page::new(0, Vec::new());
        page.flags = Ctk3PageFlags::default();
        page.operation = Some(Ctk3Operation {
            piece: Ctk3Piece::O,
            rotation: Ctk3Rotation::Spawn,
            x: 0,
            y: 0,
        });
        let encoded = encode_ctk3_compact(&Ctk3Document::new(10, vec![page])).unwrap();
        let request =
            sequence_dependencies_request_from_document(&encoded, None, None, None).unwrap();
        assert_eq!(request.command_kind(), "utility-sequence-dependencies");
        assert!(matches!(
            request.to_app_request().unwrap().command(),
            clearra_app::AppCommand::UtilitySequenceDependencies(_)
        ));
    }

    #[test]
    fn same_operation_document_lowers_to_sequence_replay() {
        let mut page = Ctk3Page::new(0, Vec::new());
        page.flags = Ctk3PageFlags::default();
        page.operation = Some(Ctk3Operation {
            piece: Ctk3Piece::O,
            rotation: Ctk3Rotation::Spawn,
            x: 0,
            y: 0,
        });
        let encoded = encode_ctk3_compact(&Ctk3Document::new(10, vec![page])).unwrap();
        let request = operation_sequence_request_from_document(&encoded, None, None, None).unwrap();
        assert_eq!(request.command_kind(), "utility-sequence");
        assert!(matches!(
            request.to_app_request().unwrap().command(),
            clearra_app::AppCommand::UtilitySequence(_)
        ));

        let parsed = crate::WebCommandParser::parse(&format!(
            "clearra utility sequence --document {encoded} --rule-profile srs-plus --kick-profile srs-plus --timeout-seconds 900"
        ))
        .unwrap();
        assert!(matches!(
            parsed.to_app_request().unwrap().command(),
            clearra_app::AppCommand::UtilitySequence(_)
        ));
    }
}
