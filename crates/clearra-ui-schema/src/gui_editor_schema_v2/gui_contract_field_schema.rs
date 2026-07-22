use clearra_i18n::TranslationKey;

use crate::LocalizedLabelSchema;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiContractFieldSchema {
    contract_key: &'static str,
    localized_label: LocalizedLabelSchema,
    required_reason: &'static str,
}

impl GuiContractFieldSchema {
    pub fn new(
        contract_key: &'static str,
        fallback_label: &'static str,
        required_reason: &'static str,
    ) -> Self {
        Self {
            contract_key,
            localized_label: LocalizedLabelSchema::new(
                TranslationKey::new(format!("ui.gui.v2.field.{contract_key}")),
                fallback_label,
            ),
            required_reason,
        }
    }
}
impl GuiContractFieldSchema {
    pub const fn contract_key(&self) -> &'static str {
        self.contract_key
    }
}
impl GuiContractFieldSchema {
    pub fn localized_label(&self) -> &LocalizedLabelSchema {
        &self.localized_label
    }
}
impl GuiContractFieldSchema {
    pub const fn required_reason(&self) -> &'static str {
        self.required_reason
    }
}
