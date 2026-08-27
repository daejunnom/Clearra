//! Finite public DTOs for typed-document utility results.

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParityReportPagePayload {
    document_format: String,
    page_number: u32,
    total_pages: u32,
    coordinate_basis: String,
    width: u16,
    height: u16,
    occupied_cell_count: u64,
    checker_black_count: u64,
    checker_white_count: u64,
    checker_delta: i64,
    four_color_counts: [u64; 4],
    even_column_count: u64,
    odd_column_count: u64,
    column_parity_delta: i64,
    occupied_area_mod_four: u8,
    pending_garbage_occupied_cell_count: u16,
    feasibility_claim: bool,
    pruning_authority: String,
    page_handle_available: bool,
}

impl ParityReportPagePayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        document_format: impl Into<String>,
        page_number: u32,
        total_pages: u32,
        coordinate_basis: impl Into<String>,
        width: u16,
        height: u16,
        occupied_cell_count: u64,
        checker_black_count: u64,
        checker_white_count: u64,
        checker_delta: i64,
        four_color_counts: [u64; 4],
        even_column_count: u64,
        odd_column_count: u64,
        column_parity_delta: i64,
        occupied_area_mod_four: u8,
        pending_garbage_occupied_cell_count: u16,
        feasibility_claim: bool,
        pruning_authority: impl Into<String>,
        page_handle_available: bool,
    ) -> Self {
        Self {
            document_format: document_format.into(),
            page_number,
            total_pages,
            coordinate_basis: coordinate_basis.into(),
            width,
            height,
            occupied_cell_count,
            checker_black_count,
            checker_white_count,
            checker_delta,
            four_color_counts,
            even_column_count,
            odd_column_count,
            column_parity_delta,
            occupied_area_mod_four,
            pending_garbage_occupied_cell_count,
            feasibility_claim,
            pruning_authority: pruning_authority.into(),
            page_handle_available,
        }
    }

    pub fn document_format(&self) -> &str {
        &self.document_format
    }
    pub const fn page_number(&self) -> u32 {
        self.page_number
    }
    pub const fn total_pages(&self) -> u32 {
        self.total_pages
    }
    pub fn coordinate_basis(&self) -> &str {
        &self.coordinate_basis
    }
    pub const fn width(&self) -> u16 {
        self.width
    }
    pub const fn height(&self) -> u16 {
        self.height
    }
    pub const fn occupied_cell_count(&self) -> u64 {
        self.occupied_cell_count
    }
    pub const fn checker_black_count(&self) -> u64 {
        self.checker_black_count
    }
    pub const fn checker_white_count(&self) -> u64 {
        self.checker_white_count
    }
    pub const fn checker_delta(&self) -> i64 {
        self.checker_delta
    }
    pub const fn four_color_counts(&self) -> [u64; 4] {
        self.four_color_counts
    }
    pub const fn even_column_count(&self) -> u64 {
        self.even_column_count
    }
    pub const fn odd_column_count(&self) -> u64 {
        self.odd_column_count
    }
    pub const fn column_parity_delta(&self) -> i64 {
        self.column_parity_delta
    }
    pub const fn occupied_area_mod_four(&self) -> u8 {
        self.occupied_area_mod_four
    }
    pub const fn pending_garbage_occupied_cell_count(&self) -> u16 {
        self.pending_garbage_occupied_cell_count
    }
    pub const fn feasibility_claim(&self) -> bool {
        self.feasibility_claim
    }
    pub fn pruning_authority(&self) -> &str {
        &self.pruning_authority
    }
    pub const fn page_handle_available(&self) -> bool {
        self.page_handle_available
    }

    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        (self.document_format.capacity() as u128)
            .checked_add(self.coordinate_basis.capacity() as u128)?
            .checked_add(self.pruning_authority.capacity() as u128)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FieldDocumentPayload {
    format: String,
    document: String,
    page_count: u32,
    canonical_sha256: String,
    filename: String,
}

