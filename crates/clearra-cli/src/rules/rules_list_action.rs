use clearra_rules::kicks::KickProfileRegistry;

use crate::{
    output::{CliOutput, RenderFormat},
    rules::rules_output_fields::{capability_fields, render_rules},
};

pub(crate) struct RulesListAction;

impl RulesListAction {
    pub(crate) fn run(format: RenderFormat) -> CliOutput {
        let mut fields = vec![("action".to_owned(), "list".to_owned())];
        let profiles = KickProfileRegistry::builtin_profiles();
        fields.push(("profile_count".to_owned(), profiles.len().to_string()));
        for (index, descriptor) in profiles.iter().enumerate() {
            let prefix = format!("profile_{index}_");
            fields.push((
                format!("{prefix}id"),
                descriptor.rule_profile_id().as_str().to_owned(),
            ));
            fields.push((
                format!("{prefix}kick_profile"),
                descriptor.id().as_str().to_owned(),
            ));
            fields.push((format!("{prefix}label"), descriptor.label().to_owned()));
            fields.push((
                format!("{prefix}source_kind"),
                descriptor.source_kind().as_str().to_owned(),
            ));
            fields.push((
                format!("{prefix}source_description"),
                descriptor.source_description().to_owned(),
            ));
            fields.extend(capability_fields(&prefix, descriptor.capability()));
        }

        render_rules(fields, format)
    }
}
