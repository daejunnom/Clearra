use super::GpuPackingCandidate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GpuReadbackCompressionError {
    Truncated,
    InvalidBoolean,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuReadbackCompression;

impl GpuReadbackCompression {
    pub fn compress(candidates: &[GpuPackingCandidate]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(4 + candidates.len() * 57);
        bytes.extend_from_slice(&(candidates.len() as u32).to_le_bytes());
        for candidate in candidates {
            bytes.extend_from_slice(&candidate.candidate_id().to_le_bytes());
            bytes.extend_from_slice(&candidate.shape_key().to_le_bytes());
            bytes.extend_from_slice(&candidate.tiling_key().to_le_bytes());
            bytes.extend_from_slice(&candidate.operation_set_key().to_le_bytes());
            bytes.extend_from_slice(&candidate.final_board_mask().to_le_bytes());
            bytes.extend_from_slice(&candidate.coverage_bits().to_le_bytes());
            bytes.push(u8::from(candidate.required_candidate()));
        }
        bytes
    }
}
impl GpuReadbackCompression {
    pub fn decompress(
        bytes: &[u8],
    ) -> Result<Vec<GpuPackingCandidate>, GpuReadbackCompressionError> {
        let mut cursor = Cursor::new(bytes);
        let count = cursor.read_u32()? as usize;
        let mut candidates = Vec::with_capacity(count);

        for _ in 0..count {
            let candidate_id = cursor.read_u64()?;
            let shape_key = cursor.read_u64()?;
            let tiling_key = cursor.read_u64()?;
            let operation_set_key = cursor.read_u64()?;
            let final_board_mask = cursor.read_u64()?;
            let coverage_bits = cursor.read_u128()?;
            let required_candidate = match cursor.read_u8()? {
                0 => false,
                1 => true,
                _ => return Err(GpuReadbackCompressionError::InvalidBoolean),
            };
            candidates.push(GpuPackingCandidate::new(
                candidate_id,
                shape_key,
                tiling_key,
                operation_set_key,
                final_board_mask,
                coverage_bits,
                required_candidate,
            ));
        }

        Ok(candidates)
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
}
impl<'a> Cursor<'a> {
    fn read_u8(&mut self) -> Result<u8, GpuReadbackCompressionError> {
        let value = *self
            .bytes
            .get(self.offset)
            .ok_or(GpuReadbackCompressionError::Truncated)?;
        self.offset += 1;
        Ok(value)
    }
}
impl<'a> Cursor<'a> {
    fn read_u32(&mut self) -> Result<u32, GpuReadbackCompressionError> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }
}
impl<'a> Cursor<'a> {
    fn read_u64(&mut self) -> Result<u64, GpuReadbackCompressionError> {
        Ok(u64::from_le_bytes(self.read_array()?))
    }
}
impl<'a> Cursor<'a> {
    fn read_u128(&mut self) -> Result<u128, GpuReadbackCompressionError> {
        Ok(u128::from_le_bytes(self.read_array()?))
    }
}
impl<'a> Cursor<'a> {
    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], GpuReadbackCompressionError> {
        let end = self.offset + N;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(GpuReadbackCompressionError::Truncated)?;
        self.offset = end;
        Ok(slice.try_into().expect("slice length matches array"))
    }
}
