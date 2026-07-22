use clearra_output::{
    model::{RenderField, RenderMessage},
    RenderFormat, RenderFormatDispatcher,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommandRenderer;

impl CommandRenderer {
    pub fn render<I>(kind: impl Into<String>, fields: I, format: RenderFormat) -> String
    where
        I: IntoIterator<Item = RenderField>,
    {
        let mut message = RenderMessage::new(kind);
        for field in fields {
            message = message.with_value(field.key().to_owned(), field.value().clone());
        }
        RenderFormatDispatcher::render(&message, format)
    }
}

#[cfg(test)]
#[path = "command_renderer_tests.rs"]
mod tests;
