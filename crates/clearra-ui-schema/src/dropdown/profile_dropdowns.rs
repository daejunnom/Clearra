use clearra_profiles::bundle::standard_profile_bundle::standard_profile_bundle;
use clearra_rules::{kicks::KickProfileRegistry, profile::builtin_rules::custom_rule};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use super::dropdown_option::DropdownOption;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileDropdowns {
    boards: Vec<DropdownOption>,
    piece_sets: Vec<DropdownOption>,
    bags: Vec<DropdownOption>,
    rules: Vec<DropdownOption>,
}

impl ProfileDropdowns {
    pub fn standard_mvp() -> Self {
        let profiles = standard_profile_bundle();
        Self {
            boards: vec![DropdownOption::new(
                profiles.board().id().as_str(),
                "Standard 10",
            )],
            piece_sets: vec![DropdownOption::new(
                profiles.piece_set().id().as_str(),
                "Standard tetrominoes",
            )],
            bags: vec![DropdownOption::new(
                profiles.bag().id().as_str(),
                "Standard 7-bag",
            )],
            rules: rule_options(),
        }
    }
}
impl ProfileDropdowns {
    pub fn boards(&self) -> &[DropdownOption] {
        &self.boards
    }
}
impl ProfileDropdowns {
    pub fn piece_sets(&self) -> &[DropdownOption] {
        &self.piece_sets
    }
}
impl ProfileDropdowns {
    pub fn bags(&self) -> &[DropdownOption] {
        &self.bags
    }
}
impl ProfileDropdowns {
    pub fn rules(&self) -> &[DropdownOption] {
        &self.rules
    }
}

impl Default for ProfileDropdowns {
    fn default() -> Self {
        Self::standard_mvp()
    }
}

fn rule_options() -> Vec<DropdownOption> {
    let mut rules = KickProfileRegistry::builtin_profiles()
        .into_iter()
        .map(|descriptor| {
            let option =
                DropdownOption::new(descriptor.rule_profile_id().as_str(), descriptor.label());
            match descriptor.capability().unsupported_reason() {
                Some(reason) => option.disabled_for(DiagnosticCode::ERuleUnsupportedMvp, reason),
                None => option,
            }
        })
        .collect::<Vec<_>>();
    rules.push(
        DropdownOption::new(custom_rule().id().as_str(), "Custom").disabled_for(
            DiagnosticCode::ERuleUnsupportedMvp,
            "Custom rule profiles are outside MVP2.",
        ),
    );
    rules
}

#[cfg(test)]
#[path = "profile_dropdowns_tests.rs"]
mod tests;
