#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppInputSource {
    Inline,
    File { display_path: String },
}
