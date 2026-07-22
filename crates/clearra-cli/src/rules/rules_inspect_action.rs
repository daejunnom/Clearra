use clearra_rules::{
    kicks::KickProfileRegistry,
    profile::{rule_capability::RuleCapability, rule_profile::RuleProfile},
};

use crate::{
    args::RulesArgs,
    error::CliErrorCode,
    output::{CliOutput, RenderFormat},
    rules::rules_output_fields::render_rules,
};

pub(crate) struct RulesInspectAction;

impl RulesInspectAction {
    pub(crate) fn run(args: &RulesArgs, format: RenderFormat) -> CliOutput {
        let Some(profile_id) = args.profile() else {
            return CliOutput::error(
                CliErrorCode::RulesProfileUnknown,
                "rules inspect requires --profile <id>",
            );
        };
        let Some(descriptor) =
            KickProfileRegistry::builtin_profiles()
                .into_iter()
                .find(|descriptor| {
                    descriptor.rule_profile_id().as_str() == profile_id
                        || descriptor.id().as_str() == profile_id
                })
        else {
            return CliOutput::error(
                CliErrorCode::RulesProfileUnknown,
                format!("unknown rule profile '{profile_id}'"),
            );
        };
        let capability = RuleCapability::from_rule(RuleProfile::new(descriptor.rule_profile_id()));
        let kick_capability = descriptor.capability();
        let mut fields = vec![
            ("action".to_owned(), "inspect".to_owned()),
            (
                "rule_profile".to_owned(),
                descriptor.rule_profile_id().as_str().to_owned(),
            ),
            (
                "kick_profile".to_owned(),
                descriptor.id().as_str().to_owned(),
            ),
            ("label".to_owned(), descriptor.label().to_owned()),
            (
                "source_kind".to_owned(),
                descriptor.source_kind().as_str().to_owned(),
            ),
            (
                "source_description".to_owned(),
                descriptor.source_description().to_owned(),
            ),
            (
                "effective_kick_model".to_owned(),
                capability.kick_model().as_str().to_owned(),
            ),
            (
                "supports_180".to_owned(),
                capability.supports_180().to_string(),
            ),
            (
                "supports_exact_180".to_owned(),
                kick_capability.supports_exact_180().to_string(),
            ),
            (
                "requires_lock_reachability".to_owned(),
                capability.requires_lock_reachability().to_string(),
            ),
            (
                "requires_spawn_reachability".to_owned(),
                capability.requires_spawn_reachability().to_string(),
            ),
            (
                "search_backend_supported".to_owned(),
                capability.search_backend_supported().to_string(),
            ),
            (
                "c_compact_descriptor_ready".to_owned(),
                kick_capability.c_compact_descriptor_ready().to_string(),
            ),
            (
                "unsupported_backend_reason".to_owned(),
                kick_capability
                    .unsupported_reason()
                    .unwrap_or("none")
                    .to_owned(),
            ),
        ];
        if let Some(reason) = capability.unsupported_reason() {
            fields.push(("unsupported_reason".to_owned(), reason.to_owned()));
        }

        render_rules(fields, format)
    }
}
