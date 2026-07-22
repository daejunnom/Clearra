use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use serde_json::{Map, Number, Value};

use crate::profile::rule_profile::RuleProfileId;

use super::{
    kick_table::{
        KickOffset, KickOffsetSequence, KickTableEntry, KickTableProfile, KickTableProfileId,
        KickTransition,
    },
    kick_verification::KickProfileVerificationReport,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KickImport;

impl KickImport {
    pub fn from_json(input: &str) -> Result<KickTableProfile, KickImportError> {
        let raw = parse_raw_profile(input)?;
        let id = KickTableProfileId::parse(raw.id.as_str())
            .ok_or_else(|| KickImportError::UnknownProfileId(raw.id.clone()))?;
        let source_rule = RuleProfileId::parse(raw.source_rule.as_str())
            .ok_or_else(|| KickImportError::UnknownRuleProfileId(raw.source_rule.clone()))?;

        let entries = raw
            .entries
            .into_iter()
            .map(raw_entry_to_entry)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(KickTableProfile::new(id, source_rule, entries))
    }
}
impl KickImport {
    pub fn to_json(profile: &KickTableProfile) -> Result<String, KickImportError> {
        let raw = RawKickProfile {
            id: profile.id().as_str().to_owned(),
            source_rule: profile.source_rule().as_str().to_owned(),
            entries: profile
                .entries()
                .iter()
                .map(|entry| RawKickEntry {
                    piece: entry.transition().piece().as_ascii().to_string(),
                    from: rotation_to_key(entry.transition().from()).to_owned(),
                    to: rotation_to_key(entry.transition().to()).to_owned(),
                    offsets: entry
                        .sequence()
                        .offsets()
                        .iter()
                        .map(|offset| RawKickOffset {
                            dx: offset.dx(),
                            dy: offset.dy(),
                        })
                        .collect(),
                    unsupported: entry.unsupported_reason().map(str::to_owned),
                })
                .collect(),
        };
        serde_json::to_string_pretty(&raw_profile_to_value(raw))
            .map_err(|_| KickImportError::InvalidJson)
    }
}
impl KickImport {
    pub fn verify_imported_profile(profile: &KickTableProfile) -> KickProfileVerificationReport {
        KickProfileVerificationReport::verify_imported_profile(profile)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KickImportError {
    InvalidJson,
    UnknownProfileId(String),
    UnknownRuleProfileId(String),
    UnknownPiece(String),
    UnknownRotation(String),
    EmptyOffsetSequence,
}

impl KickImportError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidJson => "invalid_json",
            Self::UnknownProfileId(_) => "unknown_profile_id",
            Self::UnknownRuleProfileId(_) => "unknown_rule_profile_id",
            Self::UnknownPiece(_) => "unknown_piece",
            Self::UnknownRotation(_) => "unknown_rotation",
            Self::EmptyOffsetSequence => "empty_offset_sequence",
        }
    }
}

#[derive(Clone, Debug)]
struct RawKickProfile {
    id: String,
    source_rule: String,
    entries: Vec<RawKickEntry>,
}

#[derive(Clone, Debug)]
struct RawKickEntry {
    piece: String,
    from: String,
    to: String,
    offsets: Vec<RawKickOffset>,
    unsupported: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct RawKickOffset {
    dx: i8,
    dy: i8,
}

fn parse_raw_profile(input: &str) -> Result<RawKickProfile, KickImportError> {
    let value: Value = serde_json::from_str(input).map_err(|_| KickImportError::InvalidJson)?;
    let object = exact_object(&value, &["id", "source_rule", "entries"])?;
    let entries = required_array(object, "entries")?
        .iter()
        .map(parse_raw_entry)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RawKickProfile {
        id: required_string(object, "id")?.to_owned(),
        source_rule: required_string(object, "source_rule")?.to_owned(),
        entries,
    })
}

fn parse_raw_entry(value: &Value) -> Result<RawKickEntry, KickImportError> {
    let object = exact_object(value, &["piece", "from", "to", "offsets", "unsupported"])?;
    let offsets = required_array(object, "offsets")?
        .iter()
        .map(parse_raw_offset)
        .collect::<Result<Vec<_>, _>>()?;
    let unsupported = match object.get("unsupported") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return Err(KickImportError::InvalidJson),
    };
    Ok(RawKickEntry {
        piece: required_string(object, "piece")?.to_owned(),
        from: required_string(object, "from")?.to_owned(),
        to: required_string(object, "to")?.to_owned(),
        offsets,
        unsupported,
    })
}

