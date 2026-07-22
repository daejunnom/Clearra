#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReportCsvWriter;

impl ReportCsvWriter {
    pub fn write_rows(headers: &[&str], rows: &[Vec<String>]) -> String {
        let mut output = String::new();
        output.push_str(&headers.join(","));
        for row in rows {
            output.push('\n');
            output.push_str(
                &row.iter()
                    .map(|cell| escape_csv(cell))
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        output
    }
}

fn escape_csv(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
