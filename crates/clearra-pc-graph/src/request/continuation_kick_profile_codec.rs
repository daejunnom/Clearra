use clearra_rules::kicks::{KickImport, VerifiedKickTableProfile};

use super::{
    continuation_token_error::PcContinuationTokenError, continuation_token_segments::prefixed_value,
};

pub(crate) fn format_kick_profile(
    profile: Option<&VerifiedKickTableProfile>,
) -> Result<String, PcContinuationTokenError> {
    let Some(profile) = profile else {
        return Ok("none".to_owned());
    };
    let json = KickImport::to_json(profile.profile()).map_err(|error| {
        PcContinuationTokenError::new(format!(
            "scenario continuation token could not export verified kick profile: {}",
            error.code()
        ))
    })?;
    Ok(hex_encode(json.as_bytes()))
}

pub(crate) fn parse_kick_profile(
    part: &str,
) -> Result<Option<VerifiedKickTableProfile>, PcContinuationTokenError> {
    let value = prefixed_value(part, "k")?;
    if value == "none" {
        return Ok(None);
    }
    let bytes = hex_decode(value)?;
    let json = String::from_utf8(bytes).map_err(|_| {
        PcContinuationTokenError::new("scenario continuation kick profile is not utf-8")
    })?;
    let profile = KickImport::from_json(&json).map_err(|error| {
        PcContinuationTokenError::new(format!(
            "scenario continuation kick profile import failed: {}",
            error.code()
        ))
    })?;
    VerifiedKickTableProfile::try_new(profile)
        .map(Some)
        .map_err(|report| {
            PcContinuationTokenError::new(format!(
                "scenario continuation kick profile is not verified: issue_count={}",
                report.issue_count()
            ))
        })
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(crate) fn hex_decode(value: &str) -> Result<Vec<u8>, PcContinuationTokenError> {
    if !value.len().is_multiple_of(2) {
        return Err(PcContinuationTokenError::new(
            "scenario continuation kick profile hex has odd length",
        ));
    }
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(byte: u8) -> Result<u8, PcContinuationTokenError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(PcContinuationTokenError::new(
            "scenario continuation kick profile hex contains invalid characters",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{hex_decode, hex_encode};

    #[test]
    fn fixed_pairs_preserve_empty_all_bytes_and_uppercase_compatibility() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(hex_decode(&hex_encode(&bytes)).unwrap(), bytes);
        assert_eq!(
            hex_decode(&hex_encode(&bytes).to_uppercase()).unwrap(),
            bytes
        );
        assert!(hex_decode("").unwrap().is_empty());
    }

    #[test]
    fn fixed_pairs_never_ignore_a_tail_or_invalid_character() {
        for value in ["0", "001", "xz", "é", "🙂"] {
            assert!(hex_decode(value).is_err(), "{value}");
        }
    }
}
