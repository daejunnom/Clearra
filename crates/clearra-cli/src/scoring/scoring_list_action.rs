use clearra_scoring::profile::ScoreProfileRegistry;

use crate::{
    output::{CliOutput, RenderFormat},
    scoring::scoring_output_fields::{profile_fields, render_scoring},
};

pub(crate) struct ScoringListAction;

impl ScoringListAction {
    pub(crate) fn run(format: RenderFormat) -> CliOutput {
        let registry = ScoreProfileRegistry::builtins();
        let mut fields = vec![
            ("action".to_owned(), "list".to_owned()),
            (
                "profile_count".to_owned(),
                registry.profiles().len().to_string(),
            ),
        ];
        for (index, profile) in registry.profiles().iter().enumerate() {
            fields.extend(profile_fields(profile, Some(index)));
        }
        render_scoring(fields, format)
    }
}