impl FieldDocumentPayload {
    pub fn new(
        format: impl Into<String>,
        document: impl Into<String>,
        page_count: u32,
        canonical_sha256: impl Into<String>,
        filename: impl Into<String>,
    ) -> Self {
        Self {
            format: format.into(),
            document: document.into(),
            page_count,
            canonical_sha256: canonical_sha256.into(),
            filename: filename.into(),
        }
    }
    pub fn format(&self) -> &str {
        &self.format
    }
    pub fn document(&self) -> &str {
        &self.document
    }
    pub const fn page_count(&self) -> u32 {
        self.page_count
    }
    pub fn canonical_sha256(&self) -> &str {
        &self.canonical_sha256
    }
    pub fn filename(&self) -> &str {
        &self.filename
    }
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        [
            self.format.capacity(),
            self.document.capacity(),
            self.canonical_sha256.capacity(),
            self.filename.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FieldDocumentSetPayload {
    document_contract: String,
    documents: Vec<FieldDocumentPayload>,
}

impl FieldDocumentSetPayload {
    pub fn new(document_contract: impl Into<String>, documents: Vec<FieldDocumentPayload>) -> Self {
        Self {
            document_contract: document_contract.into(),
            documents,
        }
    }
    pub fn document_contract(&self) -> &str {
        &self.document_contract
    }
    pub fn documents(&self) -> &[FieldDocumentPayload] {
        &self.documents
    }
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.document_contract.capacity() as u128).checked_add(
            (self.documents.capacity() as u128)
                .checked_mul(core::mem::size_of::<FieldDocumentPayload>() as u128)?,
        )?;
        for document in &self.documents {
            bytes = bytes.checked_add(document.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RenderArtifactPayload {
    document_format: String,
    artifact_format: String,
    selected_page_number: Option<u32>,
    document_page_count: u32,
    media_type: String,
    filename: String,
    byte_length: u64,
    sha256: String,
    bytes_base64: String,
    render_exact: bool,
    skin_id: String,
    product_max_bytes: u64,
    transport_max_bytes: u64,
}

impl RenderArtifactPayload {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        document_format: impl Into<String>,
        artifact_format: impl Into<String>,
        selected_page_number: Option<u32>,
        document_page_count: u32,
        media_type: impl Into<String>,
        filename: impl Into<String>,
        byte_length: u64,
        sha256: impl Into<String>,
        bytes_base64: impl Into<String>,
        render_exact: bool,
        skin_id: impl Into<String>,
        product_max_bytes: u64,
        transport_max_bytes: u64,
    ) -> Self {
        Self {
            document_format: document_format.into(),
            artifact_format: artifact_format.into(),
            selected_page_number,
            document_page_count,
            media_type: media_type.into(),
            filename: filename.into(),
            byte_length,
            sha256: sha256.into(),
            bytes_base64: bytes_base64.into(),
            render_exact,
            skin_id: skin_id.into(),
            product_max_bytes,
            transport_max_bytes,
        }
    }
    pub fn document_format(&self) -> &str {
        &self.document_format
    }
    pub fn artifact_format(&self) -> &str {
        &self.artifact_format
    }
    pub const fn selected_page_number(&self) -> Option<u32> {
        self.selected_page_number
    }
    pub const fn document_page_count(&self) -> u32 {
        self.document_page_count
    }
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
    pub fn filename(&self) -> &str {
        &self.filename
    }
    pub const fn byte_length(&self) -> u64 {
        self.byte_length
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn bytes_base64(&self) -> &str {
        &self.bytes_base64
    }
    pub const fn render_exact(&self) -> bool {
        self.render_exact
    }
    pub fn skin_id(&self) -> &str {
        &self.skin_id
    }
    pub const fn product_max_bytes(&self) -> u64 {
        self.product_max_bytes
    }
    pub const fn transport_max_bytes(&self) -> u64 {
        self.transport_max_bytes
    }
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        [
            self.document_format.capacity(),
            self.artifact_format.capacity(),
            self.media_type.capacity(),
            self.filename.capacity(),
            self.sha256.capacity(),
            self.bytes_base64.capacity(),
            self.skin_id.capacity(),
        ]
        .into_iter()
        .try_fold(0_u128, |total, capacity| {
            total.checked_add(capacity as u128)
        })
    }
}
