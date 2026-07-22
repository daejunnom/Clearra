use clearra_pc_graph::request::PcScenarioQuery;

use crate::fixture::PcScenarioFixture;

#[derive(Debug)]
pub struct PcScenarioAssembly {
    query: PcScenarioQuery,
    fixture: Option<PcScenarioFixture>,
    fixture_path: Option<String>,
    input_fields: Vec<(String, String)>,
}

impl PcScenarioAssembly {
    pub(super) fn inline(query: PcScenarioQuery) -> Self {
        Self {
            query,
            fixture: None,
            fixture_path: None,
            input_fields: vec![("input_mode".to_owned(), "inline".to_owned())],
        }
    }
}
impl PcScenarioAssembly {
    pub(super) fn from_fixture(
        query: PcScenarioQuery,
        fixture: PcScenarioFixture,
        fixture_path: String,
        input_fields: Vec<(String, String)>,
    ) -> Self {
        Self {
            query,
            fixture: Some(fixture),
            fixture_path: Some(fixture_path),
            input_fields,
        }
    }
}
impl PcScenarioAssembly {
    pub fn query(&self) -> &PcScenarioQuery {
        &self.query
    }
}
impl PcScenarioAssembly {
    pub fn fixture(&self) -> Option<&PcScenarioFixture> {
        self.fixture.as_ref()
    }
}
impl PcScenarioAssembly {
    pub fn fixture_path(&self) -> Option<&str> {
        self.fixture_path.as_deref()
    }
}
impl PcScenarioAssembly {
    pub fn input_fields(&self) -> Vec<(String, String)> {
        self.input_fields.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PcScenarioQueryAssemblyError {
    InvalidFixture { path: String, message: String },
    InvalidInline { message: String },
}

impl PcScenarioQueryAssemblyError {
    pub fn message(&self) -> String {
        match self {
            Self::InvalidFixture { path, message } => {
                format!("invalid scenario fixture '{path}': {message}")
            }
            Self::InvalidInline { message } => {
                format!("invalid inline pc-scenario input: {message}")
            }
        }
    }
}
