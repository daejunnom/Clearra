#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SkinProvenance {
    BuiltIn,
    UserProvided { source: String },
    Generated { generator: String },
}
