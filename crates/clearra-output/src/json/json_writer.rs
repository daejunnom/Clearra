use super::json_contract::{JsonContract, JsonMember, JsonValue};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JsonWriter;

impl JsonWriter {
    pub fn write(contract: &JsonContract) -> String {
        write_value(&contract.root())
    }
}
impl JsonWriter {
    #[cfg(test)]
    pub(crate) fn write_value_for_test(value: &JsonValue) -> String {
        write_value(value)
    }
}
impl JsonWriter {
    pub fn write_replay_trace(trace: &clearra_replay::ReplayTrace) -> String {
        Self::write(&JsonContract::from_replay_trace(trace))
    }
}

fn write_value(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".to_owned(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.clone(),
        JsonValue::String(value) => format!("\"{}\"", escape_json(value)),
        JsonValue::Object(members) => write_object(members),
        JsonValue::Array(values) => {
            let values = values.iter().map(write_value).collect::<Vec<_>>().join(",");
            format!("[{values}]")
        }
    }
}

fn write_object(members: &[JsonMember]) -> String {
    let fields = members
        .iter()
        .map(|member| {
            format!(
                "\"{}\":{}",
                escape_json(member.key()),
                write_value(member.value())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\u{08}', "\\b")
        .replace('\u{0c}', "\\f")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(test)]
#[path = "json_writer_tests.rs"]
mod tests;
