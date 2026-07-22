use std::path::Path;

use crate::input::file_input_guard::read_json_file;

pub(super) fn read_fixture_json(path: &Path) -> Result<String, String> {
    read_json_file(path).map_err(|error| error.to_string())
}