fn parse_raw_offset(value: &Value) -> Result<RawKickOffset, KickImportError> {
    let object = exact_object(value, &["dx", "dy"])?;
    Ok(RawKickOffset {
        dx: required_i8(object, "dx")?,
        dy: required_i8(object, "dy")?,
    })
}

fn exact_object<'a>(
    value: &'a Value,
    allowed_fields: &[&str],
) -> Result<&'a Map<String, Value>, KickImportError> {
    let Value::Object(object) = value else {
        return Err(KickImportError::InvalidJson);
    };
    if object
        .keys()
        .any(|field| !allowed_fields.contains(&field.as_str()))
    {
        return Err(KickImportError::InvalidJson);
    }
    Ok(object)
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, KickImportError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or(KickImportError::InvalidJson)
}

fn required_array<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a [Value], KickImportError> {
    object
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or(KickImportError::InvalidJson)
}

fn required_i8(object: &Map<String, Value>, field: &str) -> Result<i8, KickImportError> {
    object
        .get(field)
        .and_then(Value::as_i64)
        .and_then(|value| i8::try_from(value).ok())
        .ok_or(KickImportError::InvalidJson)
}

fn raw_profile_to_value(profile: RawKickProfile) -> Value {
    let mut object = Map::new();
    object.insert("id".to_owned(), Value::String(profile.id));
    object.insert("source_rule".to_owned(), Value::String(profile.source_rule));
    object.insert(
        "entries".to_owned(),
        Value::Array(
            profile
                .entries
                .into_iter()
                .map(raw_entry_to_value)
                .collect(),
        ),
    );
    Value::Object(object)
}

fn raw_entry_to_value(entry: RawKickEntry) -> Value {
    let mut object = Map::new();
    object.insert("piece".to_owned(), Value::String(entry.piece));
    object.insert("from".to_owned(), Value::String(entry.from));
    object.insert("to".to_owned(), Value::String(entry.to));
    object.insert(
        "offsets".to_owned(),
        Value::Array(entry.offsets.into_iter().map(raw_offset_to_value).collect()),
    );
    if let Some(reason) = entry.unsupported {
        object.insert("unsupported".to_owned(), Value::String(reason));
    }
    Value::Object(object)
}

fn raw_offset_to_value(offset: RawKickOffset) -> Value {
    let mut object = Map::new();
    object.insert("dx".to_owned(), Value::Number(Number::from(offset.dx)));
    object.insert("dy".to_owned(), Value::Number(Number::from(offset.dy)));
    Value::Object(object)
}

fn raw_entry_to_entry(raw: RawKickEntry) -> Result<KickTableEntry, KickImportError> {
    if raw.offsets.is_empty() {
        return Err(KickImportError::EmptyOffsetSequence);
    }
    let piece = parse_piece(raw.piece.as_str())?;
    let from = parse_rotation(raw.from.as_str())?;
    let to = parse_rotation(raw.to.as_str())?;
    let sequence = KickOffsetSequence::new(
        raw.offsets
            .into_iter()
            .map(|offset| KickOffset::new(offset.dx, offset.dy))
            .collect(),
    );
    let entry = KickTableEntry::new(KickTransition::new(piece, from, to), sequence);
    Ok(match raw.unsupported {
        Some(reason) => entry.with_unsupported_reason(reason),
        None => entry,
    })
}

fn parse_piece(value: &str) -> Result<PieceKind, KickImportError> {
    let mut chars = value.chars();
    let Some(piece) = chars.next() else {
        return Err(KickImportError::UnknownPiece(value.to_owned()));
    };
    if chars.next().is_some() {
        return Err(KickImportError::UnknownPiece(value.to_owned()));
    }
    PieceKind::from_ascii(piece).map_err(|_| KickImportError::UnknownPiece(value.to_owned()))
}

fn parse_rotation(value: &str) -> Result<RotationState, KickImportError> {
    match value.to_ascii_lowercase().as_str() {
        "0" | "zero" | "spawn" => Ok(RotationState::Zero),
        "r" | "right" | "1" => Ok(RotationState::Right),
        "2" | "two" => Ok(RotationState::Two),
        "l" | "left" | "3" => Ok(RotationState::Left),
        _ => Err(KickImportError::UnknownRotation(value.to_owned())),
    }
}

fn rotation_to_key(rotation: RotationState) -> &'static str {
    match rotation {
        RotationState::Zero => "0",
        RotationState::Right => "R",
        RotationState::Two => "2",
        RotationState::Left => "L",
    }
}

#[cfg(test)]
#[path = "kick_import_tests.rs"]
mod tests;
