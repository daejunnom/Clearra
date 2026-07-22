#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildFieldSchema {
    id: String,
    label: String,
    field_type: BuildFieldType,
    required: bool,
    options: Vec<String>,
}

impl BuildFieldSchema {
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        field_type: BuildFieldType,
        required: bool,
        options: Vec<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            field_type,
            required,
            options,
        }
    }
}
impl BuildFieldSchema {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl BuildFieldSchema {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl BuildFieldSchema {
    pub fn field_type(&self) -> BuildFieldType {
        self.field_type
    }
}
impl BuildFieldSchema {
    pub fn is_required(&self) -> bool {
        self.required
    }
}
impl BuildFieldSchema {
    pub fn options(&self) -> &[String] {
        &self.options
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildFieldType {
    Text,
    Number,
    PieceMultiSelect,
    PieceSelect,
    Select,
    CellList,
}
